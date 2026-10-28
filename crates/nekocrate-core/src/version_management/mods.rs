//! Edits stay in the selected version's actual working directory.
use super::{ContentKind, LocalContentFile};
use crate::{Launcher, storage};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const TEXT_LIMIT: usize = 512 * 1024;
#[derive(Serialize)]
pub struct ModConfigFile {
    pub path: String,
    pub size: u64,
}
#[derive(Serialize)]
pub struct ModConfigDocument {
    pub path: String,
    pub text: String,
    pub revision: String,
    pub format: String,
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes))
}
fn regular(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = storage::safe_relative(relative)?;
    let mut path = root.to_owned();
    for component in relative.components() {
        path.push(component);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("配置和 Mod 编辑不支持符号链接");
        }
    }
    if !path.is_file() {
        bail!("文件不存在或不是普通文件");
    }
    Ok(path)
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((TEXT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > TEXT_LIMIT {
        bail!("配置文件超过 512 KiB，请使用外部编辑器");
    }
    Ok(bytes)
}
fn matches_id(value: &str, id: &str) -> bool {
    let value = value.to_ascii_lowercase();
    let id = id.to_ascii_lowercase();
    !id.is_empty()
        && (value == id
            || value
                .strip_prefix(&id)
                .is_some_and(|tail| tail.starts_with(['.', '-', '_'])))
}

fn config_entries(working: &Path) -> Vec<(String, ModConfigFile)> {
    let mut roots = vec![working.join("config"), working.join("defaultconfigs")];
    if let Ok(worlds) = fs::read_dir(working.join("saves")) {
        for world in worlds.take(2048).flatten() {
            if world.file_type().is_ok_and(|kind| kind.is_dir()) {
                roots.push(world.path().join("serverconfig"));
            }
        }
    }
    let mut found = Vec::new();
    for root in roots {
        if !root.is_dir() || fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .take(8192)
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ![
                "json",
                "json5",
                "toml",
                "cfg",
                "conf",
                "properties",
                "yaml",
                "yml",
                "txt",
            ]
            .contains(&extension.as_str())
            {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&root) else {
                continue;
            };
            let first = relative
                .components()
                .next()
                .unwrap()
                .as_os_str()
                .to_string_lossy();
            let Ok(relative) = path.strip_prefix(working) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if regular(working, &relative).is_err() {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            found.push((
                first.to_ascii_lowercase(),
                ModConfigFile {
                    path: relative,
                    size: metadata.len(),
                },
            ));
        }
    }
    found
}

/// All IDs matched by the same filename/directory boundary rule used by the editor.
/// Return one compact snapshot with the index, not one directory walk per Mod.
pub(super) fn config_ids(working: &Path) -> Vec<String> {
    let mut ids = std::collections::BTreeSet::new();
    for (name, _) in config_entries(working) {
        for (offset, ch) in name.char_indices() {
            if offset > 0 && matches!(ch, '.' | '-' | '_') {
                ids.insert(name[..offset].to_owned());
            }
        }
        ids.insert(name);
    }
    ids.into_iter().collect()
}
impl Launcher {
    fn listed_mod(
        &self,
        id: &str,
        version: &str,
        file: &str,
    ) -> Result<(PathBuf, LocalContentFile)> {
        let (working, _) = self.content_location(id, version)?;
        let path = self.version_content_path(id, version, ContentKind::Mods, file, None)?;
        let relative = path
            .strip_prefix(&working)
            .context("Mod 路径不在游戏目录内")?
            .to_string_lossy()
            .replace('\\', "/");
        regular(&working, &relative)?;
        let entry = self
            .version_content_summaries(id, version, &[file.into()])?
            .into_iter()
            .next()
            .context("Mod 不存在")?;
        if entry
            .declaration
            .as_ref()
            .is_some_and(|d| d.status != "local")
        {
            bail!("Mod 尚未下载");
        }
        Ok((working, entry))
    }
    pub fn set_mod_enabled(
        &self,
        id: &str,
        version: &str,
        file: &str,
        enabled: bool,
    ) -> Result<String> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("文件任务正在运行，请完成后修改 Mod")?;
        self.resource_directory_idle(id, version)?;
        let (working, entry) = self.listed_mod(id, version, file)?;
        if entry.enabled == enabled {
            return Ok(file.into());
        }
        let target = if enabled {
            let lower = file.to_ascii_lowercase();
            let stem = lower
                .strip_suffix(".disabled")
                .or_else(|| lower.strip_suffix(".off"))
                .context("无法识别禁用文件名")?;
            file[..stem.len()].to_owned()
        } else {
            format!("{file}.disabled")
        };
        let source = regular(&working, &format!("mods/{file}"))?;
        let destination = working.join("mods").join(storage::safe_relative(&target)?);
        if fs::symlink_metadata(&destination).is_ok() {
            bail!("目标 Mod 文件已存在，未覆盖");
        }
        // Persist provenance under the renamed path, retaining update/install identities.
        let records_path = working.join(".nekocrate/resources.json");
        let records = if records_path.exists() {
            regular(&working, ".nekocrate/resources.json")?;
            if fs::symlink_metadata(records_path.with_extension("json.bak"))
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                bail!("来源备份不能是符号链接");
            }
            Some(storage::read_json::<crate::resources::Records>(
                &records_path,
            )?)
        } else {
            None
        };
        fs::rename(&source, &destination)?;
        if let Some(mut records) = records
            && let Some(record) = records.remove(&format!("mods/{file}"))
        {
            records.insert(format!("mods/{target}"), record);
            if let Err(error) = storage::write_json(&records_path, &records) {
                fs::rename(&destination, &source)
                    .context("来源信息保存失败，恢复 Mod 文件名失败")?;
                return Err(error);
            }
        }
        Ok(target)
    }
    pub fn mod_config_files(
        &self,
        id: &str,
        version: &str,
        file: &str,
    ) -> Result<Vec<ModConfigFile>> {
        let (working, entry) = self.listed_mod(id, version, file)?;
        let mut ids = entry.mod_ids;
        let filename = file.rsplit('/').next().unwrap_or(file).to_ascii_lowercase();
        let filename = filename
            .trim_end_matches(".disabled")
            .trim_end_matches(".off")
            .trim_end_matches(".jar");
        if ids.is_empty() {
            ids.push(filename.split('-').next().unwrap_or(filename).into());
        }
        let mut found: Vec<_> = config_entries(&working)
            .into_iter()
            .filter(|(name, _)| ids.iter().any(|id| matches_id(name, id)))
            .map(|(_, file)| file)
            .collect();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found.truncate(256);
        Ok(found)
    }
    fn mod_config_path(
        &self,
        id: &str,
        version: &str,
        file: &str,
        config: &str,
    ) -> Result<PathBuf> {
        if !self
            .mod_config_files(id, version, file)?
            .iter()
            .any(|entry| entry.path == config)
        {
            bail!("配置文件未关联到此 Mod");
        }
        let (working, _) = self.content_location(id, version)?;
        regular(&working, config)
    }
    pub fn read_mod_config(
        &self,
        id: &str,
        version: &str,
        file: &str,
        config: &str,
    ) -> Result<ModConfigDocument> {
        let path = self.mod_config_path(id, version, file, config)?;
        let bytes = bounded(&path)?;
        let revision = digest(&bytes);
        let text = String::from_utf8(bytes).context("配置不是 UTF-8 文本，请使用外部编辑器")?;
        if text.contains('\0') {
            bail!("配置不是文本文件");
        }
        Ok(ModConfigDocument {
            path: config.into(),
            text,
            revision,
            format: path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn save_mod_config(
        &self,
        id: &str,
        version: &str,
        file: &str,
        config: &str,
        revision: &str,
        text: &str,
    ) -> Result<ModConfigDocument> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("文件任务正在运行，请完成后保存配置")?;
        self.resource_directory_idle(id, version)?;
        if text.len() > TEXT_LIMIT || text.contains('\0') {
            bail!("配置内容无效或超过 512 KiB");
        }
        let path = self.mod_config_path(id, version, file, config)?;
        let original = bounded(&path)?;
        if digest(&original) != revision {
            bail!("配置已被其他程序修改，请重新读取后再保存");
        }
        let input = text.trim_start_matches('\u{feff}');
        match path.extension().and_then(|s| s.to_str()) {
            Some("json") => {
                serde_json::from_str::<serde_json::Value>(input).context("JSON 配置格式错误")?;
            }
            Some("toml") => {
                toml::from_str::<toml::Value>(input).context("TOML 配置格式错误")?;
            }
            _ => {}
        }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let backup = path.with_file_name(format!(
            "{}.{}.nekocrate.bak",
            path.file_name().unwrap_or_default().to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        let result = (|| -> Result<()> {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            output.set_permissions(fs::metadata(&path)?.permissions())?;
            output.write_all(text.as_bytes())?;
            output.sync_all()?;
            drop(output);
            // Recheck after validation/write; never silently overwrite a changed editor buffer.
            if bounded(&path)? != original {
                bail!("配置已被其他程序修改，请重新读取后再保存");
            }
            let mut saved = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)?;
            saved.set_permissions(fs::metadata(&path)?.permissions())?;
            saved.write_all(&original)?;
            saved.sync_all()?;
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.read_mod_config(id, version, file, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Launcher, String, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Mods", "1.20.1").unwrap();
        let game = instance.game_dir(launcher.root());
        fs::create_dir_all(game.join("mods")).unwrap();
        let mut jar = zip::ZipWriter::new(fs::File::create(game.join("mods/cat.jar")).unwrap());
        jar.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        jar.write_all(br#"{"id":"cat","version":"1","name":"Cat"}"#)
            .unwrap();
        jar.finish().unwrap();
        fs::create_dir_all(game.join("config")).unwrap();
        fs::write(
            game.join("config/cat.toml"),
            "# keep my comment\nvolume = 2\n",
        )
        .unwrap();
        fs::write(game.join("config/caterpillar.toml"), "unrelated = true").unwrap();
        (dir, launcher, instance.id, game)
    }
    #[test]
    fn toggles_without_overwriting_and_blocks_running_game() {
        let (_dir, launcher, id, game) = fixture();
        assert_eq!(
            launcher
                .set_mod_enabled(&id, "1.20.1", "cat.jar", false)
                .unwrap(),
            "cat.jar.disabled"
        );
        assert!(!game.join("mods/cat.jar").exists());
        assert!(
            !launcher
                .version_content(&id, "1.20.1", ContentKind::Mods)
                .unwrap()
                .files[0]
                .enabled
        );
        fs::write(game.join("mods/cat.jar"), b"another mod").unwrap();
        assert!(
            launcher
                .set_mod_enabled(&id, "1.20.1", "cat.jar.disabled", true)
                .is_err()
        );
        fs::remove_file(game.join("mods/cat.jar")).unwrap();
        launcher
            .set_mod_enabled(&id, "1.20.1", "cat.jar.disabled", true)
            .unwrap();
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(id.clone(), tokio_util::sync::CancellationToken::new());
        assert!(
            launcher
                .set_mod_enabled(&id, "1.20.1", "cat.jar", false)
                .is_err()
        );
        assert!(game.join("mods/cat.jar").exists());
    }
    #[test]
    fn config_matching_validation_conflicts_and_backup() {
        let (_dir, launcher, id, game) = fixture();
        let files = launcher.mod_config_files(&id, "1.20.1", "cat.jar").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "config/cat.toml");
        let doc = launcher
            .read_mod_config(&id, "1.20.1", "cat.jar", &files[0].path)
            .unwrap();
        assert!(
            launcher
                .save_mod_config(
                    &id,
                    "1.20.1",
                    "cat.jar",
                    &doc.path,
                    &doc.revision,
                    "invalid = ["
                )
                .is_err()
        );
        let text = "# keep my comment\nvolume = 3\n";
        let saved = launcher
            .save_mod_config(&id, "1.20.1", "cat.jar", &doc.path, &doc.revision, text)
            .unwrap();
        assert_eq!(saved.text, text);
        assert!(
            launcher
                .save_mod_config(&id, "1.20.1", "cat.jar", &doc.path, &doc.revision, text)
                .is_err()
        );
        let backups: Vec<_> = fs::read_dir(game.join("config"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".nekocrate.bak"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), doc.text);
        assert!(
            launcher
                .read_mod_config(&id, "1.20.1", "cat.jar", "../database.json")
                .is_err()
        );
        assert!(
            launcher
                .read_mod_config(&id, "1.20.1", "cat.jar", "config/caterpillar.toml")
                .is_err()
        );
    }
    #[test]
    fn config_index_uses_editor_boundaries_and_tracks_creation_and_removal() {
        let (_dir, launcher, id, game) = fixture();
        fs::create_dir_all(game.join("defaultconfigs/CAT/nested")).unwrap();
        fs::write(game.join("defaultconfigs/CAT/nested/options.json"), "{}").unwrap();
        fs::create_dir_all(game.join("saves/World/serverconfig")).unwrap();
        fs::write(game.join("saves/World/serverconfig/cat-server.toml"), "x=1").unwrap();
        fs::write(game.join("config/cat.png"), "not a config").unwrap();
        let snapshot = launcher.version_content_index(&id, "1.20.1").unwrap();
        assert!(snapshot.config_ids.iter().any(|id| id == "cat"));
        assert!(!snapshot.config_ids.iter().any(|id| id == "cater"));
        assert!(!snapshot.config_ids.iter().any(|id| id == "cat.png"));
        let files = launcher.mod_config_files(&id, "1.20.1", "cat.jar").unwrap();
        assert_eq!(files.len(), 3);
        for file in files {
            fs::remove_file(game.join(file.path)).unwrap();
        }
        assert!(
            launcher
                .mod_config_files(&id, "1.20.1", "cat.jar")
                .unwrap()
                .is_empty()
        );
        assert!(!config_ids(&game).iter().any(|id| id == "cat"));
        assert!(config_ids(&game).iter().any(|id| id == "caterpillar"));
    }
    #[cfg(unix)]
    #[test]
    fn config_symlinks_cannot_escape_game_directory() {
        let (dir, launcher, id, game) = fixture();
        let outside = dir.path().join("secret.toml");
        fs::write(&outside, "secret = true").unwrap();
        fs::remove_file(game.join("config/cat.toml")).unwrap();
        std::os::unix::fs::symlink(&outside, game.join("config/cat.toml")).unwrap();
        assert!(
            launcher
                .read_mod_config(&id, "1.20.1", "cat.jar", "config/cat.toml")
                .is_err()
        );
    }
}
