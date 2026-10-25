use super::*;
use serde_json::Value;
use std::{cmp::Ordering, collections::BTreeMap};

const FORGE: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json";
const PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const NEOFORGE_LEGACY: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
}

#[derive(Serialize, Deserialize)]
struct NeoCatalog {
    versions: Vec<String>,
}

async fn cached_json<T: serde::de::DeserializeOwned + Serialize>(
    root: &Path,
    name: &str,
    url: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<T> {
    let path = root.join("metadata/loaders").join(format!("{name}.json"));
    let cached = storage::read_json::<T>(&path).ok();
    let fresh = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age.as_secs() < 3600);
    if fresh && let Some(value) = cached {
        return Ok(value);
    }
    match downloader.json::<T>(url, cancel).await {
        Ok(value) => {
            storage::write_json(&path, &value)?;
            Ok(value)
        }
        Err(error) => {
            crate::download::check_cancel(cancel)?;
            cached.ok_or(error)
        }
    }
}

pub(super) async fn versions(
    root: &Path,
    game: &str,
    kind: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<Vec<LoaderVersion>> {
    validate_kind(kind)?;
    crate::archive_support::component(game)?;
    crate::download::check_cancel(cancel)?;
    let mut result = match kind {
        "fabric" => crate::fabric::versions(root, game, downloader, cancel)
            .await?
            .into_iter()
            .map(|v| LoaderVersion {
                version: v.version,
                stable: v.stable,
                recommended: false,
            })
            .collect(),
        "forge" => {
            // Earlier Forge distributions have no supported installer profile.
            if game.starts_with("1.")
                && game
                    .split('.')
                    .nth(1)
                    .and_then(|v| v.parse::<u32>().ok())
                    .is_some_and(|v| v < 6)
            {
                return Ok(vec![]);
            }
            let versions: BTreeMap<String, Vec<String>> =
                cached_json(root, "forge", FORGE, downloader, cancel).await?;
            let promotions: Value =
                cached_json(root, "forge-promotions", PROMOTIONS, downloader, cancel)
                    .await
                    .unwrap_or(Value::Null);
            crate::download::check_cancel(cancel)?;
            forge_versions(game, &versions, &promotions)
        }
        "neoforge" => {
            let legacy = game == "1.20.1";
            let value: NeoCatalog = cached_json(
                root,
                if legacy {
                    "neoforge-legacy"
                } else {
                    "neoforge"
                },
                if legacy { NEOFORGE_LEGACY } else { NEOFORGE },
                downloader,
                cancel,
            )
            .await?;
            value
                .versions
                .iter()
                .map(String::as_str)
                .filter_map(|v| {
                    if legacy {
                        v.strip_prefix("1.20.1-")
                    } else {
                        (neo_game(v).as_deref() == Some(game)).then_some(v)
                    }
                })
                .map(|v| LoaderVersion {
                    version: v.into(),
                    stable: !v.contains("-beta") && !v.contains("-alpha"),
                    recommended: false,
                })
                .collect()
        }
        _ => unreachable!(),
    };
    result.retain(|v| crate::archive_support::component(&v.version).is_ok());
    result.sort_by(|a, b| natural_cmp(&b.version, &a.version));
    result.dedup_by(|a, b| a.version == b.version);
    Ok(result)
}

fn forge_versions(
    game: &str,
    versions: &BTreeMap<String, Vec<String>>,
    promotions: &Value,
) -> Vec<LoaderVersion> {
    let recommended = promotions["promos"][format!("{game}-recommended")].as_str();
    versions
        .get(game)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let version = value.strip_prefix(&format!("{game}-"))?;
            Some(LoaderVersion {
                version: version.into(),
                // Forge supplies a recommended build, not a stable bit for every entry.
                stable: false,
                recommended: recommended
                    .is_some_and(|r| version == r || version == format!("{r}-{game}")),
            })
        })
        .collect()
}

// NeoForged's Maven version scheme: 20.2.x -> 1.20.2; 26.1.2.x -> 26.1.2.
// Snapshot suffixes are part of the game version; never offer those builds for a release.
fn neo_game(version: &str) -> Option<String> {
    let (base, extra) = version
        .split_once('+')
        .map_or((version, None), |(a, b)| (a, Some(b)));
    let parts: Vec<_> = base.split('-').next()?.split('.').collect();
    let major = parts.first()?.parse::<u32>().ok()?;
    if major == 0 {
        return parts.get(1).map(|s| (*s).into());
    }
    let minor = parts.get(1)?.parse::<u32>().ok()?;
    let mut game = if major >= 26 {
        let patch = parts.get(2)?.parse::<u32>().ok()?;
        if patch == 0 {
            format!("{major}.{minor}")
        } else {
            format!("{major}.{minor}.{patch}")
        }
    } else if major >= 20 {
        if minor == 0 {
            format!("1.{major}")
        } else {
            format!("1.{major}.{minor}")
        }
    } else {
        return None;
    };
    if let Some(extra) = extra {
        game.push('-');
        game.push_str(extra);
    }
    Some(game)
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let tokens = |s: &str| -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut digit = false;
        for ch in s.chars() {
            if !current.is_empty() && ch.is_ascii_digit() != digit {
                tokens.push(std::mem::take(&mut current));
            }
            digit = ch.is_ascii_digit();
            current.push(ch);
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    };
    for (a, b) in tokens(a).iter().zip(tokens(b).iter()) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => a.cmp(b),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Reads the public loader catalogs through the system proxy; never opens user data"]
    async fn public_loader_catalog_smoke() {
        let temp = tempfile::tempdir().unwrap();
        let downloader = Downloader::new(
            temp.path(),
            Settings {
                download_source: "official".into(),
                retries: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let cases = [
            ("1.20.1", "fabric", "0."),
            ("1.20.1", "forge", "47."),
            ("1.21.1", "neoforge", "21.1."),
            ("1.20.1", "neoforge", "47."),
        ];
        let results = futures_util::future::join_all(cases.iter().map(|(game, kind, _)| async {
            versions(
                temp.path(),
                game,
                kind,
                &downloader,
                &CancellationToken::new(),
            )
            .await
        }))
        .await;
        let mut failures = Vec::new();
        for ((game, kind, expected), result) in cases.into_iter().zip(results) {
            match result {
                Ok(result) => {
                    assert!(!result.is_empty(), "{game} {kind}");
                    assert!(
                        result.iter().all(|v| v.version.starts_with(expected)),
                        "{game} {kind}"
                    );
                    eprintln!(
                        "{game} {kind}: {} builds, newest {}",
                        result.len(),
                        result[0].version
                    );
                }
                Err(error) => failures.push(format!("{game} {kind}: {error:#}")),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
    #[tokio::test]
    async fn legacy_neoforge_uses_the_minecraft_prefixed_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        storage::write_json(
            &temp.path().join("metadata/loaders/neoforge-legacy.json"),
            &serde_json::json!({"versions":["1.20.1-47.1.106","1.20.1-47.1.9","47.1.82"]}),
        )
        .unwrap();
        let downloader = Downloader::new(temp.path(), Settings::default()).unwrap();
        let result = versions(
            temp.path(),
            "1.20.1",
            "neoforge",
            &downloader,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            result
                .iter()
                .map(|v| v.version.as_str())
                .collect::<Vec<_>>(),
            ["47.1.106", "47.1.9"]
        );
    }
    #[tokio::test]
    async fn catalog_cache_survives_network_errors_but_not_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings {
            proxy_mode: "none".into(),
            download_source: "official".into(),
            retries: 0,
            ..Default::default()
        };
        let downloader = Downloader::new(temp.path(), settings).unwrap();
        let (url, server) = crate::test_http::serve(vec![
            (200, "[\"47.4.4\"]".into()),
            (503, "unavailable".into()),
            (503, "unavailable".into()),
        ])
        .await;
        let cancel = CancellationToken::new();
        let first: Vec<String> = cached_json(temp.path(), "test", &url, &downloader, &cancel)
            .await
            .unwrap();
        assert_eq!(first, vec!["47.4.4"]);
        let again: Vec<String> = cached_json(temp.path(), "test", &url, &downloader, &cancel)
            .await
            .unwrap();
        assert_eq!(again, first);
        std::fs::File::open(temp.path().join("metadata/loaders/test.json"))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
            .unwrap();
        let offline: Vec<String> = cached_json(temp.path(), "test", &url, &downloader, &cancel)
            .await
            .unwrap();
        assert_eq!(offline, first);
        assert!(
            cached_json::<Vec<String>>(temp.path(), "missing", &url, &downloader, &cancel)
                .await
                .is_err()
        );
        assert_eq!(server.await.unwrap().len(), 3);
        cancel.cancel();
        assert!(
            cached_json::<Vec<String>>(temp.path(), "test", &url, &downloader, &cancel)
                .await
                .is_err()
        );
    }
    #[test]
    fn maps_release_patch_and_snapshot_builds_without_cross_matching() {
        for (loader, game) in [
            ("20.2.3-beta", "1.20.2"),
            ("21.0.10", "1.21"),
            ("21.1.172", "1.21.1"),
            ("26.1.0.0-alpha.13+snapshot-10", "26.1-snapshot-10"),
            ("26.1.2.3-beta", "26.1.2"),
            ("26.3.0.99", "26.3"),
            ("0.25w14craftmine.3-beta", "25w14craftmine"),
        ] {
            assert_eq!(neo_game(loader).as_deref(), Some(game));
        }
        assert_eq!(natural_cmp("47.4.28", "47.4.9"), Ordering::Greater);
        let versions =
            BTreeMap::from([("1.7.10".into(), vec!["1.7.10-10.13.4.1614-1.7.10".into()])]);
        let result = forge_versions(
            "1.7.10",
            &versions,
            &serde_json::json!({"promos":{"1.7.10-recommended":"10.13.4.1614"}}),
        );
        assert!(result[0].recommended);
        assert_eq!(result[0].version, "10.13.4.1614-1.7.10");
        assert!(forge_versions("1.20.1", &versions, &Value::Null).is_empty());
    }
}
