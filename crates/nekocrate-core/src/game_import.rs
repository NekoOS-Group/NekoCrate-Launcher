//! Classify game trees before resource detection; import into explicit destinations.
use crate::{Launcher, archive_support, download::check_cancel, model::*, storage, version};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveDirectory {
    pub prefix: String,
    pub name: String,
}

/// Only a matching profile proves that this is a version, rather than an
/// arbitrary folder named "1.21" or a world containing a mods directory.
pub(crate) fn directory_version(path: &Path) -> Option<String> {
    let id = path.file_name()?.to_str()?;
    storage::safe_version(id).ok()?;
    let read = |file: &Path| -> Option<Value> {
        if fs::metadata(file).ok()?.len() > 8 * 1024 * 1024 {
            return None;
        }
        storage::read_json(file).ok()
    };
    if read(&path.join(format!("{id}.json"))).is_some_and(|v| v["id"].as_str() == Some(id)) {
        return Some(id.to_owned());
    }
    // A paused pack/loader may not have generated its launch profile yet. Its
    // portable installation record is still a version, never an empty game root.
    for record in ["modpack", "loader"] {
        if let Some(value) = read(&path.join(format!(".nekocrate/{record}.json"))) {
            let valid = value["version"].as_str() == Some(id)
                && if record == "modpack" {
                    value["archive"] == "source.zip" && value["pack"].is_object()
                } else {
                    serde_json::from_value::<crate::loader::InstallRecord>(value.clone()).is_ok()
                };
            if valid {
                return Some(id.to_owned());
            }
        }
    }
    None
}

pub(crate) fn has_version_profile(path: &Path) -> bool {
    path.file_name().is_some_and(|id| {
        path.join(format!("{}.json", id.to_string_lossy()))
            .is_file()
    }) || path.join(".nekocrate/modpack.json").is_file()
        || path.join(".nekocrate/loader.json").is_file()
}

/// Returns the owning game directory and, for a single version, its selection.
/// Never climb arbitrary ancestors: only the standard versions/<id> layout.
pub(crate) fn directory_owner(path: &Path) -> Option<(PathBuf, Option<String>)> {
    let (versions, selected) = if path.file_name()?.eq_ignore_ascii_case("versions") {
        (path, None)
    } else {
        let id = directory_version(path)?;
        let parent = path.parent()?;
        if !parent.file_name()?.eq_ignore_ascii_case("versions") {
            return None;
        }
        (parent, Some(id))
    };
    let game = versions.parent()?;
    let standard = game.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case(".minecraft") || name.eq_ignore_ascii_case("minecraft")
    });
    let valid = selected.is_some()
        || standard
        || fs::read_dir(versions)
            .ok()?
            .filter_map(Result::ok)
            .any(|entry| {
                entry.file_type().is_ok_and(|t| t.is_dir())
                    && directory_version(&entry.path()).is_some()
            });
    (versions.is_dir() && valid).then(|| (game.to_owned(), selected))
}

pub(crate) fn archive_directories<'a>(
    names: impl Iterator<Item = &'a str>,
    fallback: &str,
) -> Vec<ArchiveDirectory> {
    let mut prefixes = BTreeSet::new();
    for name in names {
        if name.starts_with("__MACOSX/") {
            continue;
        }
        let parts: Vec<_> = name.split('/').collect();
        if let Some(index) = parts
            .iter()
            .position(|p| p.eq_ignore_ascii_case(".minecraft"))
        {
            let content = &parts[index + 1..];
            if content
                .first()
                .is_some_and(|p| matches!(*p, "versions" | "saves" | "mods" | "options.txt"))
            {
                prefixes.insert(format!("{}/", parts[..=index].join("/")));
            }
        }
    }
    prefixes
        .into_iter()
        .map(|prefix| {
            let name = Path::new(prefix.trim_end_matches('/'))
                .parent()
                .and_then(Path::file_name)
                .and_then(|p| p.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback)
                .chars()
                .take(80)
                .collect();
            ArchiveDirectory { prefix, name }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct GameImport {
    pub path: PathBuf,
    /// Present only for a full .minecraft ZIP, selected from the read-only scan.
    pub archive_prefix: Option<String>,
    /// Versions may be copied into an existing instance. ZIPs always create one.
    pub target: Option<String>,
    pub name: String,
    pub directory: Option<PathBuf>,
    pub icon: Option<u8>,
}

pub(crate) fn excluded(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some(
                "launcher_accounts.json"
                    | "launcher_profiles.json"
                    | "accounts.json"
                    | "launcher_accounts_microsoft_store.json"
                    | "launcher_profiles_microsoft_store.json"
                    | "usercache.json"
                    | ".git"
                    | ".DS_Store"
                    | ".nekocrate.lock"
            )
        )
    })
}

/// Tracks only files/directories created by this operation. A cancelled import
/// must not remove pre-existing data, including a selected external parent.
#[derive(Default)]
struct Writes {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    committed: bool,
}
impl Writes {
    fn directory(&mut self, path: &Path) -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    bail!("导入目标不是普通目录：{}", path.display());
                }
                for ancestor in path.ancestors().skip(1) {
                    if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
                        bail!("导入目标包含符号链接：{}", ancestor.display());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.directory(path.parent().context("无效的导入目录")?)?;
                fs::create_dir(path)?;
                self.dirs.push(path.to_owned());
            }
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
    fn file(&mut self, path: &Path) -> Result<File> {
        self.directory(path.parent().context("无效的导入文件")?)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("无法创建文件（不会覆盖已有文件）：{}", path.display()))?;
        self.files.push(path.to_owned());
        Ok(file)
    }
}
impl Drop for Writes {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for path in self.files.iter().rev() {
            let _ = fs::remove_file(path);
        }
        for path in self.dirs.iter().rev() {
            let _ = fs::remove_dir(path);
        }
    }
}

struct Progress<'a> {
    cancel: &'a CancellationToken,
    sink: &'a EventSink,
    total: u64,
    bytes: u64,
    last: Instant,
}
impl Progress<'_> {
    fn copy(
        &mut self,
        mut source: impl Read,
        mut target: File,
        size: u64,
        name: &str,
    ) -> Result<()> {
        let mut buffer = vec![0; 128 * 1024];
        let mut written = 0;
        loop {
            check_cancel(self.cancel)?;
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            written += count as u64;
            if written > size {
                bail!("来源文件已变化，请重新导入");
            }
            target.write_all(&buffer[..count])?;
            self.bytes += count as u64;
            if self.last.elapsed().as_millis() >= 100 {
                self.emit(name);
                self.last = Instant::now();
            }
        }
        if written != size {
            bail!("来源文件不完整，请重新导入");
        }
        Ok(())
    }
    fn emit(&self, name: &str) {
        let mut event = Event::new("progress", name);
        event.step = Some("import_files".into());
        event.bytes = self.bytes;
        event.total_bytes = self.total;
        event.overall_progress = Some(self.bytes as f64 / self.total.max(1) as f64 * 95.0);
        (self.sink)(event);
    }
}

impl Launcher {
    /// Run on a blocking worker. Inspection and extraction revalidate the same
    /// archive; no temporary copy is retained in the launcher's download cache.
    pub fn import_game(
        &self,
        request: GameImport,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<Instance> {
        self.import_game_inner(request, false, cancel, sink)
    }
    pub(crate) fn import_game_inner(
        &self,
        request: GameImport,
        include_shared: bool,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<Instance> {
        let _operation = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后导入")?;
        check_cancel(cancel)?;
        if request.icon.is_some_and(|i| i >= INSTANCE_ICON_COUNT) {
            bail!("请选择有效的实例图标");
        }
        sink(Event::workflow("import_check", "检查导入内容"));
        let source = storage::expand_user_path(&request.path)?
            .canonicalize()
            .context("无法访问导入来源")?;
        let mut archive = if request.archive_prefix.is_some() {
            if request.target.is_some() {
                bail!("完整游戏目录需要新建实例，不能合并到已有实例");
            }
            Some(archive_support::open(&source)?)
        } else {
            None
        };
        let selected = if let Some(zip) = &archive {
            let prefix = request.archive_prefix.as_deref().unwrap();
            if !archive_directories(zip.file_names(), "Minecraft")
                .iter()
                .any(|item| item.prefix == prefix)
            {
                bail!("压缩包中的游戏目录已变化，请重新拖入");
            }
            None
        } else {
            Some(directory_version(&source).context("请选择包含同名版本 JSON 的版本目录")?)
        };
        let instance = if let Some(id) = &request.target {
            self.instance(id)?
        } else {
            self.create_instance_at_unlocked(&request.name, "", request.directory.as_deref())?
        };
        let created = request.target.is_none();
        let game = instance.game_dir(self.root());
        let mut writes = Writes::default();
        // External creation owns this one new .minecraft directory, not its parent.
        if created {
            writes.dirs.push(game.clone());
        }
        let result = (|| -> Result<Instance> {
            let _directory_guard = self.idle(&instance.id)?;
            if game.starts_with(&source) || source.starts_with(&game) {
                bail!("来源与目标目录不能重叠");
            }
            sink(Event::workflow("import_files", "导入游戏文件"));
            let mut progress = Progress {
                cancel,
                sink,
                total: 0,
                bytes: 0,
                last: Instant::now(),
            };
            if let Some(zip) = &mut archive {
                let prefix = request.archive_prefix.as_deref().unwrap();
                let mut entries = Vec::new();
                for index in 0..zip.len() {
                    check_cancel(cancel)?;
                    let entry = zip.by_index(index)?;
                    let Some(relative) =
                        entry.name().strip_prefix(prefix).filter(|p| !p.is_empty())
                    else {
                        continue;
                    };
                    let relative = archive_support::relative(relative)?;
                    if excluded(&relative) {
                        continue;
                    }
                    progress.total += entry.size();
                    entries.push((index, relative));
                }
                for (index, relative) in entries {
                    check_cancel(cancel)?;
                    let entry = zip.by_index(index)?;
                    let path = game.join(&relative);
                    if entry.is_dir() {
                        writes.directory(&path)?;
                    } else {
                        let size = entry.size();
                        let file = writes.file(&path)?;
                        progress.copy(entry, file, size, &relative.to_string_lossy())?;
                    }
                }
            } else {
                let id = selected.as_deref().unwrap();
                let destination = game.join("versions").join(id);
                let name_key = id.to_lowercase();
                let collision = fs::read_dir(game.join("versions"))
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().to_lowercase() == name_key);
                if destination.try_exists()?
                    || collision
                    || instance
                        .versions
                        .iter()
                        .any(|v| v.id.to_lowercase() == name_key)
                {
                    bail!("目标实例已有同名版本：{id}");
                }
                let mut files = self.version_import_files(&source, &game, cancel)?;
                if include_shared {
                    self.append_version_working_files(&source, &instance, &mut files, cancel)?;
                }
                progress.total = files.iter().map(|(_, _, size)| size).sum();
                for (from, relative, size) in files {
                    check_cancel(cancel)?;
                    if from.is_dir() {
                        writes.directory(&game.join(&relative))?;
                        continue;
                    }
                    let file = writes.file(&game.join(&relative))?;
                    progress.copy(File::open(from)?, file, size, &relative.to_string_lossy())?;
                }
            }
            progress.emit("导入游戏文件");
            check_cancel(cancel)?;
            sink(Event::workflow("import_register", "读取版本并关联实例"));
            let mut versions = self.discover_versions_cancellable(&game, "", Some(cancel))?;
            for entry in &mut versions {
                if !entry.installed || instance.versions.iter().any(|v| v.id == entry.id) {
                    continue;
                }
                let ready = version::resolve(self.root(), &game, &entry.id).and_then(|resolved| {
                    version::check_launch_files(self.root(), &game, &resolved)
                });
                if ready.is_err() {
                    entry.installed = false;
                    entry.download_progress = 0.0;
                    // A completed record describes its old location. Keep the
                    // copied record resumable when shared dependencies are absent.
                    for name in ["loader", "modpack"] {
                        let path = game
                            .join("versions")
                            .join(&entry.id)
                            .join(format!(".nekocrate/{name}.json"));
                        if writes.files.contains(&path) {
                            let mut record: Value = storage::read_json(&path)?;
                            record["completed"] = Value::Bool(false);
                            fs::write(path, serde_json::to_vec_pretty(&record)?)?;
                        }
                    }
                }
            }
            check_cancel(cancel)?;
            if selected
                .as_ref()
                .is_some_and(|id| !versions.iter().any(|v| &v.id == id))
            {
                bail!("导入后无法识别版本配置");
            }
            self.mutate(|db| {
                let item = db
                    .instances
                    .iter_mut()
                    .find(|i| i.id == instance.id)
                    .context("实例不存在")?;
                for version in versions {
                    if !item.versions.iter().any(|v| v.id == version.id) {
                        item.versions.push(version);
                    }
                }
                item.selected_version = selected
                    .clone()
                    .or_else(|| item.versions.first().map(|v| v.id.clone()));
                if created && request.icon.is_some() {
                    item.icon = request.icon;
                }
                Ok(item.clone())
            })
        })();
        if result.is_ok() {
            writes.committed = true;
        }
        drop(writes);
        if result.is_err() && created {
            self.rollback_new(&instance.id)
                .context("导入失败且无法清理实例登记")?;
        }
        result
    }

    /// Copy the selected version and available inherited profiles/dependencies.
    /// Shared Mods, options and worlds never leak into an unrelated target.
    pub(crate) fn version_import_files(
        &self,
        source: &Path,
        target: &Path,
        cancel: &CancellationToken,
    ) -> Result<Vec<(PathBuf, PathBuf, u64)>> {
        let id = directory_version(source).context("版本配置无效")?;
        let config: Value =
            storage::read_json(&source.join("hmclversion.cfg")).unwrap_or(Value::Null);
        if config["usesGlobal"] == false && config["gameDirType"] == 2 {
            bail!("此版本使用外部游戏目录，请关联原游戏目录以保留配置和存档");
        }
        let owner = directory_owner(source).map(|(root, _)| root);
        let mut trees = vec![(source.to_owned(), PathBuf::from("versions").join(&id))];
        let mut shared = Vec::new();
        if let Some(game) = &owner {
            let mut pending = vec![id.clone()];
            let mut seen = HashSet::new();
            while let Some(current) = pending.pop() {
                check_cancel(cancel)?;
                if !seen.insert(current.clone()) {
                    continue;
                }
                if seen.len() > 32 {
                    bail!("版本继承层数过多");
                }
                let path = version::metadata_path(game, &current)?;
                if current != id {
                    shared.push(
                        PathBuf::from("versions")
                            .join(&current)
                            .join(format!("{current}.jar")),
                    );
                }
                let read_path = if path.is_file() {
                    path.clone()
                } else {
                    self.root()
                        .join("metadata/versions")
                        .join(format!("{current}.json"))
                };
                let Ok(value) = storage::read_json::<Value>(&read_path) else {
                    continue;
                };
                if value["id"].as_str() != Some(&current) {
                    bail!("版本元数据与所选版本不一致：{current}");
                }
                if let Some(parent) = value["inheritsFrom"].as_str().filter(|p| *p != current) {
                    storage::safe_version(parent)?;
                    pending.push(parent.into());
                }
                if let Some(client) = value["jar"].as_str().filter(|p| *p != current) {
                    storage::safe_version(client)?;
                    shared.push(
                        PathBuf::from("versions")
                            .join(client)
                            .join(format!("{client}.jar")),
                    );
                }
                if current != id {
                    let dest = target.join("versions").join(&current);
                    if dest.join(format!("{current}.json")).is_file() {
                        let existing: Value =
                            storage::read_json(&dest.join(format!("{current}.json")))?;
                        if existing != value {
                            bail!("目标实例的基础版本配置冲突：{current}");
                        }
                    } else {
                        // Inherited versions only supply launch files, never their worlds/mods.
                        shared.push(path.strip_prefix(game)?.to_owned());
                        shared.push(
                            PathBuf::from("versions")
                                .join(&current)
                                .join(format!("{current}.jar")),
                        );
                    }
                }
            }
            if let Ok(resolved) = version::resolve(self.root(), game, &id) {
                if let Some(file) = resolved
                    .version
                    .logging
                    .as_ref()
                    .and_then(|value| value["client"]["file"]["id"].as_str())
                {
                    storage::safe_version(file)?;
                    shared.push(PathBuf::from("assets/log_configs").join(file));
                }
                for library in &resolved.version.libraries {
                    for artifact in library
                        .downloads
                        .artifact
                        .iter()
                        .chain(library.downloads.classifiers.values())
                    {
                        let path = crate::minecraft::library_path(game, artifact)?;
                        shared.push(path.strip_prefix(game)?.to_owned());
                    }
                }
                {
                    let index = &resolved.version.asset_index;
                    storage::safe_version(&index.id)?;
                    let relative =
                        PathBuf::from("assets/indexes").join(format!("{}.json", index.id));
                    if let Ok(value) = storage::read_json::<Value>(&game.join(&relative))
                        .or_else(|_| storage::read_json(&self.root().join(&relative)))
                        && let Some(objects) = value["objects"].as_object()
                    {
                        for object in objects.values() {
                            if let Some(hash) = object["hash"].as_str().filter(|h| {
                                h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit())
                            }) {
                                shared.push(
                                    PathBuf::from("assets/objects").join(&hash[..2]).join(hash),
                                );
                            }
                        }
                    }
                    shared.push(relative);
                }
            }
        }
        let mut files = Vec::new();
        for (from, relative) in trees.drain(..) {
            for entry in walkdir::WalkDir::new(&from).follow_links(false) {
                check_cancel(cancel)?;
                let entry = entry?;
                let path = entry.path().strip_prefix(&from)?;
                if excluded(path) {
                    continue;
                }
                if entry.file_type().is_symlink() {
                    bail!("版本目录包含符号链接：{}", entry.path().display());
                }
                let dest = relative.join(path);
                archive_support::relative(&dest.to_string_lossy().replace('\\', "/"))?;
                if entry.file_type().is_file() {
                    files.push((entry.path().to_owned(), dest, entry.metadata()?.len()));
                } else if entry.file_type().is_dir() {
                    files.push((entry.path().to_owned(), dest, 0));
                }
            }
        }
        if let Some(game) = owner {
            shared.sort();
            shared.dedup();
            for relative in shared {
                check_cancel(cancel)?;
                if target.join(&relative).try_exists()? {
                    continue;
                }
                let local = game.join(&relative);
                let legacy = if relative.starts_with("versions")
                    && relative.extension().is_some_and(|e| e == "json")
                {
                    self.root()
                        .join("metadata/versions")
                        .join(relative.file_name().unwrap())
                } else {
                    self.root().join(&relative)
                };
                let (path, root) = if local.is_file() {
                    (local, &game)
                } else {
                    (legacy, &self.0.root)
                };
                if !path.is_file() {
                    continue;
                }
                let canonical = path.canonicalize()?;
                if !canonical.starts_with(root) || canonical != path {
                    bail!("版本依赖包含符号链接：{}", path.display());
                }
                files.push((path.clone(), relative, fs::metadata(path)?.len()));
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests;
