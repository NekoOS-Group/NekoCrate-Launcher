use crate::model::{Event, EventSink, Settings};
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct FileSpec {
    pub url: String,
    pub path: PathBuf,
    pub hash: String,
    pub sha256: bool,
    pub size: u64,
}

#[derive(Clone)]
pub struct Downloader {
    pub client: reqwest::Client,
    cache: PathBuf,
    settings: Settings,
    locks: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>>,
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Downloader {
    pub fn new(root: &Path, settings: Settings) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent("NekoCrate/0.1.0")
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(300))
                .build()?,
            cache: root.join("cache/objects"),
            settings,
            locks: Arc::new(Mutex::new(Default::default())),
        })
    }
    pub fn urls(&self, url: &str) -> Vec<String> {
        let mirror = [
            "https://piston-meta.mojang.com",
            "https://piston-data.mojang.com",
            "https://launchermeta.mojang.com",
            "https://launcher.mojang.com",
        ]
        .iter()
        .find_map(|prefix| {
            url.strip_prefix(prefix)
                .map(|p| format!("https://bmclapi2.bangbang93.com{p}"))
        })
        .or_else(|| {
            url.strip_prefix("https://resources.download.minecraft.net")
                .map(|p| format!("https://bmclapi2.bangbang93.com/assets{p}"))
        })
        .or_else(|| {
            url.strip_prefix("https://libraries.minecraft.net")
                .map(|p| format!("https://bmclapi2.bangbang93.com/maven{p}"))
        });
        match (self.settings.download_source.as_str(), mirror) {
            ("bmclapi", Some(m)) => vec![m, url.into()],
            ("auto", Some(m)) => vec![url.into(), m],
            _ => vec![url.into()],
        }
    }
    pub async fn json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<T> {
        let mut last = String::new();
        for candidate in self.urls(url) {
            for attempt in 0..=self.settings.retries {
                let result: Result<T> = tokio::select! {
                    _=cancel.cancelled()=>bail!("操作已取消"),
                    r=async { Ok(self.client.get(&candidate).timeout(Duration::from_secs(30)).send().await?.error_for_status()?.json().await?) }=>r,
                };
                match result {
                    Ok(v) => return Ok(v),
                    Err(e) => last = format!("{e:#}"),
                }
                if attempt < self.settings.retries {
                    tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), _=tokio::time::sleep(Duration::from_millis(300 * (1<<attempt)))=>{} }
                }
            }
        }
        bail!("无法获取元数据，请检查网络或切换下载源：{last}")
    }
    pub async fn verified(path: &Path, hash: &str, sha256: bool, size: u64) -> Result<bool> {
        let Ok(meta) = tokio::fs::symlink_metadata(path).await else {
            return Ok(false);
        };
        if !meta.is_file() || (size > 0 && meta.len() != size) {
            return Ok(false);
        }
        let mut file = tokio::fs::File::open(path).await?;
        let mut a = Sha1::new();
        let mut b = Sha256::new();
        let mut buf = vec![0u8; 65536];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            if sha256 {
                b.update(&buf[..n]);
            } else {
                a.update(&buf[..n]);
            }
        }
        Ok(if sha256 {
            hex::encode(b.finalize())
        } else {
            hex::encode(a.finalize())
        }
        .eq_ignore_ascii_case(hash))
    }
    pub async fn file(
        &self,
        file: &FileSpec,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        let expected = if file.sha256 { 64 } else { 40 };
        if file.hash.len() != expected || !file.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("资源校验值无效");
        }
        let key = format!("{}-{}", expected, file.hash);
        let lock = self
            .locks
            .lock()
            .await
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard =
            tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), guard=lock.lock()=>guard };
        if Self::verified(&file.path, &file.hash, file.sha256, file.size).await? {
            return Ok(());
        }
        let cache = self
            .cache
            .join(if file.sha256 { "sha256" } else { "sha1" })
            .join(&file.hash[..2])
            .join(&file.hash);
        if !Self::verified(&cache, &file.hash, file.sha256, file.size).await? {
            tokio::fs::create_dir_all(cache.parent().unwrap()).await?;
            let mut errors = vec![];
            let mut done = false;
            'sources: for url in self.urls(&file.url) {
                for attempt in 0..=self.settings.retries {
                    let part =
                        Partial(cache.with_extension(format!("{}.part", uuid::Uuid::new_v4())));
                    let download = async {
                        let response = self.client.get(&url).send().await?.error_for_status()?;
                        let mut stream = response.bytes_stream();
                        let mut out = tokio::fs::File::create(&part.0).await?;
                        let mut bytes = 0;
                        let mut last = std::time::Instant::now();
                        while let Some(chunk) = stream.next().await {
                            let chunk = chunk?;
                            bytes += chunk.len() as u64;
                            if file.size > 0 && bytes > file.size {
                                bail!("文件大小超过元数据声明");
                            }
                            out.write_all(&chunk).await?;
                            if last.elapsed() > Duration::from_millis(200) {
                                let mut e = Event::new(
                                    "transfer",
                                    file.path.file_name().unwrap_or_default().to_string_lossy(),
                                );
                                e.bytes = bytes;
                                sink(e);
                                last = std::time::Instant::now();
                            }
                        }
                        out.flush().await?;
                        drop(out);
                        if !Self::verified(&part.0, &file.hash, file.sha256, file.size).await? {
                            bail!("下载文件校验失败");
                        }
                        Ok::<_, anyhow::Error>(())
                    };
                    match tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), r=download=>r }
                    {
                        Ok(()) => {
                            if cache.exists() {
                                tokio::fs::remove_file(&cache).await?;
                            }
                            tokio::fs::rename(&part.0, &cache).await?;
                            done = true;
                            break 'sources;
                        }
                        Err(e) => {
                            errors.push(format!("{e:#}"));
                            sink(Event::new(
                                "retry",
                                format!(
                                    "重试 {}（{}/{}）",
                                    file.path.file_name().unwrap_or_default().to_string_lossy(),
                                    attempt + 1,
                                    self.settings.retries + 1
                                ),
                            ));
                        }
                    }
                    tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), _=tokio::time::sleep(Duration::from_millis(300*(1<<attempt)))=>{} }
                }
            }
            if !done {
                bail!("下载失败 {}：{}", file.path.display(), errors.join("；"));
            }
        }
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        tokio::fs::create_dir_all(file.path.parent().context("无效下载目标")?).await?;
        let temp = Partial(
            file.path
                .with_extension(format!("{}.part", uuid::Uuid::new_v4())),
        );
        tokio::fs::copy(&cache, &temp.0).await?;
        if file.path.exists() {
            tokio::fs::remove_file(&file.path).await?;
        }
        tokio::fs::rename(&temp.0, &file.path).await?;
        Ok(())
    }
    pub async fn batch(
        &self,
        files: Vec<FileSpec>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let mut unique = std::collections::BTreeMap::new();
        for f in files {
            unique.insert(f.path.clone(), f);
        }
        let total = unique.len();
        let completed = AtomicUsize::new(0);
        let complete = &completed;
        futures_util::stream::iter(unique.into_values().map(|file| async move {
            self.file(&file, cancel, sink).await?;
            let mut e = Event::new(
                "progress",
                file.path.file_name().unwrap_or_default().to_string_lossy(),
            );
            e.completed = complete.fetch_add(1, Ordering::SeqCst) + 1;
            e.total = total;
            sink(e);
            Ok::<_, anyhow::Error>(())
        }))
        .buffer_unordered(self.settings.concurrency.clamp(1, 32))
        .try_collect::<Vec<_>>()
        .await?;
        Ok(())
    }
}

pub fn artifact_file(a: &crate::model::Artifact, path: PathBuf) -> FileSpec {
    FileSpec {
        url: a.url.clone(),
        path,
        hash: a.sha1.clone(),
        sha256: false,
        size: a.size,
    }
}
pub fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("操作已取消")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn retries_http_failure_and_rejects_corrupt_response() {
        for valid in [true, false] {
            let t = tempfile::tempdir().unwrap();
            let settings = Settings {
                retries: if valid { 1 } else { 0 },
                ..Settings::default()
            };
            let d = Downloader::new(t.path(), settings).unwrap();
            let content = "verified";
            let replies = if valid {
                vec![(503, "retry".into()), (200, content.into())]
            } else {
                vec![(200, "tampered".into())]
            };
            let (url, server) = crate::test_http::serve(replies).await;
            let file = FileSpec {
                url,
                path: t.path().join("game/test.jar"),
                hash: hex::encode(Sha1::digest(content)),
                sha256: false,
                size: content.len() as u64,
            };
            let sink: EventSink = Arc::new(|_| {});
            let result = d.file(&file, &CancellationToken::new(), &sink).await;
            let requests = server.await.unwrap();
            if valid {
                result.unwrap();
                assert_eq!(requests.len(), 2);
                assert_eq!(std::fs::read(&file.path).unwrap(), content.as_bytes());
            } else {
                assert!(result.unwrap_err().to_string().contains("校验失败"));
                assert!(!file.path.exists());
                assert!(
                    walkdir::WalkDir::new(t.path().join("cache"))
                        .into_iter()
                        .all(|e| !e.unwrap().file_type().is_file())
                );
            }
        }
    }

    #[tokio::test]
    async fn cancelling_stream_removes_partial_file() {
        use tokio::net::TcpListener;
        let t = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let started = Arc::new(tokio::sync::Notify::new());
        let notify = started.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\nx")
                .await
                .unwrap();
            notify.notify_one();
            std::future::pending::<()>().await;
        });
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let cancellation = tokio::spawn(async move {
            started.notified().await;
            token.cancel();
        });
        let d = Downloader::new(t.path(), Settings::default()).unwrap();
        let file = FileSpec {
            url,
            path: t.path().join("game/test.jar"),
            hash: "a".repeat(40),
            sha256: false,
            size: 1024,
        };
        let sink: EventSink = Arc::new(|_| {});
        let result = tokio::time::timeout(Duration::from_secs(3), d.file(&file, &cancel, &sink))
            .await
            .unwrap();
        assert!(result.unwrap_err().to_string().contains("取消"));
        assert!(!file.path.exists());
        assert!(
            walkdir::WalkDir::new(t.path()).into_iter().all(|e| !e
                .unwrap()
                .path()
                .to_string_lossy()
                .ends_with(".part"))
        );
        cancellation.await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn detects_corruption_and_uses_cache_without_network() {
        let t = tempfile::tempdir().unwrap();
        let d = Downloader::new(t.path(), Settings::default()).unwrap();
        let bytes = b"verified content";
        let hash = hex::encode(Sha1::digest(bytes));
        let cached = t
            .path()
            .join("cache/objects/sha1")
            .join(&hash[..2])
            .join(&hash);
        crate::storage::atomic_write(&cached, bytes).unwrap();
        let f = FileSpec {
            url: "http://127.0.0.1:1/no-network".into(),
            path: t.path().join("game/library.jar"),
            hash,
            sha256: false,
            size: bytes.len() as u64,
        };
        let sink: EventSink = Arc::new(|_| {});
        d.file(&f, &CancellationToken::new(), &sink).await.unwrap();
        tokio::fs::write(&f.path, b"broken").await.unwrap();
        d.file(&f, &CancellationToken::new(), &sink).await.unwrap();
        assert_eq!(tokio::fs::read(&f.path).await.unwrap(), bytes);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(d.file(&f, &cancel, &sink).await.is_err());
    }
}
