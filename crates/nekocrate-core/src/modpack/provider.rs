use super::*;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::sync::mpsc;

// Bulk endpoints replace two serial HTTP requests per manifest entry. Keep
// responses bounded and checkpoint each batch before handing it to downloads.
const BATCH_SIZE: usize = 100;
struct Api<'a> {
    client: &'a reqwest::Client,
    key: &'a str,
    base: &'a str,
}
impl Api<'_> {
    async fn request(&self, path: &str, body: Value, cancel: &CancellationToken) -> Result<Value> {
        let mut header = reqwest::header::HeaderValue::from_str(self.key)
            .context(crate::curseforge::UNAVAILABLE)?;
        header.set_sensitive(true);
        tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            response = async {
                let mut response = self.client.post(format!("{}{path}", self.base))
                    .header("x-api-key", header).header("Accept-Encoding", "gzip").json(&body).timeout(Duration::from_secs(30)).send().await?;
                if matches!(response.status().as_u16(), 401 | 403) { bail!(crate::curseforge::ACCESS_DENIED); }
                response = response.error_for_status()?;
                let compressed = response.headers().get(reqwest::header::CONTENT_ENCODING)
                    .and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("gzip"));
                let mut data = Vec::new();
                while let Some(chunk) = response.chunk().await.with_context(|| format!("接收 CurseForge 元数据失败（已接收 {} 字节）", data.len()))? {
                    if data.len() + chunk.len() > 8 * 1024 * 1024 { bail!("CurseForge 元数据过大"); }
                    data.extend(chunk);
                }
                if compressed {
                    use std::io::Read;
                    let mut decoded = Vec::new();
                    flate2::read::GzDecoder::new(data.as_slice()).take(8 * 1024 * 1024 + 1).read_to_end(&mut decoded)?;
                    if decoded.len() > 8 * 1024 * 1024 { bail!("CurseForge 元数据过大"); }
                    data = decoded;
                }
                Ok::<_,anyhow::Error>(serde_json::from_slice::<Value>(&data)?["data"].clone())
            } => response,
        }
    }
}

pub(super) fn progress(
    sink: &EventSink,
    completed: usize,
    total: usize,
    message: impl Into<String>,
) {
    sink(Event {
        completed,
        total,
        ..Event::new("workflow_progress", message)
    });
}

pub(super) async fn resolve(
    pack: &mut Pack,
    key: Option<String>,
    client: &reqwest::Client,
    cancel: &CancellationToken,
    sink: &EventSink,
    batches: Option<&mpsc::Sender<Vec<PackFile>>>,
    checkpoint: impl FnMut(&Pack) -> Result<()>,
) -> Result<()> {
    let api = Api {
        client,
        key: key.as_deref().unwrap_or_default(),
        base: "https://api.curseforge.com/v1/",
    };
    resolve_with_api(pack, &api, cancel, sink, batches, checkpoint).await
}

async fn publish(
    batches: Option<&mpsc::Sender<Vec<PackFile>>>,
    files: Vec<PackFile>,
    cancel: &CancellationToken,
) -> Result<()> {
    if let Some(sender) = batches {
        tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            result = sender.send(files) => result.context("整合包下载队列已关闭")?,
        }
    }
    Ok(())
}

async fn resolve_with_api(
    pack: &mut Pack,
    api: &Api<'_>,
    cancel: &CancellationToken,
    sink: &EventSink,
    batches: Option<&mpsc::Sender<Vec<PackFile>>>,
    mut checkpoint: impl FnMut(&Pack) -> Result<()>,
) -> Result<()> {
    crate::download::check_cancel(cancel)?;
    let pending: Vec<_> = pack
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.path.is_none())
        .map(|(index, _)| index)
        .collect();
    let mut completed = pack.files.len() - pending.len();
    progress(
        sink,
        completed,
        pack.files.len(),
        if completed > 0 {
            "复用已解析的文件信息"
        } else {
            "准备获取整合包文件信息"
        },
    );
    // Resuming can start downloading already-resolved files before any API call.
    for chunk in pack
        .files
        .iter()
        .filter(|file| file.path.is_some())
        .cloned()
        .collect::<Vec<_>>()
        .chunks(BATCH_SIZE)
    {
        publish(batches, chunk.to_vec(), cancel).await?;
    }
    if pack.format != "curseforge" || pending.is_empty() {
        return Ok(());
    }
    if api.key.is_empty() {
        bail!(crate::curseforge::UNAVAILABLE);
    }
    let count = pending.len().div_ceil(BATCH_SIZE);
    for (batch, indices) in pending.chunks(BATCH_SIZE).enumerate() {
        crate::download::check_cancel(cancel)?;
        let projects: HashSet<_> = indices
            .iter()
            .map(|&i| pack.files[i].project.context("缺少 CurseForge 项目 ID"))
            .collect::<Result<_>>()?;
        let files: HashSet<_> = indices
            .iter()
            .map(|&i| pack.files[i].file.context("缺少 CurseForge 文件 ID"))
            .collect::<Result<_>>()?;
        let label = format!("获取项目资料与下载地址（第 {} / {count} 批）", batch + 1);
        progress(sink, completed, pack.files.len(), &label);
        let query = async {
            tokio::try_join!(
                api.request("mods", serde_json::json!({"modIds": projects}), cancel),
                api.request("mods/files", serde_json::json!({"fileIds": files}), cancel),
            )
        };
        tokio::pin!(query);
        let started = tokio::time::Instant::now();
        let mut heartbeat =
            tokio::time::interval_at(started + Duration::from_secs(1), Duration::from_secs(1));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let result = loop {
            tokio::select! {
                result = &mut query => break result,
                _ = heartbeat.tick() => progress(sink, completed, pack.files.len(), format!("等待 CurseForge 响应（第 {} / {count} 批，{} 秒）", batch+1, started.elapsed().as_secs())),
            }
        };
        let (info, file) = result
            .with_context(|| format!("获取整合包依赖失败（第 {} / {count} 批）", batch + 1))?;
        let info = indexed(info, &projects)?;
        let file = indexed(file, &files)?;
        let mut ready = Vec::new();
        let mut first_error = None;
        for &index in indices {
            let item = &pack.files[index];
            let project = item.project.unwrap();
            let file_id = item.file.unwrap();
            let result = (|| {
                let info = info
                    .get(&project)
                    .with_context(|| format!("CurseForge 项目不可用：{project}"))?;
                let file = file
                    .get(&file_id)
                    .with_context(|| format!("CurseForge 文件不可用：{project}/{file_id}"))?;
                resolve_file(item.clone(), info, file)
            })();
            match result {
                Ok(item) => {
                    pack.files[index] = item.clone();
                    ready.push(item);
                    completed += 1;
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        // Even a partial response keeps valid entries, so retrying only resolves
        // the unavailable entries. A malformed identity is never trusted.
        if !ready.is_empty() {
            checkpoint(pack)?;
            progress(
                sink,
                completed,
                pack.files.len(),
                format!(
                    "已解析：{}",
                    ready
                        .last()
                        .and_then(|f| f.display_name.as_deref())
                        .unwrap_or("CurseForge")
                ),
            );
            publish(batches, ready, cancel).await?;
        }
        if let Some(error) = first_error {
            return Err(error);
        }
    }
    Ok(())
}

fn indexed(value: Value, expected: &HashSet<u64>) -> Result<HashMap<u64, Value>> {
    let mut result = HashMap::new();
    for item in value.as_array().context("CurseForge 批量元数据格式无效")? {
        let id = item["id"].as_u64().context("CurseForge 元数据缺少 ID")?;
        if !expected.contains(&id) || result.insert(id, item.clone()).is_some() {
            bail!("CurseForge 返回了不匹配或重复的文件信息");
        }
    }
    Ok(result)
}

fn resolve_file(mut item: PackFile, info: &Value, file: &Value) -> Result<PackFile> {
    let project = item.project.context("缺少 CurseForge 项目 ID")?;
    let file_id = item.file.context("缺少 CurseForge 文件 ID")?;
    if file["id"].as_u64() != Some(file_id) || file["modId"].as_u64() != Some(project) {
        bail!("CurseForge 返回了不匹配的文件");
    }
    if info["id"].as_u64() != Some(project) || info["gameId"] != 432 {
        bail!("CurseForge 项目不是 Minecraft 项目");
    }
    let folder = match info["classId"].as_u64() {
        Some(6) => "mods",
        Some(12) => "resourcepacks",
        Some(6552) => "shaderpacks",
        _ => bail!("此 CurseForge 文件类型暂不支持导入：{project}"),
    };
    item.display_name = info["name"].as_str().map(str::to_owned);
    let name = file["fileName"].as_str().context("CurseForge 文件名缺失")?;
    archive::relative(name)?;
    if name.contains('/') {
        bail!("CurseForge 文件名无效");
    }
    item.path = Some(format!("{folder}/{name}"));
    item.size = file["fileLength"]
        .as_u64()
        .context("CurseForge 文件大小缺失")?;
    let hashes = file["hashes"]
        .as_array()
        .context("CurseForge 文件缺少校验值")?;
    let sha1 = hashes
        .iter()
        .find(|h| h["algo"] == 1)
        .context("CurseForge 文件缺少 SHA-1 校验值")?;
    item.sha1 = archive::hash(sha1, "value", 40)?;
    item.urls.clear();
    if let Some(url) = file["downloadUrl"]
        .as_str()
        .filter(|u| !u.is_empty() && info["allowModDistribution"] != false)
    {
        archive::https(url)?;
        item.urls = vec![url.into()];
    }
    if let Some(url) = info["links"]["websiteUrl"].as_str() {
        let parsed = reqwest::Url::parse(url)?;
        if parsed.scheme() == "https" && parsed.host_str() == Some("www.curseforge.com") {
            item.manual_url = Some(format!("{}/files/{file_id}", url.trim_end_matches('/')));
        }
    }
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn pack(count: usize) -> Pack {
        Pack {
            format: "curseforge".into(),
            name: "Fixture".into(),
            version: "1".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            overrides: vec![],
            manifest: Value::Null,
            files: (1..=count as u64)
                .map(|id| PackFile {
                    key: id.to_string(),
                    display_name: None,
                    path: None,
                    urls: vec![],
                    sha1: String::new(),
                    sha512: String::new(),
                    size: 0,
                    optional: false,
                    project: Some(id),
                    file: Some(id + 1000),
                    manual_url: None,
                })
                .collect(),
        }
    }
    async fn server(
        missing: Option<u64>,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        let handle = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let counter = counter.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buf = [0; 4096];
                    let (head, body) = loop {
                        let n = socket.read(&mut buf).await.unwrap();
                        assert!(n > 0);
                        data.extend(&buf[..n]);
                        if let Some(end) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&data[..end]).into_owned();
                            let size: usize = head
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .map(str::parse)
                                })
                                .unwrap()
                                .unwrap();
                            if data.len() >= end + 4 + size {
                                break (
                                    head,
                                    serde_json::from_slice::<Value>(&data[end + 4..end + 4 + size])
                                        .unwrap(),
                                );
                            }
                        }
                    };
                    counter.fetch_add(1, Ordering::SeqCst);
                    let is_files = head.starts_with("POST /mods/files ");
                    assert!(is_files || head.starts_with("POST /mods "));
                    let ids = body[if is_files { "fileIds" } else { "modIds" }]
                        .as_array()
                        .unwrap();
                    assert!(ids.len() <= BATCH_SIZE);
                    let mut entries: Vec<_> = ids.iter().filter_map(|v| {
                        let id = v.as_u64().unwrap();
                        if is_files && missing == Some(id) { return None; }
                        Some(if is_files { json!({"id":id,"modId":id-1000,"fileName":format!("{id}.jar"),"fileLength":3,"hashes":[{"algo":1,"value":"a".repeat(40)}],"downloadUrl":format!("https://edge.forgecdn.net/{id}.jar")}) }
                        else { json!({"id":id,"gameId":432,"classId":if id % 3 == 0 {12}else{6},"name":format!("Project {id}"),"allowModDistribution":id!=2,"links":{"websiteUrl":format!("https://www.curseforge.com/minecraft/mc-mods/{id}")}}) })
                    }).collect();
                    entries.reverse(); // API response ordering is not manifest ordering.
                    let body = json!({"data":entries}).to_string();
                    assert!(head.to_ascii_lowercase().contains("accept-encoding: gzip"));
                    use std::io::Write;
                    let mut encoder =
                        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                    encoder.write_all(body.as_bytes()).unwrap();
                    let body = encoder.finish().unwrap();
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                });
            }
        });
        (base, requests, handle)
    }
    #[tokio::test]
    async fn bulk_resolution_checkpoints_and_streams_without_redoing_resolved_entries() {
        let (base, requests, server) = server(None).await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let api = Api {
            client: &client,
            key: "fixture",
            base: &base,
        };
        let mut pack = pack(435);
        let mut checkpoints = 0;
        let sink: EventSink = Arc::new(|_| {});
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(1);
        let resolver = async {
            let tx = tx;
            resolve_with_api(&mut pack, &api, &cancel, &sink, Some(&tx), |_| {
                checkpoints += 1;
                Ok(())
            })
            .await
            .unwrap();
        };
        let consumer = async {
            let mut count = 0;
            while let Some(files) = rx.recv().await {
                count += files.len();
            }
            count
        };
        let (_, count) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(resolver, consumer)
        })
        .await
        .unwrap();
        assert_eq!(count, 435);
        assert_eq!(requests.load(Ordering::SeqCst), 10);
        assert_eq!(checkpoints, 5);
        assert_eq!(pack.files[0].path.as_deref(), Some("mods/1001.jar"));
        assert_eq!(
            pack.files[2].path.as_deref(),
            Some("resourcepacks/1003.jar")
        );
        assert!(pack.files[1].urls.is_empty());
        assert!(pack.files[1].manual_url.is_some());
        resolve_with_api(&mut pack, &api, &cancel, &sink, None, |_| {
            panic!("resolved metadata must be reused")
        })
        .await
        .unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 10);
        server.abort();
    }
    #[tokio::test]
    async fn partial_bulk_response_keeps_successes_and_cancel_preserves_checkpoint() {
        let (base, _, server) = server(Some(1002)).await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let api = Api {
            client: &client,
            key: "fixture",
            base: &base,
        };
        let mut pack = pack(3);
        let sink: EventSink = Arc::new(|_| {});
        let mut saved = 0;
        let error = resolve_with_api(
            &mut pack,
            &api,
            &CancellationToken::new(),
            &sink,
            None,
            |pack| {
                saved = pack.files.iter().filter(|f| f.path.is_some()).count();
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("2/1002"));
        assert_eq!(saved, 2);
        assert!(pack.files[1].path.is_none());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            resolve_with_api(&mut pack, &api, &cancel, &sink, None, |_| panic!())
                .await
                .is_err()
        );
        assert_eq!(pack.files.iter().filter(|f| f.path.is_some()).count(), 2);
        server.abort();
    }
    #[test]
    fn bulk_response_rejects_identity_mismatch_and_unsafe_paths() {
        let item = pack(1).files.remove(0);
        let info = json!({"id":1,"gameId":432,"classId":6});
        let mut file = json!({"id":1001,"modId":2});
        assert!(resolve_file(item.clone(), &info, &file).is_err());
        file["modId"] = json!(1);
        file["fileName"] = json!("../escape.jar");
        assert!(resolve_file(item, &info, &file).is_err());
        assert!(indexed(json!([{"id":1},{"id":1}]), &HashSet::from([1])).is_err());
        assert!(indexed(json!([{"id":2}]), &HashSet::from([1])).is_err());
    }
    #[test]
    fn shader_entries_in_modpacks_keep_their_own_destination() {
        let info = json!({"id":1,"gameId":432,"classId":6552});
        let file = json!({"id":1001,"modId":1,"fileName":"MakeUp.zip","fileLength":3,"hashes":[{"algo":1,"value":"a".repeat(40)}],"downloadUrl":"https://edge.forgecdn.net/MakeUp.zip"});
        let item = resolve_file(pack(1).files.remove(0), &info, &file).unwrap();
        assert_eq!(item.path.as_deref(), Some("shaderpacks/MakeUp.zip"));
    }
    #[tokio::test]
    #[ignore = "read-only live CurseForge bulk API; requires a local manifest and credential"]
    async fn live_bulk_resolves_manifest_copy() {
        let source = std::env::var("NEKOCRATE_TEST_CF_MANIFEST").unwrap();
        let value: Value = storage::read_json(Path::new(&source)).unwrap();
        let mut pack: Pack = serde_json::from_value(value["pack"].clone()).unwrap();
        for file in &mut pack.files {
            file.path = None;
        }
        let key = std::env::var("CURSEFORGE_API_KEY").unwrap();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .http1_only()
            .build()
            .unwrap();
        let sink: EventSink = Arc::new(|event| {
            if event.kind == "workflow_progress" {
                eprintln!("{} / {}: {}", event.completed, event.total, event.message);
            }
        });
        let started = std::time::Instant::now();
        resolve(
            &mut pack,
            Some(key),
            &client,
            &CancellationToken::new(),
            &sink,
            None,
            |_| Ok(()),
        )
        .await
        .unwrap();
        eprintln!(
            "Resolved {} entries in {:.2}s",
            pack.files.len(),
            started.elapsed().as_secs_f64()
        );
        assert!(pack.files.iter().all(|f| f.path.is_some()));
    }
}
