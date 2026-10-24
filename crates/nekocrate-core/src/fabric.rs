use crate::{download::Downloader, model::*, storage, version};
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tokio_util::sync::CancellationToken;

const META: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}
#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderVersion,
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
    component(game)?;
    let cache = root
        .join("metadata/fabric/loaders")
        .join(format!("{game}.json"));
    if std::fs::metadata(&cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age.as_secs() < 3600)
        && let Ok(cached) = storage::read_json(&cache)
    {
        return Ok(cached);
    }
    let entries: Vec<LoaderEntry> = downloader.json(&format!("{META}/{game}"), cancel).await?;
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
                .json(&format!("{META}/{game}/{loader}/profile/json"), cancel)
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
