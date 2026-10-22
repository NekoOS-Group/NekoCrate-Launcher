use crate::{
    download::{Downloader, FileSpec, artifact_file, check_cancel},
    model::*,
    storage,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

pub const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}
pub fn arch_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "x86"
    }
}
pub fn rules_allow(rules: &[Rule], os: &str, arch: &str, features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if let Some(r) = &rule.os {
            if r.name.as_deref().is_some_and(|n| n != os) {
                continue;
            }
            if r.arch
                .as_deref()
                .is_some_and(|a| a != arch && !(a == "x86" && arch == "i386"))
            {
                continue;
            }
            if let Some(pattern) = &r.version {
                let version = if cfg!(windows) {
                    "10.0".to_owned()
                } else {
                    std::process::Command::new("uname")
                        .arg("-r")
                        .output()
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().into())
                        .unwrap_or_default()
                };
                if !regex::Regex::new(pattern).is_ok_and(|r| r.is_match(&version)) {
                    continue;
                }
            }
        }
        if rule
            .features
            .iter()
            .any(|(key, expected)| features.get(key).copied().unwrap_or(false) != *expected)
        {
            continue;
        }
        allowed = rule.action == "allow";
    }
    allowed
}
pub fn is_allowed(library: &Library) -> bool {
    rules_allow(&library.rules, os_name(), arch_name(), &HashMap::new())
}
pub async fn manifest(
    root: &Path,
    downloader: &Downloader,
    cancel: &CancellationToken,
) -> Result<VersionManifest> {
    let value: VersionManifest = downloader.json(MANIFEST, cancel).await?;
    storage::write_json(&root.join("metadata/manifest.json"), &value)?;
    Ok(value)
}
pub async fn version(
    root: &Path,
    id: &str,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<Version> {
    let manifest = match manifest(root, downloader, cancel).await {
        Ok(v) => v,
        Err(e) => {
            check_cancel(cancel)?;
            let cache = root.join("metadata/manifest.json");
            if !cache.exists() {
                return Err(e);
            }
            sink(Event::new("warning", "网络不可用，使用已缓存版本目录"));
            storage::read_json(&cache)?
        }
    };
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .context("未找到该官方游戏版本；原型暂不支持自定义加载器版本")?;
    let relative = storage::safe_relative(id)?;
    if relative.components().count() != 1 {
        bail!("无效游戏版本");
    }
    let path = root.join("metadata/versions").join(format!("{id}.json"));
    downloader
        .file(
            &FileSpec {
                url: entry.url.clone(),
                path: path.clone(),
                hash: entry.sha1.clone(),
                sha256: false,
                size: 0,
            },
            cancel,
            sink,
        )
        .await?;
    let v: Version = storage::read_json(&path)?;
    if v.id != id || v.inherits_from.is_some() {
        bail!("当前原型仅支持独立的官方 Java 版元数据");
    }
    if v.arguments.is_none() {
        bail!("当前原型支持 Minecraft 1.13 及更新的官方版本；旧版本启动协议将在后续加入。");
    }
    if cfg!(all(target_os = "macos", target_arch = "aarch64"))
        && v.libraries
            .iter()
            .any(|l| l.name.starts_with("org.lwjgl:lwjgl:"))
        && !v
            .libraries
            .iter()
            .any(|l| l.name.contains("natives-macos-arm64"))
    {
        bail!("此版本未提供 Apple Silicon 原生库；原型在 Apple Silicon 上请使用 1.19 或更新版本。");
    }
    Ok(v)
}
pub fn library_path(root: &Path, artifact: &Artifact) -> Result<PathBuf> {
    Ok(root.join("libraries").join(storage::safe_relative(
        artifact.path.as_deref().context("库文件缺少路径")?,
    )?))
}
pub fn native_artifact(l: &Library) -> Option<&Artifact> {
    let key = l.natives.get(os_name())?.replace(
        "${arch}",
        if cfg!(target_pointer_width = "64") {
            "64"
        } else {
            "32"
        },
    );
    l.downloads.classifiers.get(&key)
}
pub async fn install_files(
    root: &Path,
    game: &Path,
    v: &Version,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<()> {
    let mut files = vec![];
    files.push(artifact_file(
        v.downloads.get("client").context("元数据缺少客户端文件")?,
        game.join("versions")
            .join(&v.id)
            .join(format!("{}.jar", v.id)),
    ));
    for library in v.libraries.iter().filter(|l| is_allowed(l)) {
        if let Some(a) = &library.downloads.artifact {
            files.push(artifact_file(a, library_path(root, a)?));
        }
        if let Some(a) = native_artifact(library) {
            files.push(artifact_file(a, library_path(root, a)?));
        }
    }
    let index_path = root
        .join("assets/indexes")
        .join(storage::safe_relative(&format!(
            "{}.json",
            v.asset_index.id
        ))?);
    downloader
        .file(
            &FileSpec {
                url: v.asset_index.url.clone(),
                path: index_path.clone(),
                hash: v.asset_index.sha1.clone(),
                sha256: false,
                size: v.asset_index.size,
            },
            cancel,
            sink,
        )
        .await?;
    let index: AssetIndex = storage::read_json(&index_path)?;
    for object in index.objects.values() {
        if object.hash.len() != 40 || !object.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("资源索引校验值无效");
        }
        let path = format!("{}/{}", &object.hash[..2], object.hash);
        files.push(FileSpec {
            url: format!("https://resources.download.minecraft.net/{path}"),
            path: root.join("assets/objects").join(&path),
            hash: object.hash.clone(),
            sha256: false,
            size: object.size,
        });
    }
    if let Some(log) = v.logging.as_ref().and_then(|l| l.get("client")) {
        let a = &log["file"];
        let id = a["id"].as_str().context("日志配置 ID 无效")?;
        files.push(FileSpec {
            url: a["url"].as_str().context("日志配置 URL 无效")?.into(),
            path: root
                .join("assets/log_configs")
                .join(storage::safe_relative(id)?),
            hash: a["sha1"].as_str().context("日志配置校验值无效")?.into(),
            sha256: false,
            size: a["size"].as_u64().unwrap_or(0),
        });
    }
    sink(Event::new(
        "stage",
        format!("校验并下载 {} 个游戏文件", files.len()),
    ));
    downloader.batch(files, cancel, sink).await?;
    check_cancel(cancel)?;
    let natives = game.join("versions").join(&v.id).join("natives");
    if natives.exists() {
        std::fs::remove_dir_all(&natives)?;
    }
    std::fs::create_dir_all(&natives)?;
    for library in v.libraries.iter().filter(|l| is_allowed(l)) {
        if let Some(a) = native_artifact(library) {
            extract_natives(
                &library_path(root, a)?,
                &natives,
                library.extract.as_ref(),
                cancel,
            )?;
        }
    }
    if index.virtual_ || index.map_to_resources {
        for (name, object) in &index.objects {
            check_cancel(cancel)?;
            let relative = storage::safe_relative(name)?;
            let source = root
                .join("assets/objects")
                .join(&object.hash[..2])
                .join(&object.hash);
            let dest = if index.map_to_resources {
                game.join("resources")
            } else {
                root.join("assets/virtual").join(&v.asset_index.id)
            }
            .join(relative);
            std::fs::create_dir_all(dest.parent().unwrap())?;
            std::fs::copy(source, dest)?;
        }
    }
    Ok(())
}
fn extract_natives(
    zip_path: &Path,
    dest: &Path,
    extract: Option<&Extract>,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(i)?;
        if entry.name().starts_with("META-INF/")
            || extract.is_some_and(|e| e.exclude.iter().any(|p| entry.name().starts_with(p)))
        {
            continue;
        }
        let relative = entry.enclosed_name().context("原生库含越界路径")?;
        if entry.is_symlink() {
            bail!("原生库包含不支持的符号链接");
        }
        let target = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(target)?;
        } else {
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::io::copy(&mut entry, &mut std::fs::File::create(&target)?)?;
        }
    }
    Ok(())
}
/// Game process identity. A local player is never persisted as a Microsoft account.
pub struct LaunchSession {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub auth_session: String,
    pub client_id: String,
    pub xuid: String,
    pub user_type: &'static str,
    pub(crate) log_secrets: Vec<String>,
}
impl LaunchSession {
    pub fn guest() -> Self {
        Self {
            username: "Player".into(),
            // Java UUID.nameUUIDFromBytes("OfflinePlayer:Player".getBytes(UTF_8)).
            // Stable across launches and instances; no account or token is invented.
            uuid: "a01e3843e5213998958af459800e4d11".into(),
            access_token: "0".into(),
            auth_session: "-".into(),
            client_id: String::new(),
            xuid: String::new(),
            user_type: "legacy",
            log_secrets: vec![],
        }
    }
}
impl From<crate::auth::MinecraftSession> for LaunchSession {
    fn from(session: crate::auth::MinecraftSession) -> Self {
        Self {
            username: session.account.username,
            uuid: session.account.id,
            auth_session: session.access_token.clone(),
            log_secrets: vec![session.access_token.clone(), session.refresh_token],
            access_token: session.access_token,
            client_id: session.account.client_id,
            xuid: session.xuid,
            user_type: "msa",
        }
    }
}

pub fn arguments(
    v: &Version,
    root: &Path,
    instance: &Instance,
    account: &LaunchSession,
) -> Result<Vec<String>> {
    let game = instance.game_dir(root);
    let native = game.join("versions").join(&v.id).join("natives");
    let mut classpath = vec![];
    for l in v.libraries.iter().filter(|l| is_allowed(l)) {
        if let Some(a) = &l.downloads.artifact {
            classpath.push(library_path(root, a)?.to_string_lossy().to_string());
        }
    }
    classpath.push(
        game.join("versions")
            .join(&v.id)
            .join(format!("{}.jar", v.id))
            .to_string_lossy()
            .into(),
    );
    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut vars = HashMap::from([
        ("auth_player_name", account.username.clone()),
        ("auth_uuid", account.uuid.clone()),
        ("auth_access_token", account.access_token.clone()),
        ("auth_session", account.auth_session.clone()),
        ("user_type", account.user_type.into()),
        ("user_properties", "{}".into()),
        ("version_name", v.id.clone()),
        ("version_type", v.kind.clone()),
        ("game_directory", game.to_string_lossy().into()),
        ("assets_root", root.join("assets").to_string_lossy().into()),
        ("assets_index_name", v.asset_index.id.clone()),
        ("natives_directory", native.to_string_lossy().into()),
        (
            "library_directory",
            root.join("libraries").to_string_lossy().into(),
        ),
        ("classpath", classpath.join(separator)),
        ("classpath_separator", separator.into()),
        ("launcher_name", "NekoCrate".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("clientid", account.client_id.clone()),
        ("auth_xuid", account.xuid.clone()),
        ("resolution_width", "1280".into()),
        ("resolution_height", "720".into()),
    ]);
    vars.insert(
        "game_assets",
        root.join("assets/virtual")
            .join(&v.asset_index.id)
            .to_string_lossy()
            .into(),
    );
    let mut args = vec![format!("-Xmx{}M", instance.memory_mb), "-Xms256M".into()];
    let config = v.arguments.as_ref().context("不支持旧版本启动参数")?;
    args.extend(expand(&config.jvm, &vars)?);
    if let Some(log) = v.logging.as_ref().and_then(|l| l.get("client"))
        && let (Some(argument), Some(id)) = (log["argument"].as_str(), log["file"]["id"].as_str())
    {
        args.push(
            argument.replace(
                "${path}",
                &root
                    .join("assets/log_configs")
                    .join(storage::safe_relative(id)?)
                    .to_string_lossy(),
            ),
        );
    }
    args.push(v.main_class.clone());
    args.extend(expand(&config.game, &vars)?);
    Ok(args)
}
fn expand(args: &[Argument], vars: &HashMap<&str, String>) -> Result<Vec<String>> {
    let mut out = vec![];
    let features = HashMap::from([("has_custom_resolution".into(), true)]);
    for arg in args {
        let values = match arg {
            Argument::Plain(v) => vec![v.clone()],
            Argument::Conditional { rules, value } => {
                if !rules_allow(rules, os_name(), arch_name(), &features) {
                    continue;
                }
                match value {
                    serde_json::Value::String(s) => vec![s.clone()],
                    serde_json::Value::Array(a) => a
                        .iter()
                        .map(|s| s.as_str().map(str::to_owned).context("启动参数不是字符串"))
                        .collect::<Result<Vec<_>>>()?,
                    _ => bail!("无法识别的启动参数格式"),
                }
            }
        };
        for mut value in values {
            for (key, replacement) in vars {
                value = value.replace(&format!("${{{key}}}"), replacement);
            }
            if value.contains("${") {
                bail!("版本使用了尚未支持的启动参数占位符");
            }
            out.push(value);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_rules_and_features() {
        let rules: Vec<Rule> = serde_json::from_value(
            serde_json::json!([{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]),
        )
        .unwrap();
        assert!(rules_allow(&rules, "linux", "x86_64", &HashMap::new()));
        assert!(!rules_allow(&rules, "osx", "aarch64", &HashMap::new()));
        let rules: Vec<Rule> = serde_json::from_value(
            serde_json::json!([{"action":"allow","features":{"is_demo_user":true}}]),
        )
        .unwrap();
        assert!(!rules_allow(&rules, "linux", "x86_64", &HashMap::new()));
    }
    #[test]
    fn expands_arguments_without_shell_splitting() {
        let args = vec![Argument::Plain("${game_directory}".into())];
        let vars = HashMap::from([("game_directory", "/tmp/My World; echo secret".into())]);
        assert_eq!(
            expand(&args, &vars).unwrap(),
            vec!["/tmp/My World; echo secret"]
        );
        assert!(expand(&[Argument::Plain("${unknown}".into())], &vars).is_err());
    }
    #[tokio::test]
    async fn two_versions_keep_clients_and_natives_separate_but_share_game_directory() {
        use sha1::{Digest, Sha1};
        use std::fs;
        for linked in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let launcher = crate::Launcher::open(Some(temp.path().join("launcher"))).unwrap();
            let source = temp.path().join("external/.minecraft");
            let instance = if linked {
                fs::create_dir_all(source.join("saves")).unwrap();
                launcher
                    .import_instance(source.clone(), "关联多版本".into(), "1.21.1".into())
                    .await
                    .unwrap()
            } else {
                launcher.create_instance("多版本", "1.21.1").unwrap()
            };
            launcher.add_version(&instance.id, "1.20.1").unwrap();
            let game = launcher.game_dir(&instance.id).unwrap();
            let downloader = Downloader::new(launcher.root(), Settings::default()).unwrap();
            let cancel = CancellationToken::new();
            let sink: EventSink = std::sync::Arc::new(|_| {});
            let account: LaunchSession = crate::auth::MinecraftSession {
                account: Account {
                    id: "fixture-player".into(),
                    username: "Fixture".into(),
                    expires_at: 0,
                    client_id: "fixture-client".into(),
                    owned: true,
                    skin: None,
                },
                access_token: "fixture-token".into(),
                refresh_token: "fixture-refresh".into(),
                xuid: "0".into(),
            }
            .into();
            let mut launch_args = vec![];
            for (index, id) in ["1.21.1", "1.20.1"].iter().enumerate() {
                let index_body = format!("{{ \"objects\": {{}} }}{}", " ".repeat(index));
                let client_body = format!("client-fixture-{id}");
                let (base, server) = crate::test_http::serve(vec![
                    (200, index_body.clone()),
                    (200, client_body.clone()),
                ])
                .await;
                let mut version = crate::test_http::version_fixture(id);
                version.asset_index.url = format!("{base}/index");
                version.asset_index.sha1 = hex::encode(Sha1::digest(index_body.as_bytes()));
                version.asset_index.size = index_body.len() as u64;
                version.downloads.insert(
                    "client".into(),
                    Artifact {
                        url: format!("{base}/client"),
                        sha1: hex::encode(Sha1::digest(client_body.as_bytes())),
                        size: client_body.len() as u64,
                        path: None,
                    },
                );
                install_files(
                    launcher.root(),
                    &game,
                    &version,
                    &downloader,
                    &cancel,
                    &sink,
                )
                .await
                .unwrap();
                assert_eq!(server.await.unwrap().len(), 2);
                fs::write(game.join("versions").join(id).join("natives/marker"), id).unwrap();
                launch_args
                    .push(arguments(&version, launcher.root(), &instance, &account).unwrap());
            }
            for (index, id) in ["1.21.1", "1.20.1"].iter().enumerate() {
                let version_dir = game.join("versions").join(id);
                assert_eq!(
                    fs::read_to_string(version_dir.join("natives/marker")).unwrap(),
                    *id
                );
                assert_eq!(
                    fs::read_to_string(version_dir.join(format!("{id}.jar"))).unwrap(),
                    format!("client-fixture-{id}")
                );
                let args = &launch_args[index];
                for (key, value) in [
                    ("--username", "Fixture"),
                    ("--uuid", "fixture-player"),
                    ("--accessToken", "fixture-token"),
                    ("--userType", "msa"),
                    ("--clientId", "fixture-client"),
                    ("--xuid", "0"),
                ] {
                    let at = args.iter().position(|arg| arg == key).unwrap();
                    assert_eq!(args[at + 1], value);
                }
                let game_at = args.iter().position(|arg| arg == "--gameDir").unwrap();
                assert_eq!(args[game_at + 1], game.to_string_lossy());
                assert!(
                    args.contains(
                        &version_dir
                            .join(format!("{id}.jar"))
                            .to_string_lossy()
                            .into_owned()
                    )
                );
                assert!(args.contains(&format!(
                    "-Djava.library.path={}",
                    version_dir.join("natives").display()
                )));
            }
            if linked {
                assert_eq!(game, source.canonicalize().unwrap());
                assert!(
                    !launcher
                        .instance_dir(&instance.id)
                        .unwrap()
                        .join(".minecraft")
                        .exists()
                );
            }
        }
    }
}
