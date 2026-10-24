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

#[derive(Debug, Serialize)]
pub struct LocalContentFile {
    pub file_name: String,
    pub name: String,
    pub version: Option<String>,
    pub enabled: bool,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct VersionContent {
    pub directory: PathBuf,
    pub files: Vec<LocalContentFile>,
}

#[derive(Debug, Serialize)]
pub struct VersionContentCounts {
    pub mods: usize,
    pub datapacks: usize,
    pub resourcepacks: usize,
    pub saves: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Mods,
    Datapacks,
    Resourcepacks,
    Saves,
}

impl Launcher {
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

    /// Move the version folder to a recoverable location on the same volume.
    /// Shared libraries/assets/mods/saves are not part of this operation.
    pub fn delete_version(&self, id: &str, version_id: &str) -> Result<Instance> {
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
        let trash = game
            .join(".nekocrate-trash")
            .join(uuid::Uuid::new_v4().to_string())
            .join(version_id);
        let moved = match fs::symlink_metadata(&source) {
            Ok(_) => {
                fs::create_dir_all(trash.parent().unwrap())?;
                fs::rename(&source, &trash)
                    .context("无法移除版本目录，请关闭占用这些文件的程序")?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        let result = self.mutate(|db| {
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
        });
        if result.is_err() && moved {
            fs::rename(&trash, &source).with_context(|| {
                format!("保存删除结果失败，版本文件保留在：{}", trash.display())
            })?;
        }
        result
    }

    pub fn version_content(
        &self,
        id: &str,
        version_id: &str,
        kind: ContentKind,
    ) -> Result<VersionContent> {
        let (working, resolved) = self.content_location(id, version_id)?;
        read_content(&working, resolved.as_ref(), kind, true)
    }

    /// Enumerate all tabs eagerly without opening each mod archive or sorting its metadata.
    pub fn version_content_counts(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<VersionContentCounts> {
        let (working, resolved) = self.content_location(id, version_id)?;
        let count = |kind| {
            Ok::<_, anyhow::Error>(
                read_content(&working, resolved.as_ref(), kind, false)?
                    .files
                    .len(),
            )
        };
        Ok(VersionContentCounts {
            mods: count(ContentKind::Mods)?,
            datapacks: count(ContentKind::Datapacks)?,
            resourcepacks: count(ContentKind::Resourcepacks)?,
            saves: count(ContentKind::Saves)?,
        })
    }

    fn content_location(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<(PathBuf, Option<version::ResolvedVersion>)> {
        storage::safe_version(version_id)?;
        let instance = self.instance(id)?;
        if !instance.versions.iter().any(|v| v.id == version_id) {
            bail!("此实例未添加该版本");
        }
        let game = self.available_game_dir(id)?;
        let resolved = if version::metadata_path(&game, version_id)?.is_file() {
            Some(version::resolve(self.root(), &game, version_id)?)
        } else {
            None
        };
        let working = match &resolved {
            Some(resolved) => version::working_directory(&game, &resolved.version)?,
            None => game,
        };
        Ok((working, resolved))
    }
}

fn read_content(
    working: &Path,
    resolved: Option<&version::ResolvedVersion>,
    kind: ContentKind,
    read_metadata: bool,
) -> Result<VersionContent> {
    let directory = match kind {
        ContentKind::Mods => working.join("mods"),
        ContentKind::Resourcepacks => working.join("resourcepacks"),
        ContentKind::Saves => working.join("saves"),
        // Data packs may be global or installed separately in each world.
        ContentKind::Datapacks => working.to_owned(),
    };
    let mut files = Vec::new();
    match kind {
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
        ContentKind::Resourcepacks | ContentKind::Saves => {
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
        files.sort_by_cached_key(|item| (item.name.to_lowercase(), item.file_name.to_lowercase()));
    }
    Ok(VersionContent { directory, files })
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
            file_name: path.strip_prefix(root)?.to_string_lossy().into_owned(),
            name,
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
        let (title, version) = if read_metadata {
            mod_metadata(&path).unwrap_or_default()
        } else {
            (None, None)
        };
        files.push(LocalContentFile {
            file_name: path.strip_prefix(root)?.to_string_lossy().into_owned(),
            name: title.unwrap_or(name),
            version,
            enabled,
            size: metadata.len(),
        });
    }
    Ok(())
}

fn mod_metadata(path: &Path) -> Option<(Option<String>, Option<String>)> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    for descriptor in [
        "fabric.mod.json",
        "quilt.mod.json",
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
        "mcmod.info",
    ] {
        let Ok(mut file) = archive.by_name(descriptor) else {
            continue;
        };
        if file.size() > 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 1024 * 1024 {
            continue;
        }
        let value: Value = if descriptor.ends_with(".toml") {
            let value: toml::Value = toml::from_str(std::str::from_utf8(&bytes).ok()?).ok()?;
            serde_json::to_value(value).ok()?
        } else {
            serde_json::from_slice(&bytes).ok()?
        };
        let info = if descriptor == "quilt.mod.json" {
            &value["quilt_loader"]
        } else if descriptor.ends_with(".toml") {
            &value["mods"][0]
        } else if descriptor == "mcmod.info" {
            if value.is_array() {
                &value[0]
            } else {
                &value["modList"][0]
            }
        } else {
            &value
        };
        let name = [
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
        let version = info["version"]
            .as_str()
            .filter(|s| !s.is_empty() && !s.contains("${"))
            .map(str::to_owned);
        return Some((name, version));
    }
    None
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
        zip.start_file(entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
        zip.finish().unwrap();
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
        let result = launcher.delete_version(&instance.id, "child").unwrap();
        assert_eq!(result.selected_version.as_deref(), Some("base"));
        assert!(!game.join("versions/child").exists());
        assert!(
            walkdir::WalkDir::new(game.join(".nekocrate-trash"))
                .into_iter()
                .filter_map(Result::ok)
                .any(|e| e.path().ends_with("child/saves/level.dat"))
        );
        for dir in ["mods", "saves", "libraries", "assets"] {
            assert_eq!(fs::read(game.join(dir).join("keep")).unwrap(), b"shared");
        }
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(launcher.instance(&instance.id).unwrap().versions.len(), 1);
        launcher.delete_version(&instance.id, "base").unwrap();
        launcher.add_version(&instance.id, "pending").unwrap();
        let result = launcher.delete_version(&instance.id, "pending").unwrap();
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
    async fn linked_version_deletion_rolls_back_failed_metadata_writes_and_uses_local_recovery() {
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
        fs::create_dir(&backup).unwrap(); // Make the database save fail after the folder move.
        assert!(launcher.delete_version(&instance.id, "pack").is_err());
        assert!(game.join("versions/pack/pack.jar").is_file());
        assert_eq!(launcher.instance(&instance.id).unwrap().versions.len(), 1);
        fs::remove_dir(backup).unwrap();
        launcher.delete_version(&instance.id, "pack").unwrap();
        assert!(
            walkdir::WalkDir::new(game.join(".nekocrate-trash"))
                .into_iter()
                .filter_map(Result::ok)
                .any(|e| e.path().ends_with("pack/pack.jar"))
        );
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
