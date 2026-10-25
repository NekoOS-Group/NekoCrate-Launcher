use super::*;
pub(super) use crate::archive_support::{MAX_ENTRY, component, json, open, relative};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
};

pub(super) fn content_path(name: &str) -> Result<PathBuf> {
    let path = relative(name)?;
    let top = name.split('/').next().unwrap_or("").to_ascii_lowercase();
    if [
        "versions",
        "libraries",
        "assets",
        "natives",
        "runtime.json",
        "hmclversion.cfg",
        "nekocrate-pack.json",
        ".nekocrate",
    ]
    .contains(&top.as_str())
        || top.starts_with("launcher_")
        || (!name.contains('/') && name.ends_with(".json") && top.starts_with("pack-"))
    {
        bail!("整合包不能覆盖启动器管理的文件：{name}");
    }
    Ok(path)
}
pub(super) fn hash(value: &Value, key: &str, len: usize) -> Result<String> {
    let text = value[key].as_str().context("整合包缺少文件校验值")?;
    if text.len() != len || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("整合包文件校验值无效");
    }
    Ok(text.to_lowercase())
}
pub(super) fn https(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        bail!("整合包下载地址必须为 HTTPS 且不含凭据");
    }
    Ok(())
}
pub(super) fn parse(path: &Path) -> Result<Pack> {
    let mut zip = open(path)?;
    let mr = zip.index_for_name("modrinth.index.json").is_some();
    let cf = zip.index_for_name("manifest.json").is_some();
    if mr == cf {
        bail!("请选择含有唯一 Modrinth 或 CurseForge 清单的整合包");
    }
    let source = if mr {
        "modrinth.index.json"
    } else {
        "manifest.json"
    };
    let value = json(&mut zip, source)?;
    let name = value["name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
        .context("整合包缺少名称")?
        .to_owned();
    let mut pack = Pack {
        format: if mr { "modrinth" } else { "curseforge" }.into(),
        name,
        version: String::new(),
        minecraft: String::new(),
        loader: None,
        files: vec![],
        overrides: vec![],
        manifest: value.clone(),
    };
    if mr {
        if value["formatVersion"] != 1 || value["game"] != "minecraft" {
            bail!("不支持此 Modrinth 整合包格式版本");
        }
        pack.version = value["versionId"].as_str().unwrap_or("").into();
        pack.minecraft = component(
            value["dependencies"]["minecraft"]
                .as_str()
                .context("缺少 Minecraft 版本")?,
        )?;
        for (key, version) in value["dependencies"]
            .as_object()
            .context("整合包依赖无效")?
        {
            if key == "minecraft" {
                continue;
            }
            let kind = match key.as_str() {
                "fabric-loader" => "fabric",
                "forge" => "forge",
                "neoforge" => "neoforge",
                _ => bail!("暂不支持整合包加载器：{key}"),
            };
            if pack.loader.is_some() {
                bail!("整合包声明了多个加载器");
            }
            pack.loader = Some(ModLoader {
                kind: kind.into(),
                version: component(version.as_str().context("加载器版本无效")?)?,
            });
        }
        for item in value["files"].as_array().context("整合包文件列表无效")? {
            let env = item["env"]["client"].as_str().unwrap_or("required");
            if env == "unsupported" {
                continue;
            }
            if !["required", "optional"].contains(&env) {
                bail!("整合包客户端环境声明无效");
            }
            let path = item["path"].as_str().context("整合包文件路径缺失")?;
            content_path(path)?;
            let urls: Vec<String> = serde_json::from_value(item["downloads"].clone())?;
            if urls.is_empty() {
                bail!("整合包文件缺少下载地址");
            }
            for url in &urls {
                https(url)?;
            }
            pack.files.push(PackFile {
                key: path.into(),
                display_name: None,
                path: Some(path.into()),
                urls,
                sha1: hash(&item["hashes"], "sha1", 40)?,
                sha512: hash(&item["hashes"], "sha512", 128)?,
                size: item["fileSize"].as_u64().context("整合包文件缺少大小")?,
                optional: env == "optional",
                project: None,
                file: None,
                manual_url: None,
            });
        }
        pack.overrides = vec!["overrides".into(), "client-overrides".into()];
    } else {
        if value["manifestType"] != "minecraftModpack" || value["manifestVersion"] != 1 {
            bail!("不支持此 CurseForge 整合包格式版本");
        }
        pack.version = value["version"].as_str().unwrap_or("").into();
        pack.minecraft = component(
            value["minecraft"]["version"]
                .as_str()
                .context("缺少 Minecraft 版本")?,
        )?;
        let loaders = value["minecraft"]["modLoaders"]
            .as_array()
            .context("整合包加载器声明无效")?;
        let selected = loaders
            .iter()
            .find(|l| l["primary"] == true)
            .or_else(|| (loaders.len() == 1).then(|| &loaders[0]));
        if !loaders.is_empty() && selected.is_none() {
            bail!("整合包未指定主加载器");
        }
        if let Some(loader) = selected {
            let id = loader["id"].as_str().context("加载器 ID 无效")?;
            let (kind, version) = id.split_once('-').context("加载器 ID 无效")?;
            if !["forge", "neoforge", "fabric"].contains(&kind) {
                bail!("暂不支持整合包加载器：{kind}");
            }
            pack.loader = Some(ModLoader {
                kind: kind.into(),
                version: component(version)?,
            });
        }
        for item in value["files"].as_array().context("整合包文件列表无效")? {
            let project = item["projectID"]
                .as_u64()
                .filter(|n| *n > 0)
                .context("CurseForge 项目 ID 无效")?;
            let file = item["fileID"]
                .as_u64()
                .filter(|n| *n > 0)
                .context("CurseForge 文件 ID 无效")?;
            pack.files.push(PackFile {
                key: format!("{project}:{file}"),
                display_name: None,
                path: None,
                urls: vec![],
                sha1: String::new(),
                sha512: String::new(),
                size: 0,
                optional: item["required"] == false,
                project: Some(project),
                file: Some(file),
                manual_url: Some(format!(
                    "https://www.curseforge.com/minecraft/mc-mods/{project}/files/{file}"
                )),
            });
        }
        let prefix = value["overrides"].as_str().unwrap_or("overrides");
        relative(prefix)?;
        pack.overrides = vec![prefix.trim_end_matches('/').into()];
    }
    let mut seen = HashSet::new();
    for file in &pack.files {
        if file.size > MAX_ENTRY || !seen.insert(file.key.to_lowercase()) {
            bail!("整合包文件重复或超过大小限制");
        }
    }
    // Preflight all overrides before creating a destination.
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        for prefix in &pack.overrides {
            if let Some(name) = entry.name().strip_prefix(&format!("{prefix}/"))
                && !name.is_empty()
            {
                content_path(name)?;
            }
        }
    }
    Ok(pack)
}
/// Reject links in every existing component, including paths created since preflight.
pub(super) fn destination(root: &Path, relative_path: &str) -> Result<PathBuf> {
    content_path(relative_path)?;
    managed_destination(root, relative_path)
}
pub(super) fn managed_destination(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = relative(relative_path)?;
    let mut path = root.to_path_buf();
    if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
        bail!("整合包安装目录不能是符号链接");
    }
    for component in relative.components() {
        path.push(component);
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            bail!("整合包目标包含符号链接");
        }
    }
    Ok(path)
}
pub(super) fn override_paths(path: &Path, pack: &Pack) -> Result<HashSet<String>> {
    let mut zip = open(path)?;
    let mut paths = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        for prefix in &pack.overrides {
            if let Some(name) = entry.name().strip_prefix(&format!("{prefix}/")) {
                content_path(name)?;
                paths.insert(name.to_lowercase());
            }
        }
    }
    Ok(paths)
}
pub(super) fn extract(
    path: &Path,
    pack: &Pack,
    target: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut zip = open(path)?;
    for prefix in &pack.overrides {
        for index in 0..zip.len() {
            crate::download::check_cancel(cancel)?;
            let mut entry = zip.by_index(index)?;
            let Some(name) = entry.name().strip_prefix(&format!("{prefix}/")) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let out = destination(target, name)?;
            if entry.is_dir() {
                std::fs::create_dir_all(out)?;
                continue;
            }
            std::fs::create_dir_all(out.parent().unwrap())?;
            let mut file = File::create(out)?;
            let mut buffer = [0_u8; 64 * 1024];
            let expected = entry.size();
            let mut written = 0_u64;
            loop {
                crate::download::check_cancel(cancel)?;
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                written += n as u64;
                if written > expected || written > MAX_ENTRY {
                    bail!("整合包文件解压大小不匹配");
                }
                file.write_all(&buffer[..n])?;
            }
        }
    }
    storage::write_json(
        &target.join(if pack.format == "modrinth" {
            "modrinth.index.json"
        } else {
            "manifest.json"
        }),
        &pack.manifest,
    )?;
    Ok(())
}
