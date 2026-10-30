use super::*;
use crate::archive_support;
use std::{
    collections::HashSet,
    io::{Read, Write},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedResource {
    pub path: PathBuf,
    pub name: String,
    pub kinds: Vec<ResourceKind>,
    pub format: String,
    pub issue: Option<String>,
    #[serde(skip)]
    pub(super) prefix: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct LocalImport {
    pub path: PathBuf,
    pub kind: Option<ResourceKind>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ResourceCopySource {
    pub kind: ResourceKind,
    pub instance: String,
    pub version: String,
    pub file: String,
}
const DESCRIPTORS: &[&str] = &[
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/mods.toml",
    "META-INF/neoforge.mods.toml",
    "mcmod.info",
];
const MAX_TOTAL: u64 = 8 * archive_support::MAX_ENTRY;

pub(super) fn inspect(path: &Path) -> Result<DetectedResource> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        bail!("不能导入符号链接，请选择实际文件或目录");
    }
    let name = path
        .file_name()
        .context("文件名为空")?
        .to_string_lossy()
        .into_owned();
    component(&name)?;
    let mut result = DetectedResource {
        path: path.to_owned(),
        name,
        kinds: Vec::new(),
        format: "unknown".into(),
        issue: None,
        prefix: None,
    };
    let mut names = HashSet::new();
    if meta.is_dir() {
        for marker in [
            "level.dat",
            "level.dat_old",
            "pack.mcmeta",
            "shaders",
            "data",
            "assets",
        ] {
            if path.join(marker).exists() {
                names.insert(marker.to_owned());
            }
        }
    } else {
        let filename = result.name.to_ascii_lowercase();
        let enabled_name = filename
            .strip_suffix(".disabled")
            .or_else(|| filename.strip_suffix(".off"))
            .unwrap_or(&filename);
        let extension = Path::new(if enabled_name.ends_with(".jar") {
            enabled_name
        } else {
            &filename
        })
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
        if !["jar", "zip", "mrpack"].contains(&extension.as_str()) {
            return Ok(result);
        }
        let archive = archive_support::open(path)?;
        names.extend(archive.file_names().map(str::to_owned));
        if names.contains("modrinth.index.json") || names.contains("manifest.json") {
            // A generic manifest.json is not necessarily a CurseForge modpack.
            let mut archive = archive_support::open(path)?;
            if names.contains("modrinth.index.json")
                || archive_support::json(&mut archive, "manifest.json")
                    .is_ok_and(|v| v["manifestType"] == "minecraftModpack")
            {
                result.format = "modpack".into();
                return Ok(result);
            }
        }
        if extension == "jar" && DESCRIPTORS.iter().any(|d| names.contains(*d)) {
            result.kinds.push(ResourceKind::Mods);
            result.format = "resource".into();
            return Ok(result);
        }
        if extension == "jar" {
            return Ok(result);
        }
        // Ignore macOS packaging metadata when discovering one wrapper folder.
        let relevant: Vec<_> = names
            .iter()
            .filter(|n| {
                !n.starts_with("__MACOSX/") && !n.ends_with(".DS_Store") && !n.ends_with('/')
            })
            .collect();
        if !relevant.iter().any(|n| {
            ["level.dat", "level.dat_old", "pack.mcmeta"].contains(&n.as_str())
                || n.starts_with("shaders/")
        }) {
            let roots: HashSet<_> = relevant
                .iter()
                .filter_map(|n| n.split_once('/').map(|(root, _)| root))
                .collect();
            if roots.len() == 1 && relevant.iter().all(|n| n.contains('/')) {
                let prefix = format!("{}/", roots.iter().next().unwrap());
                names = names
                    .iter()
                    .filter_map(|n| n.strip_prefix(&prefix).map(str::to_owned))
                    .collect();
                result.prefix = Some(prefix);
            }
        }
    }
    let has = |prefix: &str| {
        names.contains(prefix) || names.iter().any(|n| n.starts_with(&format!("{prefix}/")))
    };
    if has("level.dat") || has("level.dat_old") {
        result.kinds.push(ResourceKind::Saves);
    } else if has("shaders") {
        result.kinds.push(ResourceKind::Shaderpacks);
    } else if names.contains("pack.mcmeta") {
        if has("assets") {
            result.kinds.push(ResourceKind::Resourcepacks);
        }
        if has("data") {
            result.kinds.push(ResourceKind::Datapacks);
        }
    }
    if !result.kinds.is_empty() {
        result.format = "resource".into();
        if meta.is_file() && result.kinds == [ResourceKind::Saves] && result.prefix.is_none() {
            result.prefix = Some(String::new());
        }
    } else if meta.is_dir() {
        result.format = "directory".into();
    }
    Ok(result)
}

impl Launcher {
    /// Only export a resource that is actually listed in this game version.
    /// Never accept an arbitrary absolute path from the webview.
    pub fn resource_export_path(&self, source: &ResourceCopySource) -> Result<PathBuf> {
        use crate::version_management::ContentKind;
        let kind = match source.kind {
            ResourceKind::Mods => ContentKind::Mods,
            ResourceKind::Resourcepacks => ContentKind::Resourcepacks,
            ResourceKind::Shaderpacks => ContentKind::Shaderpacks,
            ResourceKind::Datapacks => ContentKind::Datapacks,
            ResourceKind::Saves => ContentKind::Saves,
        };
        Ok(self
            .version_content_path(&source.instance, &source.version, kind, &source.file, None)?
            .canonicalize()?)
    }

    /// Resolve a listed resource rather than trusting a frontend-supplied absolute path.
    /// Use the normal transactional installer; copying never removes the source.
    pub fn prepare_resource_copy(
        &self,
        source: &ResourceCopySource,
        id: &str,
        version: &str,
        cancel: &CancellationToken,
    ) -> Result<Option<ResourcePlan>> {
        self.idle(id)?;
        use crate::version_management::ContentKind;
        let kind = match source.kind {
            ResourceKind::Mods => ContentKind::Mods,
            ResourceKind::Resourcepacks => ContentKind::Resourcepacks,
            ResourceKind::Shaderpacks => ContentKind::Shaderpacks,
            _ => bail!("此资源类型不支持跨版本复制"),
        };
        let path =
            self.version_content_path(&source.instance, &source.version, kind, &source.file, None)?;
        let target = self.resource_target(id, version)?;
        let origin = self.resource_target(&source.instance, &source.version)?;
        if origin.directory == target.directory {
            return Ok(None);
        }
        let plan = self.prepare_local_resources_inner(
            id,
            version,
            &[LocalImport {
                path,
                kind: Some(source.kind),
            }],
            None,
            cancel,
            source.kind == ResourceKind::Mods,
        )?;
        if !plan.conflicts.is_empty() || plan.entries.iter().all(|entry| entry.reuse) {
            // This new plan has no staged data. Failed/idempotent drags must not
            // accumulate unfinished tasks in the target version.
            fs::remove_dir_all(task_dir(&plan.target, &plan.id)?)?;
            if !plan.conflicts.is_empty() {
                bail!("{}", plan.conflicts.join("\n"));
            }
            return Ok(None);
        }
        Ok(Some(plan))
    }

    pub fn inspect_resources(&self, paths: &[PathBuf]) -> Result<Vec<DetectedResource>> {
        if paths.is_empty() || paths.len() > 256 {
            bail!("每次可导入 1–256 个文件或目录");
        }
        Ok(paths
            .iter()
            .map(|path| {
                inspect(path).unwrap_or_else(|error| DetectedResource {
                    path: path.clone(),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    kinds: Vec::new(),
                    format: "unknown".into(),
                    issue: Some(error.to_string()),
                    prefix: None,
                })
            })
            .collect())
    }
    pub fn prepare_local_resources(
        &self,
        id: &str,
        version: &str,
        inputs: &[LocalImport],
        world: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ResourcePlan> {
        self.prepare_local_resources_inner(id, version, inputs, world, cancel, false)
    }
    fn prepare_local_resources_inner(
        &self,
        id: &str,
        version: &str,
        inputs: &[LocalImport],
        world: Option<&str>,
        cancel: &CancellationToken,
        listed_mod: bool,
    ) -> Result<ResourcePlan> {
        self.idle(id)?;
        if inputs.is_empty() || inputs.len() > 256 {
            bail!("每次可导入 1–256 个文件或目录");
        }
        let mut plan = new_plan(self.resource_target(id, version)?);
        for input in inputs {
            let mut detected = inspect(&input.path)?;
            // Legacy/core Mods may lack loader descriptors. A file already
            // resolved from a version's Mods list is still a valid copy source.
            if listed_mod && detected.kinds.is_empty() && detected.format == "unknown" {
                archive_support::open(&input.path)?;
                detected.kinds.push(ResourceKind::Mods);
            }
            let kind = input
                .kind
                .or_else(|| (detected.kinds.len() == 1).then(|| detected.kinds[0]))
                .context("无法判断资源类型，请选择资源包或数据包")?;
            if !detected.kinds.contains(&kind) {
                bail!("文件内容与所选资源类型不符：{}", detected.name);
            }
            let path = fs::canonicalize(&input.path)?;
            let filename = if detected.prefix.is_some() {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                detected.name.clone()
            };
            let destination = destination(&plan.target, kind, &filename, world)?;
            let (digest, size) = fingerprint(&path, cancel)?;
            if kind == ResourceKind::Mods {
                let archive = archive_support::open(&path)?;
                let loaders: Vec<_> = [
                    ("fabric.mod.json", "fabric"),
                    ("quilt.mod.json", "quilt"),
                    ("META-INF/mods.toml", "forge"),
                    ("META-INF/neoforge.mods.toml", "neoforge"),
                    ("mcmod.info", "forge"),
                ]
                .into_iter()
                .filter(|(file, _)| archive.file_names().any(|name| name == *file))
                .map(|(_, loader)| loader)
                .collect();
                if !loaders.is_empty()
                    && !plan
                        .target
                        .loader
                        .as_ref()
                        .is_some_and(|l| loaders.contains(&l.kind.to_ascii_lowercase().as_str()))
                {
                    plan.conflicts.push(format!(
                        "Mod 加载器不匹配：{} ({})",
                        detected.name,
                        loaders.join(" / ")
                    ));
                }
                plan.warnings.push(format!(
                    "本地 Mod 的游戏版本与前置依赖请确认：{}",
                    detected.name
                ));
            }
            if kind == ResourceKind::Shaderpacks {
                plan.warnings
                    .push("光影需要游戏中已有兼容的 Iris 或 OptiFine".into());
            }
            let content_sha256 = detected.prefix.is_none().then(|| digest.clone());
            plan.entries.push(PlanEntry {
                kind,
                destination,
                size,
                origin: ResourceOrigin {
                    platform: Platform::Local,
                    project: None,
                    version: None,
                    name: detected.name,
                    dependencies: Vec::new(),
                },
                input: Input::Local {
                    path,
                    digest,
                    archive_prefix: detected.prefix,
                },
                reuse: false,
                content_sha256,
            });
        }
        install::preflight(&mut plan, cancel)?;
        plan.warnings.sort();
        plan.warnings.dedup();
        save_plan(&plan)?;
        Ok(plan)
    }
}

pub(super) fn tree(path: &Path) -> Result<Vec<(String, PathBuf, u64)>> {
    let mut files = Vec::new();
    let mut total = 0_u64;
    let mut names = HashSet::new();
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_depth(64)
    {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            bail!("资源目录包含符号链接");
        }
        if entry.path() == path {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(path)?
            .to_str()
            .context("文件名不是有效的 UTF-8")?
            .replace('\\', "/");
        archive_support::relative(&relative)?;
        if !names.insert(relative.to_lowercase()) {
            bail!("资源目录包含大小写冲突");
        }
        if entry.file_type().is_dir() {
            if entry.depth() == 64 {
                bail!("资源目录层数过多");
            }
            continue;
        }
        if !entry.file_type().is_file() {
            bail!("不支持的资源文件类型");
        }
        let size = entry.metadata()?.len();
        total = total.checked_add(size).context("资源大小无效")?;
        if size > archive_support::MAX_ENTRY || total > MAX_TOTAL || names.len() > 100_000 {
            bail!("资源目录过大");
        }
        files.push((relative, entry.path().to_owned(), size));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}
pub(super) fn fingerprint(path: &Path, cancel: &CancellationToken) -> Result<(String, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        bail!("资源路径包含符号链接");
    }
    let directory = metadata.is_dir();
    let files = if directory {
        tree(path)?
    } else {
        if !metadata.is_file() || metadata.len() > MAX_TOTAL {
            bail!("无效资源文件或文件过大");
        }
        vec![(String::new(), path.to_owned(), metadata.len())]
    };
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut total = 0;
    let mut buffer = [0; 64 * 1024];
    for (relative, file, size) in files {
        if directory {
            digest.update(relative.as_bytes());
            digest.update(&[0]);
            digest.update(&size.to_le_bytes());
        }
        let mut source = fs::File::open(file)?;
        let mut read = 0;
        loop {
            if cancel.is_cancelled() {
                bail!("操作已取消");
            }
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read += count as u64;
            if read > size {
                bail!("资源在读取时发生变化");
            }
            digest.update(&buffer[..count]);
        }
        if read != size {
            bail!("资源在读取时发生变化");
        }
        total += size;
    }
    Ok((hex::encode(digest.finish()), total))
}
fn copy(
    mut source: impl Read,
    target: &Path,
    size: u64,
    cancel: &CancellationToken,
    progress: &dyn Fn(u64),
) -> Result<()> {
    fs::create_dir_all(target.parent().context("无效目标目录")?)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0; 64 * 1024];
    let mut written = 0;
    let mut last = std::time::Instant::now();
    loop {
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > size {
            bail!("源文件在预览后发生变化，请重新导入");
        }
        file.write_all(&buffer[..n])?;
        if last.elapsed() >= std::time::Duration::from_millis(100) {
            progress(written);
            last = std::time::Instant::now();
        }
    }
    if written != size {
        bail!("资源文件不完整");
    }
    file.sync_all()?;
    progress(written);
    Ok(())
}
pub(super) fn stage(
    entry: &PlanEntry,
    target: &Path,
    cancel: &CancellationToken,
    progress: &dyn Fn(u64, u64),
) -> Result<u64> {
    let Input::Local {
        path,
        digest,
        archive_prefix,
    } = &entry.input
    else {
        bail!("不是本地资源");
    };
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("资源路径包含符号链接");
    }
    let mut done = 0;
    if path.is_dir() {
        fs::create_dir_all(target)?;
        let files = tree(path)?;
        let total = files.iter().map(|(_, _, size)| size).sum();
        for (relative, file, size) in files {
            copy(
                fs::File::open(file)?,
                &safe_join(target, &relative)?,
                size,
                cancel,
                &|written| progress(done + written, total),
            )?;
            done += size;
        }
    } else if let Some(prefix) = archive_prefix {
        fs::create_dir_all(target)?;
        let mut archive = archive_support::open(path)?;
        let mut total = 0;
        for index in 0..archive.len() {
            let file = archive.by_index(index)?;
            if file.name().starts_with(prefix) && !file.is_dir() {
                total += file.size();
            }
        }
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            let Some(relative) = entry.name().strip_prefix(prefix) else {
                continue;
            };
            if relative.is_empty()
                || entry.is_dir()
                || relative.starts_with("__MACOSX/")
                || relative.ends_with(".DS_Store")
            {
                continue;
            }
            let destination = safe_join(target, relative)?;
            let size = entry.size();
            copy(entry, &destination, size, cancel, &|written| {
                progress(done + written, total)
            })?;
            done += size;
        }
    } else {
        copy(
            fs::File::open(path)?,
            target,
            entry.size,
            cancel,
            &|written| progress(written, entry.size),
        )?;
        done = entry.size;
    }
    // Plain files and directories are checked against the preview digest at commit.
    // For extraction the input and output have different representations; validate the archive too.
    if archive_prefix.is_some() && fingerprint(path, cancel)?.0 != *digest {
        bail!("源文件在导入时发生变化，请重新导入");
    }
    Ok(done)
}
