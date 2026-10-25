use crate::{download::Downloader, model::*, storage, version};
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tokio_util::sync::CancellationToken;

const META: &str = "https://meta.fabricmc.net/v2/versions";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}
#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderVersion,
}
#[derive(Deserialize, Serialize)]
struct GameVersion {
    version: String,
}

fn recent_cache<T: serde::de::DeserializeOwned>(path: &Path, max_age: u64) -> Option<T> {
    let age = std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()?;
    (age.as_secs() < max_age)
        .then(|| storage::read_json(path).ok())
        .flatten()
}

fn component(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 60
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
    {
        bail!("Fabric 版本号无效");
    }
    storage::safe_version(value)
}

pub fn version_id(game: &str, loader: &str) -> Result<String> {
    component(game)?;
    component(loader)?;
    let id = format!("fabric-loader-{loader}-{game}");
    storage::safe_version(&id)?;
    Ok(id)
}

pub async fn versions(
    root: &Path,
    game: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<Vec<LoaderVersion>> {
    match versions_from(root, game, downloader, cancel, META).await {
        Ok(versions) => Ok(versions),
        Err(error) => {
            crate::download::check_cancel(cancel)?;
            storage::safe_version(game)?;
            storage::read_json(
                &root
                    .join("metadata/fabric/loaders")
                    .join(format!("{game}.json")),
            )
            .map_err(|_| error)
        }
    }
}

async fn versions_from(
    root: &Path,
    game: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
    meta: &str,
) -> Result<Vec<LoaderVersion>> {
    storage::safe_version(game)?;
    crate::download::check_cancel(cancel)?;
    let cache = root
        .join("metadata/fabric/loaders")
        .join(format!("{game}.json"));
    if let Some(cached) = recent_cache(&cache, 3600) {
        return Ok(cached);
    }
    // The loader endpoint returns HTTP 400 for unsupported games. Consult the
    // authoritative compatibility catalog first instead of retrying a request
    // that cannot succeed. Fabric Meta refreshes this catalog every five minutes:
    // https://github.com/FabricMC/fabric-meta#v2versionsgame
    let games_cache = root.join("metadata/fabric/games.json");
    let games: Vec<GameVersion> = match recent_cache(&games_cache, 300) {
        Some(cached) => cached,
        None => {
            let games: Vec<GameVersion> = downloader.json(&format!("{meta}/game"), cancel).await?;
            storage::write_json(&games_cache, &games)?;
            games
        }
    };
    if !games.iter().any(|entry| entry.version == game) {
        return Ok(Vec::new());
    }
    component(game)?;
    let entries: Vec<LoaderEntry> = downloader
        .json(&format!("{meta}/loader/{game}"), cancel)
        .await?;
    let versions: Vec<_> = entries.into_iter().map(|e| e.loader).collect();
    storage::write_json(&cache, &versions)?;
    Ok(versions)
}

pub async fn profile(
    root: &Path,
    game: &str,
    loader: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<Value> {
    let id = version_id(game, loader)?;
    let cache = root
        .join("metadata/fabric/profiles")
        .join(format!("{id}.json"));
    let mut value: Value = match storage::read_json(&cache) {
        Ok(value) => value,
        Err(_) => {
            downloader
                .json(
                    &format!("{META}/loader/{game}/{loader}/profile/json"),
                    cancel,
                )
                .await?
        }
    };
    validate_profile(&value, game, loader)?;
    prepare_libraries(&mut value, downloader, cancel).await?;
    storage::write_json(&cache, &value)?;
    Ok(value)
}

fn validate_profile(value: &Value, game: &str, loader: &str) -> Result<()> {
    let id = version_id(game, loader)?;
    if value["id"].as_str() != Some(&id)
        || value["inheritsFrom"].as_str() != Some(game)
        || version::detect_loader(value)
            != Some(ModLoader {
                kind: "fabric".into(),
                version: loader.into(),
            })
    {
        bail!("Fabric 配置与所选游戏或加载器版本不一致");
    }
    Ok(())
}

pub(crate) async fn prepare_libraries(
    value: &mut Value,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<()> {
    let libraries = value["libraries"]
        .as_array_mut()
        .context("Fabric 配置缺少依赖")?;
    for library in libraries.iter_mut() {
        version::normalize_library(library)?;
    }
    let pending: Vec<_> = libraries
        .iter()
        .enumerate()
        .filter_map(|(index, library)| {
            let artifact = &library["downloads"]["artifact"];
            (artifact.is_object() && artifact["sha1"].as_str().unwrap_or("").is_empty())
                .then(|| (index, artifact["url"].as_str().unwrap_or("").to_owned()))
        })
        .collect();
    let hashes: Vec<_> =
        futures_util::stream::iter(pending.into_iter().map(|(index, url)| async move {
            Ok::<_, anyhow::Error>((index, downloader.maven_sha1(&url, cancel).await?))
        }))
        .buffer_unordered(4)
        .try_collect()
        .await?;
    for (index, hash) in hashes {
        libraries[index]["downloads"]["artifact"]["sha1"] = Value::String(hash);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        routes: Vec<(&'static str, &'static str, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = [0_u8; 4096];
                let mut request = Vec::new();
                loop {
                    let size = socket.read(&mut buffer).await.unwrap();
                    if size == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..size]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let path = String::from_utf8_lossy(&request)
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                recorded.lock().unwrap().push(path.clone());
                let (_, status, body) = routes
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .copied()
                    .unwrap_or(("", "500 Internal Server Error", "unexpected request"));
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (url, requests, task)
    }
    fn downloader(root: &Path) -> Downloader {
        Downloader::new(
            root,
            Settings {
                proxy_mode: "none".into(),
                download_source: "official".into(),
                retries: 0,
                ..Settings::default()
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn unsupported_games_skip_loader_requests_and_reuse_the_catalog() {
        let root = tempfile::tempdir().unwrap();
        let (meta, requests, server) = server(vec![
            ("/game", "200 OK", r#"[{"version":"1.20.1","stable":true}]"#),
            (
                "/loader/b1.8.1",
                "400 Bad Request",
                r#"{"error":"unsupported game"}"#,
            ),
            (
                "/loader/1.20.1",
                "200 OK",
                r#"[{"loader":{"version":"0.16.14","stable":true}}]"#,
            ),
        ])
        .await;
        let dl = downloader(root.path());
        let cancel = CancellationToken::new();
        for game in ["b1.8.1", "a1.2.6", "3D Shareware v1.34", "b1.8.1"] {
            assert!(
                versions_from(root.path(), game, &dl, &cancel, &meta)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(*requests.lock().unwrap(), ["/game"]);
        let versions = versions_from(root.path(), "1.20.1", &dl, &cancel, &meta)
            .await
            .unwrap();
        assert_eq!(versions[0].version, "0.16.14");
        assert!(versions[0].stable);
        assert_eq!(
            versions_from(root.path(), "1.20.1", &dl, &cancel, &meta)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(*requests.lock().unwrap(), ["/game", "/loader/1.20.1"]);
        server.abort();
    }

    #[tokio::test]
    async fn empty_loader_lists_are_unavailable_but_service_errors_are_not() {
        let root = tempfile::tempdir().unwrap();
        let (meta, _, server) = server(vec![
            (
                "/game",
                "200 OK",
                r#"[{"version":"1.20.1"},{"version":"1.21.1"}]"#,
            ),
            ("/loader/1.20.1", "200 OK", "[]"),
            ("/loader/1.21.1", "503 Service Unavailable", "offline"),
        ])
        .await;
        let dl = downloader(root.path());
        let cancel = CancellationToken::new();
        assert!(
            versions_from(root.path(), "1.20.1", &dl, &cancel, &meta)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            versions_from(root.path(), "1.21.1", &dl, &cancel, &meta)
                .await
                .unwrap_err()
                .to_string()
                .contains("503")
        );
        assert!(
            !root
                .path()
                .join("metadata/fabric/loaders/1.21.1.json")
                .exists()
        );
        server.abort();
    }

    #[tokio::test]
    async fn unavailable_or_invalid_catalog_is_not_cached_as_unsupported() {
        for (status, body) in [
            ("503 Service Unavailable", "offline"),
            ("200 OK", "not json"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let (meta, requests, server) = server(vec![("/game", status, body)]).await;
            let result = versions_from(
                root.path(),
                "b1.8.1",
                &downloader(root.path()),
                &CancellationToken::new(),
                &meta,
            )
            .await;
            assert!(result.is_err());
            assert_eq!(*requests.lock().unwrap(), ["/game"]);
            assert!(!root.path().join("metadata/fabric/games.json").exists());
            server.abort();
        }
    }
}
