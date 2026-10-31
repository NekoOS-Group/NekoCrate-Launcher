//! Manage local profiles without rewriting loader IDs or shared game files.
use crate::{Launcher, model::Instance, storage, version};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

mod cache;
mod declarations;
pub mod details;
pub mod mods;
pub(crate) use cache::SummaryCache;

#[derive(Clone, Debug, Serialize)]
pub struct LocalContentFile {
    pub file_name: String,
    pub name: String,
    pub version: Option<String>,
    pub enabled: bool,
    pub size: u64,
    pub mod_ids: Vec<String>,
    /// None until the existing archive-summary batch has inspected this file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<ContentDeclaration>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentDeclaration {
    pub key: String,
    pub source: String,
    pub project: Option<u64>,
    pub file: Option<u64>,
    pub path: Option<String>,
    pub status: &'static str,
    pub downloaded: u64,
    pub total: u64,
    pub optional: bool,
}

#[derive(Debug, Serialize)]
pub struct VersionContent {
    pub directory: PathBuf,
    pub files: Vec<LocalContentFile>,
}

#[derive(Debug, Serialize)]
pub struct VersionContentIndex {
    #[serde(flatten)]
    pub content: VersionContent,
    pub pending: Vec<String>,
    /// Matched against Mod IDs after summaries arrive; scan config directories once.
    pub config_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VersionContentCounts {
    pub mods: usize,
    pub datapacks: usize,
    pub resourcepacks: usize,
    pub shaderpacks: usize,
    pub saves: usize,
    pub unresolved: usize,
    pub libraries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Mods,
    Datapacks,
    Resourcepacks,
    Shaderpacks,
    Saves,
    Unresolved,
}

impl Launcher {
    /// Reorder version metadata without changing the selected version or game files.
    pub fn move_version(&self, id: &str, version: &str, before_id: Option<&str>) -> Result<()> {
        self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            let from = instance
                .versions
                .iter()
                .position(|v| v.id == version)
                .context("此实例未添加该版本")?;
            let before = before_id
                .map(|target| {
                    instance
                        .versions
                        .iter()
                        .position(|v| v.id == target)
                        .context("目标版本不存在，请刷新后重试")
                })
                .transpose()?;
            if before == Some(from) {
                return Ok(());
            }
            let entry = instance.versions.remove(from);
            let destination = before
                .map(|index| if index > from { index - 1 } else { index })
                .unwrap_or(instance.versions.len());
            instance.versions.insert(destination, entry);
            Ok(())
        })
    }

    pub fn rename_version(&self, id: &str, version_id: &str, name: &str) -> Result<()> {
        storage::safe_version(version_id)?;
        self.instance(id)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            bail!("子实例名称应为 1–80 个字符，且不能包含控制字符");
        }
        self.mutate(|db| {
            let version = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .and_then(|i| i.versions.iter_mut().find(|v| v.id == version_id))
                .context("此实例未添加该版本")?;
            version.name = (name != version_id).then(|| name.to_owned());
            Ok(())
        })
    }

    /// Move the version folder to the operating system's recycle bin.
    /// Shared libraries/assets/mods/saves are not part of this operation.
    pub fn delete_version(&self, id: &str, version_id: &str) -> Result<Instance> {
        self.delete_version_with_recycle(id, version_id, crate::recycle::move_to_trash)
    }
    pub(crate) fn delete_version_with_recycle(
        &self,
        id: &str,
        version_id: &str,
        recycle: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<Instance> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后删除版本")?;
        self.idle(id)?;
        storage::safe_version(version_id)?;
        let instance = self.instance(id)?;
        if !instance.versions.iter().any(|v| v.id == version_id) {
            bail!("此实例未添加该版本");
        }
        let game = self.available_game_dir(id)?;
        let versions = game.join("versions");
        if versions.is_dir() {
            for entry in fs::read_dir(&versions)? {
                let entry = entry?;
                let other = entry.file_name().to_string_lossy().into_owned();
                if other == version_id || storage::safe_version(&other).is_err() {
                    continue;
                }
                let metadata = version::metadata_path(&game, &other)?;
                if !metadata.is_file() {
                    continue;
                }
                let value: Value = storage::read_json(&metadata)
                    .with_context(|| format!("无法检查版本依赖：{other}"))?;
                if ["inheritsFrom", "jar"]
                    .iter()
                    .any(|key| value[*key].as_str() == Some(version_id))
                {
                    bail!("版本 {other} 仍依赖此版本，请先删除依赖它的子实例");
                }
            }
        }
        let source = versions.join(version_id);
        let result = self.mutate_recycling(
            &source,
            |db| {
                let instance = db
                    .instances
                    .iter_mut()
                    .find(|i| i.id == id)
                    .context("实例不存在")?;
                let index = instance
                    .versions
                    .iter()
                    .position(|v| v.id == version_id)
                    .context("此实例未添加该版本")?;
                instance.versions.remove(index);
                if instance.selected_version.as_deref() == Some(version_id) {
                    instance.selected_version = instance
                        .versions
                        .get(index.min(instance.versions.len().saturating_sub(1)))
                        .map(|v| v.id.clone());
                }
                Ok(instance.clone())
            },
            |source| crate::download_tasks::recycle_without_staging(self.root(), source, recycle),
        )?;
        if let Err(error) = crate::download_tasks::remove_owner(self.root(), id, Some(version_id)) {
            let _ = crate::diagnostics::record_error(self.root(), "download_cleanup", &error);
        }
        if let Err(error) = crate::resources::remove_version_tasks(&game, id, Some(version_id)) {
            let _ = crate::diagnostics::record_error(self.root(), "download_cleanup", &error);
        }
        Ok(result)
    }

    pub fn version_content(
        &self,
        id: &str,
        version_id: &str,
        kind: ContentKind,
    ) -> Result<VersionContent> {
        let (working, resolved) = self.content_location(id, version_id)?;
        let mut content = read_content(&working, resolved.as_ref(), kind, true)?;
        self.merge_declared_content(id, version_id, &working, kind, &mut content)?;
        details::mark_available(&working, &mut content, kind);
        sort_content(&mut content.files);
        Ok(content)
    }

    /// Return directory entries immediately, using cached identities where still valid.
    /// Opening unknown JARs happens in small, cancellable client-requested batches.
    pub fn version_content_index(&self, id: &str, version_id: &str) -> Result<VersionContentIndex> {
        let (working, resolved) = self.content_location(id, version_id)?;
        let mut content = read_content(&working, resolved.as_ref(), ContentKind::Mods, false)?;
        let mut pending = Vec::new();
        for file in &mut content.files {
            if let Some(identity) = self
                .0
                .mod_summaries
                .get(&content.directory.join(&file.file_name))
            {
                apply_identity(file, identity);
            } else {
                pending.push(file.file_name.clone());
            }
        }
        self.merge_declared_content(id, version_id, &working, ContentKind::Mods, &mut content)?;
        details::mark_available(&working, &mut content, ContentKind::Mods);
        sort_content(&mut content.files);
        // Read visible rows first, rather than the filesystem's arbitrary enumeration order.
        let pending: std::collections::HashSet<_> = pending.into_iter().collect();
        let pending = content
            .files
            .iter()
            .filter(|f| {
                pending.contains(&f.file_name)
                    && f.declaration.as_ref().is_none_or(|d| d.status == "local")
            })
            .map(|f| f.file_name.clone())
            .collect();
        let config_ids = mods::config_ids(&working);
        Ok(VersionContentIndex {
            content,
            pending,
            config_ids,
        })
    }

    pub fn version_content_summaries(
        &self,
        id: &str,
        version_id: &str,
        files: &[String],
    ) -> Result<Vec<LocalContentFile>> {
        if files.len() > 24 {
            bail!("单次读取的 Mod 数量过多");
        }
        let (working, resolved) = self.content_location(id, version_id)?;
        // Re-check membership on each batch: renamed/deleted files and a switched
        // working directory must not reuse arbitrary paths supplied by the UI.
        let mut content = read_content(&working, resolved.as_ref(), ContentKind::Mods, false)?;
        self.merge_declared_content(id, version_id, &working, ContentKind::Mods, &mut content)?;
        let mut listed: std::collections::HashMap<_, _> = content
            .files
            .into_iter()
            .map(|f| (f.file_name.clone(), f))
            .collect();
        let mut result = Vec::new();
        for name in files {
            storage::safe_relative(name)?;
            let Some(mut file) = listed.remove(name) else {
                continue;
            };
            if file
                .declaration
                .as_ref()
                .is_some_and(|d| d.status != "local")
            {
                result.push(file);
                continue;
            }
            apply_identity(
                &mut file,
                self.0.mod_summaries.read(&content.directory.join(name)),
            );
            result.push(file);
        }
        let mut content = VersionContent {
            directory: content.directory,
            files: result,
        };
        details::mark_available(&working, &mut content, ContentKind::Mods);
        Ok(content.files)
    }

    /// Enumerate all tabs eagerly without opening each mod archive or sorting its metadata.
    pub fn version_content_counts(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<VersionContentCounts> {
        let (working, resolved) = self.content_location(id, version_id)?;
        let game = self.available_game_dir(id)?;
        let declarations = crate::pack_content::load(&game, version_id, &working)?;
        let count = |kind| {
            let mut content = read_content(&working, resolved.as_ref(), kind, false)?;
            declarations::merge(
                &game.join("versions").join(version_id),
                kind,
                &declarations,
                &mut content,
            );
            Ok::<_, anyhow::Error>(content.files.len())
        };
        Ok(VersionContentCounts {
            mods: count(ContentKind::Mods)?,
            datapacks: count(ContentKind::Datapacks)?,
            resourcepacks: count(ContentKind::Resourcepacks)?,
            shaderpacks: count(ContentKind::Shaderpacks)?,
            saves: count(ContentKind::Saves)?,
            unresolved: count(ContentKind::Unresolved)?,
            libraries: resolved
                .as_ref()
                .map_or(0, |resolved| resolved.version.libraries.len()),
        })
    }

    fn merge_declared_content(
        &self,
        id: &str,
        version: &str,
        working: &Path,
        kind: ContentKind,
        content: &mut VersionContent,
    ) -> Result<()> {
        let game = self.available_game_dir(id)?;
        let declarations = crate::pack_content::load(&game, version, working)?;
        declarations::merge(
            &game.join("versions").join(version),
            kind,
            &declarations,
            content,
        );
        Ok(())
    }

    pub fn version_content_directory(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
    ) -> Result<PathBuf> {
        let (working, _) = self.content_location(id, version)?;
        let path = content_directory(&working, kind);
        if !path.is_dir() {
            bail!("目录尚不存在");
        }
        Ok(path)
    }

    pub(crate) fn content_location(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<(PathBuf, Option<version::ResolvedVersion>)> {
        storage::safe_version(version_id)?;
        let instance = self.instance(id)?;
        instance
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .context("此实例未添加该版本")?;
        let game = self.available_game_dir(id)?;
        let resolved = if version::metadata_path(&game, version_id)?.is_file() {
            Some(version::resolve(self.root(), &game, version_id)?)
        } else {
            None
        };
        let working = version::instance_working_directory(&game, &instance, version_id)?;
        Ok((working, resolved))
    }
}

fn content_directory(working: &Path, kind: ContentKind) -> PathBuf {
    match kind {
        ContentKind::Mods => working.join("mods"),
        ContentKind::Resourcepacks => working.join("resourcepacks"),
        ContentKind::Shaderpacks => working.join("shaderpacks"),
        ContentKind::Saves => working.join("saves"),
        // Data packs may be global or installed separately in each world.
        ContentKind::Datapacks | ContentKind::Unresolved => working.to_owned(),
    }
}
fn read_content(
    working: &Path,
    resolved: Option<&version::ResolvedVersion>,
    kind: ContentKind,
    read_metadata: bool,
) -> Result<VersionContent> {
    let directory = content_directory(working, kind);
    let mut files = Vec::new();
    match kind {
        ContentKind::Unresolved => {}
        ContentKind::Mods => {
            scan_mods(&directory, &directory, &mut files, read_metadata)?;
            // Legacy Forge also loads mods/<Minecraft version>.
            if let Some(resolved) = resolved
                && resolved.version.main_class == "net.minecraft.launchwrapper.Launch"
                && storage::safe_version(&resolved.minecraft_version).is_ok()
            {
                scan_mods(
                    &directory.join(&resolved.minecraft_version),
                    &directory,
                    &mut files,
                    read_metadata,
                )?;
            }
        }
        ContentKind::Resourcepacks | ContentKind::Shaderpacks | ContentKind::Saves => {
            scan_content(&directory, &directory, kind, &mut files)?;
        }
        ContentKind::Datapacks => {
            scan_content(&working.join("datapacks"), working, kind, &mut files)?;
            for world in content_entries(&working.join("saves"))? {
                if is_world(&world) {
                    scan_content(&world.join("datapacks"), working, kind, &mut files)?;
                }
            }
        }
    }
    if read_metadata {
        sort_content(&mut files);
    }
    Ok(VersionContent { directory, files })
}

fn sort_content(files: &mut [LocalContentFile]) {
    files.sort_by_cached_key(|item| (item.name.to_lowercase(), item.file_name.to_lowercase()));
}
fn apply_identity(file: &mut LocalContentFile, identity: ModIdentity) {
    if let Some(name) = identity.name {
        file.name = name;
    }
    file.version = identity.version;
    file.mod_ids = identity.ids;
    file.has_metadata = Some(identity.has_metadata);
}

fn content_entries(directory: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(directory) {
        Ok(entries) => entries
            .map(|entry| Ok(entry?.path()))
            .collect::<Result<Vec<_>>>()
            .context("无法读取游戏内容目录"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).context("无法读取游戏内容目录"),
    }
}

fn is_world(path: &Path) -> bool {
    path.is_dir() && (path.join("level.dat").is_file() || path.join("level.dat_old").is_file())
}

/// Only inspect immediate entries and world metadata markers. Never walk region files
/// to compute folder sizes, or unpack archives just to display a list.
fn scan_content(
    directory: &Path,
    root: &Path,
    kind: ContentKind,
    files: &mut Vec<LocalContentFile>,
) -> Result<()> {
    for path in content_entries(directory)? {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let included = match kind {
            ContentKind::Saves => is_world(&path),
            ContentKind::Shaderpacks => {
                (path.is_dir() && path.join("shaders").is_dir())
                    || (path.is_file() && name.to_ascii_lowercase().ends_with(".zip"))
            }
            _ => {
                (path.is_dir() && path.join("pack.mcmeta").is_file())
                    || (path.is_file() && name.to_ascii_lowercase().ends_with(".zip"))
            }
        };
        if !included {
            continue;
        }
        let metadata = fs::metadata(&path)?;
        files.push(LocalContentFile {
            declaration: None,
            file_name: relative_name(&path, root)?,
            name,
            mod_ids: Vec::new(),
            has_metadata: None,
            version: None,
            enabled: true,
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }
    Ok(())
}

fn scan_mods(
    directory: &Path,
    root: &Path,
    files: &mut Vec<LocalContentFile>,
    read_metadata: bool,
) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("无法读取 Mod 目录"),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let enabled = lower.ends_with(".jar");
        if !enabled && !lower.ends_with(".jar.disabled") && !lower.ends_with(".jar.off") {
            continue;
        }
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let identity = if read_metadata {
            mod_metadata(&path).unwrap_or_default()
        } else {
            ModIdentity::default()
        };
        files.push(LocalContentFile {
            declaration: None,
            file_name: relative_name(&path, root)?,
            name: identity.name.unwrap_or(name),
            version: identity.version,
            mod_ids: identity.ids,
            has_metadata: read_metadata.then_some(identity.has_metadata),
            enabled,
            size: metadata.len(),
        });
    }
    Ok(())
}

fn relative_name(path: &Path, root: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)?
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

#[derive(Clone, Default)]
pub(crate) struct ModIdentity {
    name: Option<String>,
    version: Option<String>,
    pub(crate) ids: Vec<String>,
    has_metadata: bool,
}

pub(crate) fn mod_metadata(path: &Path) -> Option<ModIdentity> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    let mut result = ModIdentity::default();
    for descriptor in details::MOD_DESCRIPTORS {
        let Ok(mut file) = archive.by_name(descriptor) else {
            continue;
        };
        result.has_metadata = true;
        if file.size() > 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::new();
        if file
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 1024 * 1024
        {
            continue;
        }
        let value: Option<Value> = if descriptor.ends_with(".toml") {
            std::str::from_utf8(&bytes)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(s).ok())
                .and_then(|v| serde_json::to_value(v).ok())
        } else {
            serde_json::from_slice(&bytes).ok()
        };
        let Some(value) = value else { continue };
        let infos: Vec<&Value> = if descriptor == "quilt.mod.json" {
            vec![&value["quilt_loader"]]
        } else if descriptor.ends_with(".toml") {
            value["mods"]
                .as_array()
                .map(|a| a.iter().collect())
                .unwrap_or_default()
        } else if descriptor == "mcmod.info" {
            value
                .as_array()
                .or_else(|| value["modList"].as_array())
                .map(|a| a.iter().collect())
                .unwrap_or_default()
        } else {
            vec![&value]
        };
        for info in infos {
            let id = info["id"]
                .as_str()
                .or_else(|| info["modid"].as_str())
                .or_else(|| info["modId"].as_str());
            for id in id.into_iter().chain(
                info["provides"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().or_else(|| v["id"].as_str())),
            ) {
                if !id.is_empty() && !result.ids.iter().any(|v| v == id) {
                    result.ids.push(id.to_owned());
                }
            }
            // Keep the first descriptor's display identity while indexing every
            // mod and declared alias in this JAR, using the same bounded reads.
            if result.name.is_none() {
                result.name = [
                    info.get("name"),
                    info.get("displayName"),
                    info["metadata"].get("name"),
                    info.get("id"),
                    info.get("modid"),
                    info.get("modId"),
                ]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|s| !s.trim().is_empty())
                .map(str::to_owned);
                result.version = info["version"]
                    .as_str()
                    .filter(|s| !s.is_empty() && !s.contains("${"))
                    .map(str::to_owned);
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tokio_util::sync::CancellationToken;

    fn profile(game: &Path, id: &str) {
        storage::write_json(
            &version::metadata_path(game, id).unwrap(),
            &crate::test_http::version_fixture(id),
        )
        .unwrap();
        fs::write(
            game.join("versions").join(id).join(format!("{id}.jar")),
            b"client",
        )
        .unwrap();
    }
    fn jar(path: &Path, entry: &str, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        zip.start_file(
            entry,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn installed_curseforge_manifest_matches_local_mods_and_resourcepacks_without_api() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Games", "RLCraft").unwrap();
        let game = instance.game_dir(launcher.root());
        profile(&game, "RLCraft");
        let directory = game.join("versions/RLCraft");
        fs::create_dir_all(directory.join("mods")).unwrap();
        fs::create_dir_all(directory.join("resourcepacks")).unwrap();
        let mut manifest = json!({"manifestType":"minecraftModpack","files":[]});
        for id in 0..187 {
            let (folder, filename) = if id < 177 {
                ("mods", format!("mod-{id}.jar"))
            } else {
                ("resourcepacks", format!("resources-{id}.zip"))
            };
            fs::write(directory.join(folder).join(&filename), b"existing").unwrap();
            manifest["files"].as_array_mut().unwrap().push(json!({
                "projectID":1000+id,"fileID":2000+id,"fileName":filename
            }));
        }
        storage::write_json(&directory.join("manifest.json"), &manifest).unwrap();
        // HMCL's record also contains the manifest, but without enriched filenames.
        let mut hmcl = manifest.clone();
        for file in hmcl["files"].as_array_mut().unwrap() {
            file.as_object_mut().unwrap().remove("fileName");
        }
        storage::write_json(&directory.join("modpack.cfg"), &json!({"manifest":hmcl})).unwrap();
        fs::write(directory.join("mods/user-added.jar"), b"custom").unwrap();
        fs::rename(
            directory.join("mods/mod-0.jar"),
            directory.join("mods/mod-0.jar.disabled"),
        )
        .unwrap();
        let counts = launcher
            .version_content_counts(&instance.id, "RLCraft")
            .unwrap();
        assert_eq!(
            (counts.mods, counts.resourcepacks, counts.unresolved),
            (178, 10, 0)
        );
        let mods = launcher
            .version_content_index(&instance.id, "RLCraft")
            .unwrap();
        assert_eq!(
            mods.content
                .files
                .iter()
                .filter(|f| f.declaration.as_ref().is_some_and(|d| d.status == "local"))
                .count(),
            177
        );
        let metadata = launcher.version_metadata(&instance.id, "RLCraft").unwrap();
        for name in ["manifest.json", "modpack.cfg"] {
            let document = metadata.documents.iter().find(|d| d.name == name).unwrap();
            assert_eq!(
                document
                    .sections
                    .iter()
                    .map(|s| (s.target, s.entries.len()))
                    .collect::<Vec<_>>(),
                [(Some("mods"), 177), (Some("resourcepacks"), 10)]
            );
        }
        // Viewing content must not rewrite the user's original manifest.
        assert_eq!(
            storage::read_json::<Value>(&directory.join("manifest.json")).unwrap(),
            manifest
        );
    }
    #[test]
    fn progressive_mod_index_reuses_summaries_and_invalidates_replaced_files() {
        use std::sync::atomic::Ordering;
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "pack");
        let first = game.join("mods/first.jar");
        jar(
            &first,
            "fabric.mod.json",
            r#"{"id":"old","name":"Old mod","version":"1"}"#,
        );
        jar(
            &game.join("mods/second.jar.disabled"),
            "mcmod.info",
            r#"[{"modid":"legacy","name":"Legacy"}]"#,
        );
        let cold = launcher
            .version_content_index(&instance.id, "pack")
            .unwrap();
        assert_eq!(cold.pending.len(), 2);
        assert_eq!(cold.content.files[0].name, "first.jar");
        assert_eq!(launcher.0.mod_summaries.reads.load(Ordering::Relaxed), 0);
        let batch = launcher
            .version_content_summaries(&instance.id, "pack", &cold.pending)
            .unwrap();
        assert_eq!(batch[0].name, "Old mod");
        assert!(!batch[1].enabled);
        assert_eq!(launcher.0.mod_summaries.reads.load(Ordering::Relaxed), 2);
        let warm = launcher
            .version_content_index(&instance.id, "pack")
            .unwrap();
        assert!(warm.pending.is_empty());
        assert_eq!(warm.content.files[0].name, "Legacy");
        assert_eq!(launcher.0.mod_summaries.reads.load(Ordering::Relaxed), 2);
        let previous = fs::metadata(&first).unwrap();
        let replacement = game.join("replacement.jar");
        jar(
            &replacement,
            "fabric.mod.json",
            r#"{"id":"new","name":"New mod","version":"2"}"#,
        );
        fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(previous.modified().unwrap()))
            .unwrap();
        fs::remove_file(&first).unwrap();
        fs::rename(replacement, &first).unwrap();
        assert_eq!(fs::metadata(&first).unwrap().len(), previous.len());
        let changed = launcher
            .version_content_index(&instance.id, "pack")
            .unwrap();
        assert_eq!(changed.pending, vec!["first.jar"]);
        let batch = launcher
            .version_content_summaries(&instance.id, "pack", &changed.pending)
            .unwrap();
        assert_eq!(batch[0].name, "New mod");
        assert_eq!(batch[0].mod_ids, vec!["new"]);
        assert_eq!(launcher.0.mod_summaries.reads.load(Ordering::Relaxed), 3);
        fs::remove_file(first).unwrap();
        let removed = launcher
            .version_content_index(&instance.id, "pack")
            .unwrap();
        assert_eq!(removed.content.files.len(), 1);
        assert!(removed.pending.is_empty());
        assert!(
            launcher
                .version_content_summaries(&instance.id, "pack", &["../outside.jar".into()])
                .is_err()
        );
        assert!(
            launcher
                .version_content_summaries(
                    &instance.id,
                    "pack",
                    &vec!["second.jar.disabled".into(); 25]
                )
                .is_err()
        );
    }
    #[test]
    fn version_order_persists_without_changing_selection_files_or_other_instances() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Game", "a").unwrap();
        let other = launcher.create_instance("Other", "other").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        for id in ["a", "b", "c"] {
            profile(&game, id);
            launcher.add_version(&instance.id, id).unwrap();
        }
        launcher.select_version(&instance.id, "b").unwrap();
        let originals: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(|id| {
                let path = version::metadata_path(&game, id).unwrap();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let guard = launcher.0.operation.try_lock().unwrap();
        launcher.move_version(&instance.id, "c", Some("a")).unwrap();
        launcher.move_version(&instance.id, "a", None).unwrap();
        launcher.move_version(&instance.id, "a", Some("a")).unwrap();
        for (source, target) in [("missing", Some("a")), ("a", Some("missing"))] {
            assert!(launcher.move_version(&instance.id, source, target).is_err());
        }
        assert!(launcher.move_version("missing", "a", None).is_err());
        drop(guard);
        launcher.add_version(&instance.id, "new").unwrap();
        launcher.select_version(&instance.id, "b").unwrap();
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let restored = launcher.instance(&instance.id).unwrap();
        assert_eq!(
            restored
                .versions
                .iter()
                .map(|v| v.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b", "a", "new"]
        );
        assert_eq!(restored.selected_version.as_deref(), Some("b"));
        assert_eq!(
            launcher
                .instance(&other.id)
                .unwrap()
                .selected_version
                .as_deref(),
            Some("other")
        );
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn rename_persists_across_discovery_without_changing_profile_or_directory() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "pack");
        let metadata = version::metadata_path(&game, "pack").unwrap();
        let original = fs::read(&metadata).unwrap();
        let guard = launcher.0.operation.try_lock().unwrap();
        launcher
            .rename_version(&instance.id, "pack", "猫咪 世界")
            .unwrap();
        drop(guard);
        for invalid in ["", "\n", "bad\0name"] {
            assert!(
                launcher
                    .rename_version(&instance.id, "pack", invalid)
                    .is_err()
            );
        }
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let restored = launcher.instance(&instance.id).unwrap();
        assert_eq!(
            restored.selected().unwrap().name.as_deref(),
            Some("猫咪 世界")
        );
        assert_eq!(restored.selected_version.as_deref(), Some("pack"));
        assert_eq!(fs::read(metadata).unwrap(), original);
        assert!(game.join("versions/pack/pack.jar").is_file());
    }

    #[test]
    fn deleting_keeps_shared_data_and_recoverable_version_files_and_respects_dependents() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Game", "base").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "base");
        storage::write_json(
            &version::metadata_path(&game, "child").unwrap(),
            &json!({"id":"child", "inheritsFrom":"base"}),
        )
        .unwrap();
        launcher.add_version(&instance.id, "child").unwrap();
        fs::create_dir_all(game.join("versions/child/saves")).unwrap();
        fs::write(
            game.join("versions/child/saves/level.dat"),
            b"isolated save",
        )
        .unwrap();
        for dir in ["mods", "saves", "libraries", "assets"] {
            fs::create_dir_all(game.join(dir)).unwrap();
            fs::write(game.join(dir).join("keep"), b"shared").unwrap();
        }
        assert!(
            launcher
                .delete_version(&instance.id, "base")
                .unwrap_err()
                .to_string()
                .contains("child")
        );
        assert!(game.join("versions/base/base.json").is_file());
        let recovered = temp.path().join("recycled-child");
        let result = launcher
            .delete_version_with_recycle(&instance.id, "child", |source| {
                assert_eq!(source, game.join("versions/child"));
                fs::rename(source, &recovered)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.selected_version.as_deref(), Some("base"));
        assert!(!game.join("versions/child").exists());
        assert_eq!(
            fs::read(recovered.join("saves/level.dat")).unwrap(),
            b"isolated save"
        );
        assert!(!game.join(".nekocrate-trash").exists());
        for dir in ["mods", "saves", "libraries", "assets"] {
            assert_eq!(fs::read(game.join(dir).join("keep")).unwrap(), b"shared");
        }
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(launcher.instance(&instance.id).unwrap().versions.len(), 1);
        launcher
            .delete_version_with_recycle(&instance.id, "base", crate::recycle::discard_test_fixture)
            .unwrap();
        launcher.add_version(&instance.id, "pending").unwrap();
        let result = launcher
            .delete_version_with_recycle(&instance.id, "pending", |_| {
                panic!("missing files must not reach system trash")
            })
            .unwrap();
        assert!(result.versions.is_empty());
        assert!(result.selected_version.is_none());
    }

    #[test]
    fn deletion_rejects_active_games_tasks_and_path_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "pack");
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(instance.id.clone(), CancellationToken::new());
        assert!(launcher.delete_version(&instance.id, "pack").is_err());
        launcher.0.games.lock().unwrap().clear();
        let guard = launcher.0.operation.try_lock().unwrap();
        assert!(launcher.delete_version(&instance.id, "pack").is_err());
        drop(guard);
        assert!(launcher.delete_version(&instance.id, "../pack").is_err());
        assert!(game.join("versions/pack/pack.json").is_file());
    }

    #[tokio::test]
    async fn linked_version_deletion_checks_database_write_before_recycling() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("external/.minecraft");
        profile(&game, "pack");
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(game.clone(), "External".into(), String::new())
            .await
            .unwrap();
        let backup = launcher.root().join("database.json.bak");
        if backup.is_file() {
            fs::remove_file(&backup).unwrap();
        }
        fs::create_dir(&backup).unwrap(); // Fail database persistence before touching files.
        assert!(
            launcher
                .delete_version_with_recycle(&instance.id, "pack", |_| panic!(
                    "database write failed; files must stay untouched"
                ))
                .is_err()
        );
        assert!(game.join("versions/pack/pack.jar").is_file());
        assert_eq!(launcher.instance(&instance.id).unwrap().versions.len(), 1);
        fs::remove_dir(backup).unwrap();
        let recovered = temp.path().join("recycled-pack");
        launcher
            .delete_version_with_recycle(&instance.id, "pack", |source| {
                fs::rename(source, &recovered)?;
                Ok(())
            })
            .unwrap();
        assert!(recovered.join("pack.jar").is_file());
        assert!(!game.join(".nekocrate-trash").exists());
        assert!(!launcher.root().join(".nekocrate-trash").exists());
        assert!(game.is_dir());
    }

    #[test]
    fn legacy_mods_use_hmcl_external_working_directory_and_game_version_subfolder() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("data"))).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let mut value = serde_json::to_value(crate::test_http::version_fixture("pack")).unwrap();
        value["mainClass"] = json!("net.minecraft.launchwrapper.Launch");
        value["libraries"] = json!([{"name":"net.minecraftforge:forge:1.12.2-14.23.5.2860"}]);
        storage::write_json(&version::metadata_path(&game, "pack").unwrap(), &value).unwrap();
        let external = temp.path().join("custom-world");
        jar(
            &external.join("mods/1.12.2/version-specific.jar"),
            "mcmod.info",
            r#"[{"name":"Version specific","version":"1"}]"#,
        );
        jar(
            &external.join("mods/1.7.10/wrong.jar"),
            "mcmod.info",
            r#"[{"name":"Wrong"}]"#,
        );
        storage::write_json(
            &game.join("versions/pack/hmclversion.cfg"),
            &json!({"usesGlobal":false,"gameDirType":2,"gameDir":external}),
        )
        .unwrap();
        let result = launcher
            .version_content(&instance.id, "pack", ContentKind::Mods)
            .unwrap();
        assert_eq!(result.directory, external.join("mods"));
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "Version specific");
    }

    #[test]
    fn content_tabs_list_packs_and_worlds_from_the_actual_directory_without_walking_world_data() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "pack");
        let external = temp.path().join("external-world");
        fs::create_dir_all(&external).unwrap();
        storage::write_json(
            &game.join("versions/pack/hmclversion.cfg"),
            &json!({"usesGlobal":false,"gameDirType":2,"gameDir":external}),
        )
        .unwrap();
        for relative in [
            "resourcepacks/Folder pack/pack.mcmeta",
            "resourcepacks/Zipped pack.zip",
            "resourcepacks/notes.txt",
            "resourcepacks/unrelated/readme.txt",
            "datapacks/Shared pack/pack.mcmeta",
            "datapacks/unrelated/readme.txt",
            "saves/世界一/level.dat",
            "saves/世界一/datapacks/World pack.zip",
            "saves/世界一/datapacks/Folder pack/pack.mcmeta",
            "saves/世界二/level.dat_old",
            "saves/世界二/datapacks/World pack.zip",
            "saves/世界二/region/ignore.zip",
            "saves/unrelated/readme.txt",
        ] {
            let path = external.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        jar(&game.join("resourcepacks/wrong.zip"), "pack.mcmeta", "{}");
        let packs = launcher
            .version_content(&instance.id, "pack", ContentKind::Resourcepacks)
            .unwrap();
        assert_eq!(packs.directory, external.join("resourcepacks"));
        assert_eq!(
            packs
                .files
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["Folder pack", "Zipped pack.zip"]
        );
        let worlds = launcher
            .version_content(&instance.id, "pack", ContentKind::Saves)
            .unwrap();
        assert_eq!(worlds.directory, external.join("saves"));
        assert_eq!(worlds.files.len(), 2);
        assert!(worlds.files.iter().all(|world| world.size == 0));
        let data = launcher
            .version_content(&instance.id, "pack", ContentKind::Datapacks)
            .unwrap();
        assert_eq!(data.directory, external);
        assert_eq!(data.files.len(), 4);
        let counts = launcher
            .version_content_counts(&instance.id, "pack")
            .unwrap();
        assert_eq!(counts.mods, 0);
        assert_eq!(counts.datapacks, data.files.len());
        assert_eq!(counts.resourcepacks, packs.files.len());
        assert_eq!(counts.saves, worlds.files.len());
        for relative in [
            "datapacks/Shared pack",
            "saves/世界一/datapacks/World pack.zip",
            "saves/世界一/datapacks/Folder pack",
            "saves/世界二/datapacks/World pack.zip",
        ] {
            assert!(
                data.files
                    .iter()
                    .any(|file| Path::new(&file.file_name) == Path::new(relative))
            );
        }
        assert_eq!(
            fs::read(external.join("saves/世界一/level.dat")).unwrap(),
            b"fixture"
        );
        // A pending version can be inspected before any content folders exist.
        launcher.add_version(&instance.id, "pending").unwrap();
        for kind in [
            ContentKind::Mods,
            ContentKind::Saves,
            ContentKind::Datapacks,
        ] {
            assert!(
                launcher
                    .version_content(&instance.id, "pending", kind)
                    .unwrap()
                    .files
                    .is_empty()
            );
        }
        assert!(
            launcher
                .version_content(&instance.id, "../pack", ContentKind::Saves)
                .is_err()
        );
        assert!(
            launcher
                .version_content(&instance.id, "unknown", ContentKind::Saves)
                .is_err()
        );
    }

    #[test]
    fn mod_list_reads_loader_metadata_and_disabled_jars_from_the_actual_working_directory() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Game", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        profile(&game, "pack");
        let mods = game.join("versions/pack/mods");
        jar(
            &mods.join("fabric.jar"),
            "fabric.mod.json",
            r#"{"id":"fabric","name":"Cute Fabric","version":"1.2"}"#,
        );
        jar(
            &mods.join("forge.jar"),
            "META-INF/mods.toml",
            "[[mods]]\nmodId='forge'\ndisplayName='Forge Mod'\nversion='2.0'",
        );
        jar(
            &mods.join("neo.jar.disabled"),
            "META-INF/neoforge.mods.toml",
            "[[mods]]\nmodId='neo'\ndisplayName='Neo Mod'\nversion='3.0'",
        );
        jar(
            &mods.join("legacy.jar"),
            "mcmod.info",
            r#"[{"modid":"legacy","name":"Legacy Mod","version":"4.0"}]"#,
        );
        jar(
            &game.join("mods/not-this-version.jar"),
            "fabric.mod.json",
            r#"{"id":"wrong"}"#,
        );
        fs::write(mods.join("broken.jar"), b"not a zip").unwrap();
        fs::write(mods.join("notes.txt"), b"not a mod").unwrap();
        let result = launcher
            .version_content(&instance.id, "pack", ContentKind::Mods)
            .unwrap();
        assert_eq!(result.directory, mods);
        assert_eq!(result.files.len(), 5);
        assert_eq!(
            launcher
                .version_content_counts(&instance.id, "pack")
                .unwrap()
                .mods,
            result.files.len()
        );
        let summary =
            read_content(&game.join("versions/pack"), None, ContentKind::Mods, false).unwrap();
        assert!(
            summary
                .files
                .iter()
                .all(|file| file.name == file.file_name && file.version.is_none())
        );
        assert!(
            result
                .files
                .iter()
                .any(|m| m.name == "Cute Fabric" && m.version.as_deref() == Some("1.2"))
        );
        assert!(result.files.iter().any(|m| m.name == "Forge Mod"));
        assert!(
            result
                .files
                .iter()
                .any(|m| m.name == "Neo Mod" && !m.enabled)
        );
        assert!(result.files.iter().any(|m| m.name == "Legacy Mod"));
        assert!(result.files.iter().any(|m| m.name == "broken.jar"));
        assert!(!result.files.iter().any(|m| m.name == "wrong"));
        assert_eq!(fs::read(mods.join("broken.jar")).unwrap(), b"not a zip");
    }
}
