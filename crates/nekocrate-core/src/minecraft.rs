use crate::{
    download::{Downloader, FileSpec, artifact_file, check_cancel},
    model::*,
    storage, version,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MANIFEST_MISS_TTL: Duration = Duration::from_secs(15 * 60);
const UNKNOWN_VERSION: &str =
    "未在官方版本目录中找到此版本，请检查游戏版本号或关联已安装的加载器目录";
const CUSTOM_VERSION: &str = "该本地版本使用自定义加载器，启动器暂不支持安装或修复";
const LEGACY_VERSION: &str =
    "当前原型支持 Minecraft 1.13 及更新的官方版本；旧版本启动协议将在后续加入。";

/// Read only enough local metadata to reject an unsupported install before any
/// network work. Missing/broken JSON may be repaired using official metadata.
pub fn check_local_install(game: &Path, id: &str) -> Result<()> {
    storage::safe_version(id)?;
    let path = game.join("versions").join(id).join(format!("{id}.json"));
    let Ok(value) = storage::read_json::<serde_json::Value>(&path) else {
        return Ok(());
    };
    let inherits = value["inheritsFrom"]
        .as_str()
        .is_some_and(|id| !id.is_empty());
    let loader_main = value["mainClass"].as_str().is_some_and(|main| {
        [
            "net.minecraft.launchwrapper.",
            "cpw.mods.",
            "net.minecraftforge.",
            "net.neoforged.",
            "net.fabricmc.",
            "org.quiltmc.",
            "optifine.",
        ]
        .iter()
        .any(|prefix| main.starts_with(prefix))
    });
    let loader_library = value["libraries"].as_array().is_some_and(|libraries| {
        libraries
            .iter()
            .filter_map(|library| library["name"].as_str())
            .any(|name| {
                [
                    "net.minecraftforge:forge:",
                    "net.minecraftforge:fmlloader:",
                    "net.neoforged:",
                    "net.fabricmc:fabric-loader:",
                    "org.quiltmc:quilt-loader:",
                    "optifine:OptiFine:",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
            })
    });
    if inherits || loader_main || loader_library {
        bail!(CUSTOM_VERSION);
    }
    if value["arguments"].is_null() && value["minecraftArguments"].is_string() {
        bail!(LEGACY_VERSION);
    }
    Ok(())
}

async fn version_entry(
    root: &Path,
    id: &str,
    refresh: impl Future<Output = Result<VersionManifest>>,
) -> Result<VersionEntry> {
    storage::safe_version(id)?;
    let path = root.join("metadata/manifest.json");
    if let Ok(cached) = storage::read_json::<VersionManifest>(&path) {
        if let Some(entry) = cached.versions.into_iter().find(|entry| entry.id == id) {
            return Ok(entry);
        }
        // A recent complete manifest also caches negative results. Old misses
        // must refresh so newly published versions are not permanently rejected.
        let fresh = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < MANIFEST_MISS_TTL);
        if fresh {
            bail!(UNKNOWN_VERSION);
        }
    }
    refresh
        .await?
        .versions
        .into_iter()
        .find(|entry| entry.id == id)
        .context(UNKNOWN_VERSION)
}
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
            if r.name
                .as_deref()
                .is_some_and(|n| n != os && n != "universal")
            {
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
pub fn is_allowed_on(library: &Library, arch: &str) -> bool {
    rules_allow(&library.rules, os_name(), arch, &HashMap::new())
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
    check_cancel(cancel)?;
    let entry = version_entry(root, id, manifest(root, downloader, cancel)).await?;
    check_cancel(cancel)?;
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
    if v.arguments.is_none() && v.minecraft_arguments.is_none() {
        bail!("版本配置缺少启动参数");
    }
    Ok(v)
}
pub fn library_path(root: &Path, artifact: &Artifact) -> Result<PathBuf> {
    Ok(root.join("libraries").join(storage::safe_relative(
        artifact.path.as_deref().context("库文件缺少路径")?,
    )?))
}
/// LWJGL 2's base platform artifact is an empty ZIP, often omitted by launchers.
/// Skip only that known empty artifact, never real Java classes packaged beside
/// natives (e.g. LWJGL 3/text2speech), or self-extracting native jars.
pub fn classpath_artifact(library: &Library) -> Option<&Artifact> {
    const EMPTY_ZIP_SHA1: &str = "b04f3ee8f5e43fa3b162981b50bb72fe1acabb33";
    library.downloads.artifact.as_ref().filter(|artifact| {
        library.natives.is_empty() || !artifact.sha1.eq_ignore_ascii_case(EMPTY_ZIP_SHA1)
    })
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
fn asset_index_file(game: &Path, v: &Version) -> Result<FileSpec> {
    Ok(FileSpec {
        url: v.asset_index.url.clone(),
        path: game
            .join("assets/indexes")
            .join(storage::safe_relative(&format!(
                "{}.json",
                v.asset_index.id
            ))?),
        hash: v.asset_index.sha1.clone(),
        sha256: false,
        size: v.asset_index.size,
    })
}

fn installation_files(game: &Path, v: &Version, index: &AssetIndex) -> Result<Vec<FileSpec>> {
    let runtime_arch = crate::java::JavaRequirement::for_version(v).arch;
    let mut files = vec![];
    files.push(artifact_file(
        v.downloads.get("client").context("元数据缺少客户端文件")?,
        version::client_path(game, v)?,
    ));
    for library in v
        .libraries
        .iter()
        .filter(|l| is_allowed_on(l, runtime_arch))
    {
        if let Some(a) = classpath_artifact(library) {
            files.push(artifact_file(a, library_path(game, a)?));
        }
        if let Some(a) = native_artifact(library) {
            files.push(artifact_file(a, library_path(game, a)?));
        }
    }
    for object in index.objects.values() {
        if object.hash.len() != 40 || !object.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("资源索引校验值无效");
        }
        let path = format!("{}/{}", &object.hash[..2], object.hash);
        files.push(FileSpec {
            url: format!("https://resources.download.minecraft.net/{path}"),
            path: game.join("assets/objects").join(&path),
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
            path: game
                .join("assets/log_configs")
                .join(storage::safe_relative(id)?),
            hash: a["sha1"].as_str().context("日志配置校验值无效")?.into(),
            sha256: false,
            size: a["size"].as_u64().unwrap_or(0),
        });
    }
    Ok(files)
}

/// Rebuild an interrupted official install's plan using verified local metadata.
/// This does not fetch, copy or change anything in the game directory.
pub async fn cached_installation_files(
    root: &Path,
    game: &Path,
    id: &str,
) -> Result<Vec<FileSpec>> {
    storage::safe_version(id)?;
    let local = version::resolve(root, game, id).ok().filter(|resolved| {
        resolved
            .loader
            .as_ref()
            .is_some_and(|loader| loader.kind == "fabric")
    });
    let v = if let Some(resolved) = local {
        resolved.version
    } else {
        let manifest: VersionManifest = storage::read_json(&root.join("metadata/manifest.json"))?;
        let entry = manifest
            .versions
            .iter()
            .find(|entry| entry.id == id)
            .context(UNKNOWN_VERSION)?;
        let path = root.join("metadata/versions").join(format!("{id}.json"));
        if !Downloader::verified(&path, &entry.sha1, false, 0).await? {
            bail!("版本元数据未缓存");
        }
        let v: Version = storage::read_json(&path)?;
        if v.id != id || v.inherits_from.is_some() {
            bail!(CUSTOM_VERSION);
        }
        v
    };
    let index_file = asset_index_file(game, &v)?;
    let path = if Downloader::verified(&index_file.path, &index_file.hash, false, index_file.size)
        .await?
    {
        index_file.path
    } else {
        let cache = crate::download::object_path(&root.join("cache/objects"), &index_file)?;
        if !Downloader::verified(&cache, &index_file.hash, false, index_file.size).await? {
            bail!("资源索引未缓存");
        }
        cache
    };
    let index = storage::read_json(&path)?;
    installation_files(game, &v, &index)
}

pub async fn install_files(
    game: &Path,
    v: &Version,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<()> {
    let index_file = asset_index_file(game, v)?;
    downloader.file(&index_file, cancel, sink).await?;
    let index: AssetIndex = storage::read_json(&index_file.path)?;
    let files = installation_files(game, v, &index)?;
    sink(Event::new(
        "stage",
        format!("校验并下载 {} 个游戏文件", files.len()),
    ));
    downloader.batch(files, cancel, sink).await?;
    check_cancel(cancel)?;
    sink(Event::workflow("natives", "准备原生库与资源映射"));
    let natives = game.join("versions").join(&v.id).join("natives");
    if natives.exists() {
        std::fs::remove_dir_all(&natives)?;
    }
    std::fs::create_dir_all(&natives)?;
    let runtime_arch = crate::java::JavaRequirement::for_version(v).arch;
    for library in v
        .libraries
        .iter()
        .filter(|l| is_allowed_on(l, runtime_arch))
    {
        if let Some(a) = native_artifact(library) {
            extract_natives(
                &library_path(game, a)?,
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
            let source = game
                .join("assets/objects")
                .join(&object.hash[..2])
                .join(&object.hash);
            let dest = if index.map_to_resources {
                game.join("resources")
            } else {
                game.join("assets/virtual").join(&v.asset_index.id)
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
    let memory = crate::memory::options(root, instance).allocation(
        crate::memory::MemoryConfig {
            mode: instance.memory_mode,
            custom_mb: instance.memory_mb,
        },
        crate::java::JavaRequirement::for_version(v).arch,
    )?;
    arguments_with_memory(v, root, instance, account, memory)
}

pub fn arguments_with_memory(
    v: &Version,
    root: &Path,
    instance: &Instance,
    account: &LaunchSession,
    memory_mb: u32,
) -> Result<Vec<String>> {
    if !(512..=65536).contains(&memory_mb) {
        bail!("内存应为 512–65536 MiB");
    }
    let runtime_arch = crate::java::JavaRequirement::for_version(v).arch;
    let game = instance.game_dir(root);
    let working_directory = version::working_directory(&game, v)?;
    let native = game.join("versions").join(&v.id).join("natives");
    // New installations live entirely in their .minecraft directory. Keep a
    // read-only fallback for games installed by releases with shared storage.
    let index = storage::safe_relative(&format!("{}.json", v.asset_index.id))?;
    let assets = if game.join("assets/indexes").join(&index).is_file()
        || !root.join("assets/indexes").join(&index).is_file()
    {
        game.join("assets")
    } else {
        root.join("assets")
    };
    let libraries = if game.join("libraries").is_dir() || !root.join("libraries").is_dir() {
        game.join("libraries")
    } else {
        root.join("libraries")
    };
    let mut classpath = vec![];
    for l in v
        .libraries
        .iter()
        .filter(|l| is_allowed_on(l, runtime_arch))
    {
        if let Some(a) = classpath_artifact(l) {
            let local = library_path(&game, a)?;
            let legacy = library_path(root, a)?;
            let path = if local.is_file() || !legacy.is_file() {
                local
            } else {
                legacy
            };
            classpath.push(crate::java::argument_path(&path));
        }
    }
    classpath.push(crate::java::argument_path(&version::client_path(&game, v)?));
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
        (
            "game_directory",
            crate::java::argument_path(&working_directory),
        ),
        (
            "primary_jar_name",
            format!("{}.jar", v.jar.as_deref().unwrap_or(&v.id)),
        ),
        ("assets_root", crate::java::argument_path(&assets)),
        ("assets_index_name", v.asset_index.id.clone()),
        ("natives_directory", crate::java::argument_path(&native)),
        ("library_directory", crate::java::argument_path(&libraries)),
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
        crate::java::argument_path(&assets.join("virtual").join(&v.asset_index.id)),
    );
    let mut args = Vec::new();
    if let Some(config) = &v.arguments {
        if config.jvm.is_empty() {
            args.extend(expand_for_arch(&legacy_jvm(), &vars, runtime_arch)?);
        } else {
            args.extend(expand_for_arch(&config.jvm, &vars, runtime_arch)?);
        }
    } else {
        args.extend(expand_for_arch(&legacy_jvm(), &vars, runtime_arch)?);
    }
    // The chosen preset owns the heap flags. Imported profile options must not
    // override it or leave an initial heap larger than the selected maximum.
    args.retain(|arg| {
        !arg.starts_with("-Xmx")
            && !arg.starts_with("-Xms")
            && !arg.starts_with("-XX:MaxHeapSize=")
            && !arg.starts_with("-XX:InitialHeapSize=")
    });
    args.insert(0, "-Xms256M".into());
    args.insert(0, format!("-Xmx{memory_mb}M"));
    if let Some(log) = v.logging.as_ref().and_then(|l| l.get("client"))
        && let (Some(argument), Some(id)) = (log["argument"].as_str(), log["file"]["id"].as_str())
    {
        args.push(argument.replace(
            "${path}",
            &crate::java::argument_path(&{
                let relative = storage::safe_relative(id)?;
                let local = game.join("assets/log_configs").join(&relative);
                let legacy = root.join("assets/log_configs").join(&relative);
                if local.is_file() || !legacy.is_file() {
                    local
                } else {
                    legacy
                }
            }),
        ));
    }
    args.push(v.main_class.clone());
    if let Some(legacy) = &v.minecraft_arguments {
        let tokens = split_legacy_arguments(legacy)?
            .into_iter()
            .map(Argument::Plain)
            .collect::<Vec<_>>();
        args.extend(expand_for_arch(&tokens, &vars, runtime_arch)?);
        if let Some(config) = &v.arguments {
            args.extend(expand_for_arch(&config.game, &vars, runtime_arch)?);
        }
    } else if let Some(config) = &v.arguments {
        args.extend(expand_for_arch(&config.game, &vars, runtime_arch)?);
    } else {
        bail!("版本配置缺少启动参数");
    }
    Ok(args)
}
fn legacy_jvm() -> Vec<Argument> {
    [
        "-Djava.library.path=${natives_directory}",
        "-Dminecraft.launcher.brand=${launcher_name}",
        "-Dminecraft.launcher.version=${launcher_version}",
        "-cp",
        "${classpath}",
    ]
    .into_iter()
    .map(|s| Argument::Plain(s.into()))
    .collect()
}

/// Tokenize the old launcher format before substitution. Never invoke a shell;
/// a substituted username/path (including spaces or shell syntax) stays one argument.
fn split_legacy_arguments(input: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars
                .peek()
                .is_some_and(|next| Some(*next) == quote || *next == '\\') =>
            {
                token.push(chars.next().unwrap());
                started = true;
            }
            '\'' | '"' if quote.is_none() => {
                quote = Some(c);
                started = true;
            }
            c if quote == Some(c) => quote = None,
            c if c.is_whitespace() && quote.is_none() => {
                if started {
                    tokens.push(std::mem::take(&mut token));
                    started = false;
                }
            }
            c => {
                token.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        bail!("旧版启动参数的引号未闭合");
    }
    if started {
        tokens.push(token);
    }
    Ok(tokens)
}

pub fn prepare_local_natives(
    root: &Path,
    game: &Path,
    v: &Version,
    cancel: &CancellationToken,
) -> Result<()> {
    let dest = game.join("versions").join(&v.id).join("natives");
    std::fs::create_dir_all(&dest)?;
    let runtime_arch = crate::java::JavaRequirement::for_version(v).arch;
    for library in v
        .libraries
        .iter()
        .filter(|l| is_allowed_on(l, runtime_arch))
    {
        if let Some(artifact) = native_artifact(library) {
            check_cancel(cancel)?;
            extract_natives(
                &version::local_library(root, game, artifact)?,
                &dest,
                library.extract.as_ref(),
                cancel,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn expand(args: &[Argument], vars: &HashMap<&str, String>) -> Result<Vec<String>> {
    expand_for_arch(args, vars, arch_name())
}
fn expand_for_arch(
    args: &[Argument],
    vars: &HashMap<&str, String>,
    arch: &str,
) -> Result<Vec<String>> {
    let mut out = vec![];
    let features = HashMap::from([("has_custom_resolution".into(), true)]);
    for arg in args {
        let values = match arg {
            Argument::Plain(v) => vec![v.clone()],
            Argument::Conditional { rules, value } => {
                if !rules_allow(rules, os_name(), arch, &features) {
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
    fn installation_plan_keeps_real_classes_beside_natives_and_skips_empty_platform_artifact() {
        use serde_json::json;
        let mut v = crate::test_http::version_fixture("1.12.2");
        v.downloads.insert(
            "client".into(),
            serde_json::from_value(json!({
                "url":"http://127.0.0.1:9/client","sha1":"","size":100
            }))
            .unwrap(),
        );
        let native = json!({"path":"platform-natives.jar","url":"http://127.0.0.1:9/native","sha1":"","size":100});
        let mut platform: Library = serde_json::from_value(json!({
            "name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
            "natives":{os_name():"selected"},
            "downloads":{
                "artifact":{"path":"empty-platform.jar","url":"http://127.0.0.1:9/empty","size":22,"sha1":"b04f3ee8f5e43fa3b162981b50bb72fe1acabb33"},
                "classifiers":{"selected":native}}
        })).unwrap();
        v.libraries.push(platform.clone());
        let index: AssetIndex = serde_json::from_value(json!({"objects":{}})).unwrap();
        let files = installation_files(Path::new("game"), &v, &index).unwrap();
        assert_eq!(files.len(), 2); // client + natives; no empty base JAR
        assert!(
            files
                .iter()
                .any(|f| f.path.ends_with("platform-natives.jar"))
        );
        assert!(!files.iter().any(|f| f.path.ends_with("empty-platform.jar")));
        // A combined descriptor with real classes must still fetch both files.
        platform.name = "org.lwjgl:lwjgl:3.2.2".into();
        let artifact = platform.downloads.artifact.as_mut().unwrap();
        artifact.path = Some("lwjgl.jar".into());
        artifact.sha1 = "1".repeat(40);
        artifact.size = 100;
        v.libraries = vec![platform];
        let files = installation_files(Path::new("game"), &v, &index).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|f| f.path.ends_with("lwjgl.jar")));
        assert!(
            files
                .iter()
                .any(|f| f.path.ends_with("platform-natives.jar"))
        );
    }

    fn manifest_fixture(id: &str) -> VersionManifest {
        VersionManifest {
            latest: HashMap::from([("release".into(), id.into())]),
            versions: vec![VersionEntry {
                id: id.into(),
                kind: "release".into(),
                url: "http://127.0.0.1:9/unexpected-network".into(),
                sha1: "0".repeat(40),
            }],
        }
    }

    fn expire_manifest(root: &Path) {
        let file = std::fs::File::options()
            .write(true)
            .open(root.join("metadata/manifest.json"))
            .unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(
            std::time::SystemTime::now() - MANIFEST_MISS_TTL - Duration::from_secs(60),
        ))
        .unwrap();
    }

    #[tokio::test]
    async fn cached_entries_and_recent_misses_do_not_refresh_the_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("metadata/manifest.json");
        storage::write_json(&path, &manifest_fixture("1.21.1")).unwrap();
        let entry = version_entry(temp.path(), "1.21.1", async {
            panic!("cache hit must not fetch")
        })
        .await
        .unwrap();
        assert_eq!(entry.id, "1.21.1");
        let error = version_entry(temp.path(), "RLCraft", async {
            panic!("recent miss must not fetch")
        })
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), UNKNOWN_VERSION);
        expire_manifest(temp.path());
        let entry = version_entry(temp.path(), "1.21.1", async {
            panic!("stale positive hit must not fetch")
        })
        .await
        .unwrap();
        assert_eq!(entry.id, "1.21.1");
        let error = version_entry(temp.path(), "../invalid", async {
            panic!("invalid path must not fetch")
        })
        .await
        .unwrap_err();
        assert!(error.to_string().contains("路径"));
    }

    #[tokio::test]
    async fn missing_corrupt_or_stale_negative_cache_refreshes_and_preserves_network_errors() {
        for state in ["missing", "corrupt", "stale"] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("metadata/manifest.json");
            if state != "missing" {
                storage::write_json(&path, &manifest_fixture("1.21.1")).unwrap();
                if state == "corrupt" {
                    std::fs::write(&path, b"broken JSON").unwrap();
                } else {
                    expire_manifest(temp.path());
                }
            }
            let mut refreshes = 0;
            let entry = version_entry(temp.path(), "26.1", async {
                refreshes += 1;
                Ok(manifest_fixture("26.1"))
            })
            .await
            .unwrap();
            assert_eq!(entry.id, "26.1");
            assert_eq!(refreshes, 1);
            let error = version_entry(temp.path(), "26.1", async {
                bail!("fixture network unavailable")
            })
            .await
            .unwrap_err();
            assert_eq!(error.to_string(), "fixture network unavailable");
            let error = version_entry(temp.path(), "Unknown", async {
                Ok(manifest_fixture("26.1"))
            })
            .await
            .unwrap_err();
            assert_eq!(error.to_string(), UNKNOWN_VERSION);
        }
    }

    #[tokio::test]
    async fn cached_version_is_verified_without_any_network_and_corruption_is_repaired() {
        use sha1::{Digest, Sha1};
        let temp = tempfile::tempdir().unwrap();
        let version_path = temp.path().join("metadata/versions/1.21.1.json");
        let body = serde_json::to_vec(&crate::test_http::version_fixture("1.21.1")).unwrap();
        std::fs::create_dir_all(version_path.parent().unwrap()).unwrap();
        std::fs::write(&version_path, &body).unwrap();
        let mut manifest = manifest_fixture("1.21.1");
        manifest.versions[0].sha1 = hex::encode(Sha1::digest(&body));
        storage::write_json(&temp.path().join("metadata/manifest.json"), &manifest).unwrap();
        expire_manifest(temp.path());
        let mut downloader = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                retries: 0,
                ..Settings::default()
            },
        )
        .unwrap();
        // Route any accidental request (including the public manifest) to this
        // local socket. The hit must finish without even opening a connection.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        downloader.client = reqwest::Client::builder()
            .no_proxy()
            .proxy(
                reqwest::Proxy::all(format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            )
            .build()
            .unwrap();
        let cancel = CancellationToken::new();
        let sink: EventSink = std::sync::Arc::new(|_| {});
        let v = tokio::time::timeout(
            Duration::from_secs(2),
            version(temp.path(), "1.21.1", &downloader, &cancel, &sink),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(v.id, "1.21.1");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );

        std::fs::write(&version_path, b"corrupted metadata").unwrap();
        let (url, server) =
            crate::test_http::serve(vec![(200, String::from_utf8(body.clone()).unwrap())]).await;
        manifest.versions[0].url = url;
        storage::write_json(&temp.path().join("metadata/manifest.json"), &manifest).unwrap();
        let downloader = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let v = version(temp.path(), "1.21.1", &downloader, &cancel, &sink)
            .await
            .unwrap();
        assert_eq!(v.id, "1.21.1");
        assert_eq!(server.await.unwrap().len(), 1);
        assert_eq!(std::fs::read(version_path).unwrap(), body);
    }

    #[test]
    fn local_loader_metadata_is_rejected_without_guessing_from_version_names() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions/RLCraft/RLCraft.json");
        for value in [
            serde_json::json!({"inheritsFrom": "1.20.1"}),
            serde_json::json!({"mainClass": "net.minecraft.launchwrapper.Launch", "minecraftArguments": "--version 1.12.2"}),
            serde_json::json!({"mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"}),
            serde_json::json!({"libraries": [{"name": "net.neoforged:neoforge:21.1.1"}]}),
        ] {
            storage::write_json(&path, &value).unwrap();
            assert_eq!(
                check_local_install(temp.path(), "RLCraft")
                    .unwrap_err()
                    .to_string(),
                CUSTOM_VERSION
            );
        }
        storage::write_json(
            &path,
            &serde_json::json!({"minecraftArguments": "--version 1.12.2"}),
        )
        .unwrap();
        assert_eq!(
            check_local_install(temp.path(), "RLCraft")
                .unwrap_err()
                .to_string(),
            LEGACY_VERSION
        );
        storage::write_json(&path, &crate::test_http::version_fixture("RLCraft")).unwrap();
        check_local_install(temp.path(), "RLCraft").unwrap();
        std::fs::write(&path, b"broken JSON").unwrap();
        check_local_install(temp.path(), "RLCraft").unwrap();
        check_local_install(temp.path(), "Missing").unwrap();
    }

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
    async fn install_and_launch_use_original_directory_for_libraries_and_assets() {
        use sha1::{Digest, Sha1};
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let game = temp.path().join("original/.minecraft");
        let launcher = crate::Launcher::open(Some(root.clone())).unwrap();
        fs::create_dir_all(game.join("saves")).unwrap();
        let instance = launcher
            .import_instance(game.clone(), "Original".into(), "1.21.1".into())
            .await
            .unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let mut v = crate::test_http::version_fixture("1.21.1");
        let hash = |bytes: &[u8]| hex::encode(Sha1::digest(bytes));
        let object = b"original-asset";
        let object_hash = hash(object);
        let object_path = format!("assets/objects/{}/{}", &object_hash[..2], object_hash);
        let index = serde_json::to_vec(
            &serde_json::json!({"objects": {"test": {"hash":object_hash,"size":object.len()}}}),
        )
        .unwrap();
        let library = b"original-library";
        let log = b"log-config";
        let client = b"client";
        let lib_path = "fixture/client/1/client-1.jar";
        v.asset_index.sha1 = hash(&index);
        v.asset_index.size = index.len() as u64;
        v.asset_index.url = "http://127.0.0.1:9/no-network".into();
        v.downloads.insert(
            "client".into(),
            Artifact {
                url: "http://127.0.0.1:9/no-network".into(),
                sha1: hash(client),
                size: client.len() as u64,
                path: None,
            },
        );
        v.libraries.push(serde_json::from_value(serde_json::json!({
            "name": "fixture:client:1", "downloads": {"artifact": {"path":lib_path,"url":"http://127.0.0.1:9/no-network","sha1":hash(library),"size":library.len()}}
        })).unwrap());
        v.logging = Some(
            serde_json::json!({"client":{"argument":"-Dlog=${path}","file":{"id":"client.xml","url":"http://127.0.0.1:9/no-network","sha1":hash(log),"size":log.len()}}}),
        );
        v.arguments.as_mut().unwrap().game.extend([
            Argument::Plain("--assetsDir".into()),
            Argument::Plain("${assets_root}".into()),
            Argument::Plain("--libraryDir".into()),
            Argument::Plain("${library_directory}".into()),
        ]);
        let files = [
            ("assets/indexes/1.21.1.json".to_owned(), index.as_slice()),
            (object_path.clone(), object.as_slice()),
            (format!("libraries/{lib_path}"), library.as_slice()),
            ("assets/log_configs/client.xml".to_owned(), log.as_slice()),
            ("versions/1.21.1/1.21.1.jar".to_owned(), client.as_slice()),
        ];
        for (relative, bytes) in &files {
            let path = game.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        // A missing game file can be restored from the cache into the game tree.
        fs::remove_file(game.join(&object_path)).unwrap();
        let cache = root
            .join("cache/objects/sha1")
            .join(&object_hash[..2])
            .join(&object_hash);
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(cache, object).unwrap();
        let downloader = Downloader::new(&root, Settings::default()).unwrap();
        let sink: EventSink = std::sync::Arc::new(|_| {});
        install_files(&game, &v, &downloader, &CancellationToken::new(), &sink)
            .await
            .unwrap();
        for (relative, bytes) in &files {
            assert_eq!(fs::read(game.join(relative)).unwrap(), *bytes);
            assert!(!root.join(relative).exists());
        }
        let args = arguments(&v, &root, &instance, &LaunchSession::guest()).unwrap();
        for path in [game.join("assets"), game.join("libraries"), game.clone()] {
            assert!(args.contains(&crate::java::argument_path(&path)));
        }
        assert!(args.iter().any(|arg| {
            arg.contains(&crate::java::argument_path(
                &game.join("libraries").join(lib_path),
            ))
        }));
        assert!(args.contains(&format!(
            "-Dlog={}",
            crate::java::argument_path(&game.join("assets/log_configs/client.xml"))
        )));
        assert!(
            !launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
        // Releases before this fix stored shared resources in the app directory.
        for name in ["libraries", "assets"] {
            fs::remove_dir_all(root.join(name)).unwrap();
            fs::rename(game.join(name), root.join(name)).unwrap();
        }
        let legacy_args = arguments(&v, &root, &instance, &LaunchSession::guest()).unwrap();
        assert!(legacy_args.contains(&crate::java::argument_path(&root.join("assets"))));
        assert!(legacy_args.iter().any(|arg| {
            arg.contains(&crate::java::argument_path(
                &root.join("libraries").join(lib_path),
            ))
        }));
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
                install_files(&game, &version, &downloader, &cancel, &sink)
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
                assert_eq!(args[game_at + 1], crate::java::argument_path(&game));
                assert!(args.contains(&crate::java::argument_path(
                    &version_dir.join(format!("{id}.jar"))
                )));
                assert!(args.contains(&format!(
                    "-Djava.library.path={}",
                    crate::java::argument_path(&version_dir.join("natives"))
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
