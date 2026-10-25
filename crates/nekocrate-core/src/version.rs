//! Resolve local launcher profiles without changing their JSON or game directory.
use crate::{minecraft, model::*, storage};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct ResolvedVersion {
    pub version: Version,
    pub minecraft_version: String,
    pub loader: Option<ModLoader>,
}

pub fn metadata_path(game: &Path, id: &str) -> Result<PathBuf> {
    storage::safe_version(id)?;
    Ok(game.join("versions").join(id).join(format!("{id}.json")))
}

pub fn resolve(root: &Path, game: &Path, id: &str) -> Result<ResolvedVersion> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = id.to_owned();
    loop {
        if chain.len() >= 32 || !seen.insert(current.clone()) {
            bail!("版本继承存在循环或层数过多：{id}");
        }
        let path = metadata_path(game, &current)?;
        let path = if !path.is_file() && current != id {
            root.join("metadata/versions")
                .join(format!("{current}.json"))
        } else {
            path
        };
        let value: Value = storage::read_json(&path)
            .with_context(|| format!("缺少或无法读取版本配置：{current}"))?;
        if value["id"].as_str() != Some(&current) {
            bail!("版本元数据与所选版本不一致：{current}");
        }
        let parent = value["inheritsFrom"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        chain.push(value);
        let Some(parent) = parent else { break };
        current = parent;
    }
    chain.reverse();
    resolve_chain(&chain)
}

pub fn resolve_chain(chain: &[Value]) -> Result<ResolvedVersion> {
    let first = chain.first().context("版本配置为空")?;
    let base_id = first["id"].as_str().context("版本配置缺少 ID")?;
    let mut merged = json!({"libraries": [], "downloads": {}, "type": "release"});
    let mut client_id = base_id.to_owned();
    for patch in chain {
        let id = patch["id"].as_str().context("版本配置缺少 ID")?;
        storage::safe_version(id)?;
        if patch["downloads"]["client"].is_object() {
            client_id = id.into();
        }
        if let Some(jar) = patch["jar"].as_str() {
            storage::safe_version(jar)?;
            client_id = jar.into();
        }
        merge_profile(&mut merged, patch)?;
    }
    merged.as_object_mut().unwrap().remove("inheritsFrom");
    merged["jar"] = json!(client_id);
    let loader = detect_loader(&merged);
    let minecraft_version = game_version(&merged, base_id, loader.as_ref());
    let libs = merged["libraries"]
        .as_array_mut()
        .context("版本依赖格式无效")?;
    libs.retain(|library| library["clientreq"].as_bool() != Some(false));
    for library in libs {
        normalize_library(library)?;
    }
    let mut version: Version =
        serde_json::from_value(merged).context("版本配置不完整，请检查基础游戏版本")?;
    if version.arguments.is_none() && version.minecraft_arguments.is_none() {
        bail!("版本配置缺少启动参数");
    }
    if version.main_class.trim().is_empty() {
        bail!("版本配置缺少启动入口");
    }
    // Old profiles often omit javaVersion; infer it from the game, never the pack name.
    if version.java_version.is_none() {
        version.java_version = Some(JavaVersion {
            major: required_java(&minecraft_version),
        });
    }
    Ok(ResolvedVersion {
        version,
        minecraft_version,
        loader,
    })
}

fn merge_profile(base: &mut Value, patch: &Value) -> Result<()> {
    for (key, value) in patch.as_object().context("版本配置必须是对象")? {
        if value.is_null() || key == "inheritsFrom" {
            continue;
        }
        match key.as_str() {
            "libraries" => {
                let additions = value.as_array().context("版本依赖格式无效")?;
                let libraries = base["libraries"].as_array_mut().unwrap();
                let keys: HashSet<_> = additions
                    .iter()
                    .filter_map(|l| l["name"].as_str())
                    .map(library_key)
                    .collect();
                libraries.retain(|l| {
                    l["name"]
                        .as_str()
                        .is_none_or(|name| !keys.contains(&library_key(name)))
                });
                libraries.extend(additions.iter().cloned());
            }
            "arguments" => {
                for side in ["jvm", "game"] {
                    if let Some(args) = value.get(side) {
                        let args = args.as_array().context("启动参数格式无效")?;
                        if !base["arguments"].is_object() {
                            base["arguments"] = json!({});
                        }
                        if !base["arguments"][side].is_array() {
                            base["arguments"][side] = json!([]);
                        }
                        base["arguments"][side]
                            .as_array_mut()
                            .unwrap()
                            .extend(args.iter().cloned());
                    }
                }
            }
            "downloads" | "logging" => {
                if !base[key].is_object() {
                    base[key] = json!({});
                }
                base[key]
                    .as_object_mut()
                    .unwrap()
                    .extend(value.as_object().context("版本下载配置无效")?.clone());
            }
            _ => base[key] = value.clone(),
        }
    }
    Ok(())
}

fn library_key(name: &str) -> String {
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = coordinate.split(':').collect();
    format!(
        "{}:{}:{}@{extension}",
        parts.first().unwrap_or(&""),
        parts.get(1).unwrap_or(&""),
        parts.get(3).unwrap_or(&"")
    )
}

pub fn maven_path(name: &str, classifier: Option<&str>) -> Result<String> {
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len())
        || parts.iter().chain([extension].iter()).any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
        })
    {
        bail!("无效 Maven 依赖：{name}");
    }
    let classifier = classifier.or_else(|| parts.get(3).copied());
    let suffix = classifier.map(|c| format!("-{c}")).unwrap_or_default();
    let path = format!(
        "{}/{}/{}/{}-{}{suffix}.{extension}",
        parts[0].replace('.', "/"),
        parts[1],
        parts[2],
        parts[1],
        parts[2]
    );
    storage::safe_relative(&path)?;
    Ok(path)
}

pub(crate) fn normalize_library(library: &mut Value) -> Result<()> {
    let name = library["name"]
        .as_str()
        .context("依赖缺少 Maven 名称")?
        .to_owned();
    let path = maven_path(&name, None)?;
    // Forge 1.12 and Fabric profiles use Maven coordinates + a repository URL.
    if !library["downloads"].is_object()
        || library["downloads"]
            .as_object()
            .is_some_and(|v| v.is_empty())
    {
        let repository = library["url"]
            .as_str()
            .unwrap_or("https://libraries.minecraft.net/");
        let artifact = |path: String| {
            json!({"url": format!("{}/{path}", repository.trim_end_matches('/')), "path":path,
            "sha1": library["sha1"].as_str().unwrap_or(""), "size":library["size"].as_u64().unwrap_or(0)})
        };
        let mut downloads = json!({"artifact": artifact(path.clone()), "classifiers": {}});
        if let Some(natives) = library["natives"].as_object() {
            for classifier in natives.values().filter_map(Value::as_str) {
                for arch in ["32", "64"] {
                    let classifier = classifier.replace("${arch}", arch);
                    downloads["classifiers"][&classifier] =
                        artifact(maven_path(&name, Some(&classifier))?);
                }
            }
        }
        library["downloads"] = downloads;
    }
    if let Some(artifact) = library["downloads"]["artifact"].as_object_mut() {
        artifact.entry("path").or_insert(json!(path));
    }
    if let Some(classifiers) = library["downloads"]["classifiers"].as_object_mut() {
        for (classifier, artifact) in classifiers {
            if artifact["path"].is_null() {
                artifact["path"] = json!(maven_path(&name, Some(classifier))?);
            }
        }
    }
    if library["downloads"]["classifiers"].is_null() {
        library["downloads"]["classifiers"] = json!({});
    }
    if library["natives"].is_null() {
        library["natives"] = json!({});
    }
    if library["rules"].is_null() {
        library["rules"] = json!([]);
    }
    Ok(())
}

pub fn detect_loader(value: &Value) -> Option<ModLoader> {
    if let Some(args) = value["arguments"]["game"].as_array() {
        for (flag, kind) in [
            ("--fml.neoForgeVersion", "neoforge"),
            ("--fml.forgeVersion", "forge"),
        ] {
            if let Some(version) = args
                .windows(2)
                .find(|pair| pair[0].as_str() == Some(flag))
                .and_then(|pair| pair[1].as_str())
            {
                let kind = if kind == "forge"
                    && args.iter().any(|a| a.as_str() == Some("net.neoforged"))
                {
                    "neoforge"
                } else {
                    kind
                };
                return Some(ModLoader {
                    kind: kind.into(),
                    version: version.into(),
                });
            }
        }
    }
    let libraries = value["libraries"].as_array()?;
    for (kind, coordinates) in [
        (
            "neoforge",
            &["net.neoforged:neoforge:", "net.neoforged:forge:"][..],
        ),
        (
            "forge",
            &["net.minecraftforge:forge:", "net.minecraftforge:fmlloader:"][..],
        ),
        ("fabric", &["net.fabricmc:fabric-loader:"][..]),
        ("quilt", &["org.quiltmc:quilt-loader:"][..]),
    ] {
        for library in libraries {
            let Some(name) = library["name"].as_str() else {
                continue;
            };
            if let Some(version) = coordinates
                .iter()
                .find_map(|prefix| name.strip_prefix(prefix))
            {
                let version = version
                    .split(':')
                    .next()
                    .unwrap_or(version)
                    .split('@')
                    .next()
                    .unwrap_or(version);
                let version = if kind == "forge" || name.starts_with("net.neoforged:forge:") {
                    version
                        .split_once('-')
                        .map_or(version, |(_, loader)| loader)
                } else {
                    version
                };
                return Some(ModLoader {
                    kind: kind.into(),
                    version: version.into(),
                });
            }
        }
    }
    None
}

fn game_version(value: &Value, base: &str, loader: Option<&ModLoader>) -> String {
    if let Some(patches) = value["patches"].as_array()
        && let Some(game) = patches
            .iter()
            .find(|p| p["id"].as_str() == Some("game"))
            .and_then(|p| p["version"].as_str())
    {
        return game.into();
    }
    if loader.is_some() {
        if let Some(args) = value["arguments"]["game"].as_array() {
            for pair in args.windows(2) {
                if pair[0].as_str() == Some("--fml.mcVersion")
                    && let Some(version) = pair[1].as_str()
                {
                    return version.into();
                }
            }
        }
        if let Some(libraries) = value["libraries"].as_array() {
            for library in libraries {
                let name = library["name"].as_str().unwrap_or("");
                if let Some(version) = name.strip_prefix("net.fabricmc:intermediary:") {
                    return version.into();
                }
                for prefix in [
                    "net.minecraftforge:forge:",
                    "net.minecraftforge:fmlloader:",
                    "net.neoforged:forge:",
                ] {
                    if let Some((game, _)) =
                        name.strip_prefix(prefix).and_then(|v| v.split_once('-'))
                    {
                        return game.into();
                    }
                }
            }
        }
    }
    base.into()
}

fn required_java(game: &str) -> u32 {
    let pieces: Vec<u32> = game.split('.').filter_map(|v| v.parse().ok()).collect();
    match pieces.as_slice() {
        [1, minor, rest @ ..]
            if *minor > 20 || (*minor == 20 && rest.first().copied().unwrap_or(0) >= 5) =>
        {
            21
        }
        [1, minor, ..] if *minor >= 18 => 17,
        [1, 17, ..] => 16,
        _ => 8,
    }
}

pub fn client_path(game: &Path, version: &Version) -> Result<PathBuf> {
    let id = version.jar.as_deref().unwrap_or(&version.id);
    storage::safe_version(id)?;
    Ok(game.join("versions").join(id).join(format!("{id}.jar")))
}

/// HMCL exports retain an isolated working directory inside versions/<id>.
/// Honour that layout without moving any saves/mods; ordinary profiles still
/// share their instance's .minecraft directory.
pub fn working_directory(game: &Path, version: &Version) -> Result<PathBuf> {
    storage::safe_version(&version.id)?;
    let directory = game.join("versions").join(&version.id);
    let config: Value =
        storage::read_json(&directory.join("hmclversion.cfg")).unwrap_or(Value::Null);
    if config["usesGlobal"].as_bool() == Some(false) {
        match config["gameDirType"].as_u64() {
            Some(0) => return Ok(game.to_owned()),
            Some(1) => return Ok(directory),
            Some(2) => {
                if let Some(path) = config["gameDir"].as_str().filter(|p| !p.is_empty()) {
                    let path = storage::expand_user_path(Path::new(path))?;
                    let path = if path.is_absolute() {
                        path
                    } else {
                        game.join(path)
                    };
                    if !path.is_dir() {
                        bail!("整合包的游戏目录不存在：{}", path.display());
                    }
                    return Ok(path);
                }
            }
            _ => (),
        }
    }
    if ["mods", "saves", "config"]
        .iter()
        .any(|name| directory.join(name).is_dir())
    {
        return Ok(directory);
    }
    Ok(game.to_owned())
}

pub fn local_library(root: &Path, game: &Path, artifact: &Artifact) -> Result<PathBuf> {
    let local = minecraft::library_path(game, artifact)?;
    let legacy = minecraft::library_path(root, artifact)?;
    Ok(if local.is_file() || !legacy.is_file() {
        local
    } else {
        legacy
    })
}

/// A quick readiness check, not a full repair/hash pass over thousands of assets.
pub fn check_launch_files(root: &Path, game: &Path, resolved: &ResolvedVersion) -> Result<()> {
    let version = &resolved.version;
    let runtime_arch = crate::java::JavaRequirement::for_version(version).arch;
    let mut files = vec![client_path(game, version)?];
    for library in version
        .libraries
        .iter()
        .filter(|l| minecraft::is_allowed_on(l, runtime_arch))
    {
        if let Some(a) = minecraft::classpath_artifact(library) {
            files.push(local_library(root, game, a)?);
        }
        if let Some(a) = minecraft::native_artifact(library) {
            files.push(local_library(root, game, a)?);
        }
    }
    for path in files {
        if !std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() > 0) {
            bail!("启动所需文件缺失：{}", path.display());
        }
    }
    Ok(())
}

pub fn describe(root: &Path, game: &Path, id: &str) -> InstanceVersion {
    if let Ok(managed) = crate::loader::describe(game, id) {
        return managed;
    }
    match resolve(root, game, id) {
        Ok(resolved) => InstanceVersion {
            id: id.into(),
            name: None,
            modpack: false,
            minecraft_version: Some(resolved.minecraft_version.clone()),
            loader: resolved.loader.clone(),
            installed: check_launch_files(root, game, &resolved).is_ok(),
            download_progress: 0.0,
        },
        Err(_) => {
            let value: Value = metadata_path(game, id)
                .ok()
                .and_then(|p| storage::read_json(&p).ok())
                .unwrap_or(Value::Null);
            let loader = detect_loader(&value);
            let game = game_version(
                &value,
                value["inheritsFrom"].as_str().unwrap_or(id),
                loader.as_ref(),
            );
            InstanceVersion {
                id: id.into(),
                minecraft_version: Some(game),
                loader,
                ..InstanceVersion::default()
            }
        }
    }
}
