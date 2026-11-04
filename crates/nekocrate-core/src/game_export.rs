//! Explicit, cancellable exports; the source tree is never changed.
use crate::{
    Launcher, archive_support, download::check_cancel, game_import, model::*, storage, version,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize)]
pub struct ExportedGame {
    pub id: String,
    pub name: String,
    pub size: u64,
}
pub struct PortablePayload {
    pub platform: String,
    pub bytes: Vec<u8>,
}
type Files = BTreeMap<PathBuf, (PathBuf, u64)>;

fn collect(
    root: &Path,
    prefix: &Path,
    files: &mut Files,
    cancel: &CancellationToken,
) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(64)
    {
        check_cancel(cancel)?;
        let entry = entry?;
        ensure!(files.len() < 99_000, "导出文件数量过多");
        let relative = entry.path().strip_prefix(root)?;
        if game_import::excluded(relative) {
            continue;
        }
        ensure!(
            !entry.file_type().is_symlink(),
            "导出目录包含符号链接：{}",
            entry.path().display()
        );
        if entry.file_type().is_file() || entry.file_type().is_dir() {
            files.insert(
                prefix.join(relative),
                (
                    entry.path().to_owned(),
                    if entry.file_type().is_file() {
                        entry.metadata()?.len()
                    } else {
                        0
                    },
                ),
            );
        }
    }
    Ok(())
}

impl Launcher {
    // A native drag reports the drop before Finder/Explorer finishes copying.
    // Keep handed-off files alive; clean stale exports on startup/next export.
    pub(crate) fn clean_game_exports(&self) {
        let Ok(entries) = fs::read_dir(self.root().join("exports")) else {
            return;
        };
        for entry in entries.flatten().take(4096) {
            if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir()
                && meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age.as_secs() > 86400)
            {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    pub fn export_game(
        &self,
        id: &str,
        selected: Option<&str>,
        portable: Option<PortablePayload>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<ExportedGame> {
        let _operation = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后导出")?;
        let _directory = self.idle(id)?;
        self.clean_game_exports();
        let instance = self.instance(id)?;
        let game = instance.game_dir(self.root()).canonicalize()?;
        sink(Event::workflow("export_check", "收集游戏文件"));
        let mut files = Files::new();
        let name = if let Some(selected) = selected {
            ensure!(
                instance.versions.iter().any(|v| v.id == selected),
                "版本不存在"
            );
            let source = game.join("versions").join(selected);
            for (path, relative, size) in
                self.version_import_files(&source, &self.root().join("exports/unused"), cancel)?
            {
                files.insert(relative, (path, size));
            }
            // Shared working files are part of this version too. Do not include
            // unrelated versions, Java runtimes or other launcher identities.
            let working = version::instance_working_directory(&game, &instance, selected)?;
            if working == game {
                for folder in [
                    "mods",
                    "config",
                    "defaultconfigs",
                    "saves",
                    "resourcepacks",
                    "shaderpacks",
                    "datapacks",
                    "scripts",
                    "kubejs",
                    "resource",
                    "resources",
                ] {
                    collect(&game.join(folder), Path::new(folder), &mut files, cancel)?;
                }
                for file in [
                    "options.txt",
                    "optionsof.txt",
                    "optionsshaders.txt",
                    "servers.dat",
                ] {
                    if game.join(file).is_file() {
                        files.insert(
                            file.into(),
                            (game.join(file), fs::metadata(game.join(file))?.len()),
                        );
                    }
                }
            } else {
                ensure!(
                    working == source,
                    "此版本使用外部游戏目录，请先关联对应游戏目录"
                );
            }
            selected.to_owned()
        } else {
            collect(&game, Path::new(""), &mut files, cancel)?;
            for entry in &instance.versions {
                let source = game.join("versions").join(&entry.id);
                if game_import::directory_version(&source).is_none() {
                    continue;
                }
                for (path, relative, size) in self.version_import_files(&source, &game, cancel)? {
                    files.entry(relative).or_insert((path, size));
                }
            }
            instance.name.clone()
        };
        let total: u64 = files.values().map(|(_, size)| size).sum();
        ensure!(
            files.len() <= 99_000 && total <= 8 * archive_support::MAX_ENTRY,
            "导出内容超过 ZIP 导入大小限制"
        );
        let name: String = name
            .chars()
            .map(|c| {
                if c.is_control() || "/\\:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let name = format!("{}.zip", name.trim_matches([' ', '.']));
        let name = if archive_support::relative(&name).is_ok() {
            name
        } else {
            "Minecraft.zip".into()
        };
        let receipt = uuid::Uuid::new_v4().to_string();
        let dir = self.root().join("exports").join(&receipt);
        fs::create_dir_all(&dir)?;
        let output = dir.join(&name);
        let result = (|| -> Result<ExportedGame> {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(1))
                .unix_permissions(0o644);
            let mut names = std::collections::HashSet::new();
            let mut buffer = vec![0; 128 * 1024];
            let mut bytes = 0_u64;
            let mut last = Instant::now();
            sink(Event::workflow("export_files", "打包游戏文件"));
            for (relative, (path, size)) in files {
                check_cancel(cancel)?;
                let name = format!(
                    ".minecraft/{}",
                    relative.to_string_lossy().replace('\\', "/")
                );
                archive_support::relative(&name)?;
                ensure!(
                    names.insert(name.trim_end_matches('/').to_lowercase()),
                    "导出文件存在大小写冲突"
                );
                ensure!(
                    size <= archive_support::MAX_ENTRY,
                    "导出文件过大：{}",
                    path.display()
                );
                if path.is_dir() {
                    zip.add_directory(name, options.unix_permissions(0o755))?;
                    continue;
                }
                zip.start_file(&name, options.large_file(size >= u32::MAX as u64))?;
                let mut input = File::open(&path)?;
                let mut written = 0;
                loop {
                    check_cancel(cancel)?;
                    let n = input.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    written += n as u64;
                    ensure!(written <= size, "导出期间文件发生变化，请重试");
                    zip.write_all(&buffer[..n])?;
                    bytes += n as u64;
                    if last.elapsed().as_millis() >= 100 {
                        let mut e = Event::new("progress", &name);
                        e.step = Some("export_files".into());
                        e.bytes = bytes;
                        e.total_bytes = total;
                        e.overall_progress = Some(bytes as f64 / total.max(1) as f64 * 95.0);
                        sink(e);
                        last = Instant::now();
                    }
                }
                ensure!(written == size, "导出期间文件发生变化，请重试");
            }
            if let Some(portable) = portable {
                sink(Event::workflow("export_launcher", "加入便携版启动器"));
                append_portable(&mut zip, portable, cancel)?;
            }
            check_cancel(cancel)?;
            zip.finish()?.sync_all()?;
            archive_support::open(&output)?;
            storage::write_json(&dir.join("export.json"), &serde_json::json!({"name":name}))?;
            Ok(ExportedGame {
                id: receipt,
                name,
                size: fs::metadata(&output)?.len(),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&dir);
        }
        result
    }
    pub fn exported_game_path(&self, id: &str) -> Result<PathBuf> {
        storage::safe_id(id)?;
        let dir = self.root().join("exports").join(id);
        let metadata: serde_json::Value = storage::read_json(&dir.join("export.json"))?;
        let name = metadata["name"].as_str().context("导出文件信息无效")?;
        let relative = archive_support::relative(name)?;
        ensure!(relative.components().count() == 1, "导出文件名无效");
        let path = dir.join(relative);
        ensure!(
            path.canonicalize()? == path && path.is_file(),
            "导出文件已不存在"
        );
        Ok(path)
    }
    pub fn discard_game_export(&self, id: &str) -> Result<()> {
        storage::safe_id(id)?;
        let dir = self.root().join("exports").join(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
    pub(crate) fn append_version_working_files(
        &self,
        source: &Path,
        target: &Instance,
        files: &mut Vec<(PathBuf, PathBuf, u64)>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        use crate::launch_options::IsolationMode;
        let Some((game, Some(id))) = game_import::directory_owner(source) else {
            return Ok(());
        };
        let source_instance = self
            .snapshot()
            .database
            .instances
            .into_iter()
            .find(|i| i.game_dir(self.root()).canonicalize().ok().as_deref() == Some(&game));
        let working = if let Some(instance) = source_instance {
            version::instance_working_directory(&game, &instance, &id)?
        } else {
            version::working_directory(&game, &version::resolve(self.root(), &game, &id)?.version)?
        };
        ensure!(
            working == game || working == source,
            "此版本使用外部游戏目录，请关联原目录"
        );
        if working == source {
            ensure!(
                target.launch.isolation != Some(IsolationMode::Shared),
                "来源版本使用独立目录，请先将目标实例的版本隔离设为独立"
            );
            return Ok(());
        }
        let prefix = if target.launch.isolation == Some(IsolationMode::Version) {
            PathBuf::from("versions").join(&id)
        } else {
            PathBuf::new()
        };
        let mut shared = Files::new();
        for folder in [
            "mods",
            "config",
            "defaultconfigs",
            "saves",
            "resourcepacks",
            "shaderpacks",
            "datapacks",
            "scripts",
            "kubejs",
            "resource",
            "resources",
        ] {
            collect(
                &game.join(folder),
                &prefix.join(folder),
                &mut shared,
                cancel,
            )?;
        }
        for name in [
            "options.txt",
            "optionsof.txt",
            "optionsshaders.txt",
            "servers.dat",
        ] {
            let path = game.join(name);
            if path.is_file() {
                shared.insert(prefix.join(name), (path.clone(), fs::metadata(path)?.len()));
            }
        }
        for (relative, (path, size)) in shared {
            ensure!(
                !files.iter().any(|(_, existing, _)| existing == &relative),
                "版本独立目录与共享游戏文件冲突，请先检查版本隔离设置"
            );
            files.push((path, relative, size));
        }
        Ok(())
    }
    pub fn copy_version(
        &self,
        source_id: &str,
        version: &str,
        target: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<Instance> {
        let source = self.instance(source_id)?;
        ensure!(source_id != target, "版本已在此实例中");
        ensure!(
            source.versions.iter().any(|v| v.id == version),
            "来源版本不存在"
        );
        let _source_guard = self.idle(source_id)?;
        self.import_game_inner(
            game_import::GameImport {
                path: source.game_dir(self.root()).join("versions").join(version),
                archive_prefix: None,
                target: Some(target.into()),
                name: String::new(),
                directory: None,
                icon: None,
            },
            true,
            cancel,
            sink,
        )
    }
}

fn append_portable(
    zip: &mut zip::ZipWriter<File>,
    payload: PortablePayload,
    cancel: &CancellationToken,
) -> Result<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(1))
        .unix_permissions(0o755);
    ensure!(payload.bytes.len() <= 128 * 1024 * 1024, "便携版文件过大");
    match payload.platform.as_str() {
        "windows" | "linux" => {
            let executable = if payload.platform == "windows" {
                ensure!(
                    payload.bytes.starts_with(b"MZ"),
                    "便携版不是 Windows 可执行文件"
                );
                "NekoCrate-Launcher.exe"
            } else {
                ensure!(
                    payload.bytes.starts_with(b"\x7fELF"),
                    "便携版不是 Linux 可执行文件"
                );
                "NekoCrate-Launcher.elf"
            };
            zip.start_file(executable, options)?;
            zip.write_all(&payload.bytes)?;
        }
        "macos" => {
            let mut archive =
                tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(payload.bytes)));
            let mut total = 0_u64;
            let mut bundle_root = None;
            for entry in archive.entries()? {
                check_cancel(cancel)?;
                let mut entry = entry?;
                let path = entry.path()?.to_string_lossy().replace('\\', "/");
                let path = path.trim_end_matches('/');
                archive_support::relative(path)?;
                let (root, relative) = path.split_once('/').unwrap_or((path, ""));
                ensure!(root.ends_with(".app"), "便携版 App 路径无效");
                if let Some(expected) = &bundle_root {
                    ensure!(root == expected, "便携版包含多个 App");
                } else {
                    bundle_root = Some(root.to_owned());
                }
                ensure!(
                    !matches!(
                        relative,
                        "Contents/Resources/nekocrate.runtime.json"
                            | "Contents/MacOS/nekocrate.runtime.json"
                    ),
                    "归档包含安装模式配置"
                );
                // Older signed packages used a Portable app name. Normalize only
                // the outer directory; signed bundle contents stay untouched.
                let path = if relative.is_empty() {
                    "NekoCrate Launcher.app".to_owned()
                } else {
                    format!("NekoCrate Launcher.app/{relative}")
                };
                let kind = entry.header().entry_type();
                ensure!(kind.is_file() || kind.is_dir(), "便携版 App 含不支持的链接");
                total += entry.size();
                ensure!(total <= 256 * 1024 * 1024, "便携版 App 过大");
                if kind.is_dir() {
                    zip.add_directory(&path, options)?;
                } else {
                    zip.start_file(
                        &path,
                        options.unix_permissions(entry.header().mode()? & 0o777),
                    )?;
                    std::io::copy(&mut entry, zip)?;
                }
            }
        }
        _ => bail!("不支持的便携版平台"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    fn fixture() -> (tempfile::TempDir, Launcher, Instance) {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("My Pack", "pack").unwrap();
        let game = instance.game_dir(launcher.root());
        let mut profile = serde_json::to_value(crate::test_http::version_fixture("pack")).unwrap();
        profile["jar"] = json!("1.20.1");
        storage::write_json(&version::metadata_path(&game, "pack").unwrap(), &profile).unwrap();
        storage::write_json(
            &version::metadata_path(&game, "1.20.1").unwrap(),
            &crate::test_http::version_fixture("1.20.1"),
        )
        .unwrap();
        fs::write(game.join("versions/1.20.1/1.20.1.jar"), b"base client").unwrap();
        (dir, launcher, instance)
    }
    fn sink() -> EventSink {
        Arc::new(|_| {})
    }
    fn put(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    #[test]
    fn version_export_includes_jar_reference_and_isolated_files_without_other_versions() {
        let (_dir, launcher, i) = fixture();
        let game = i.game_dir(launcher.root());
        put(&game, "versions/pack/mods/cat.jar", "cat");
        put(&game, "versions/pack/config/cat.toml", "enabled=true");
        put(&game, "versions/other/saves/world/level.dat", "unrelated");
        put(&game, "mods/shared.jar", "unrelated");
        let exported = launcher
            .export_game(
                &i.id,
                Some("pack"),
                Some(PortablePayload {
                    platform: "windows".into(),
                    bytes: b"MZportable".to_vec(),
                }),
                &CancellationToken::new(),
                &sink(),
            )
            .unwrap();
        let path = launcher.exported_game_path(&exported.id).unwrap();
        assert_eq!(path.file_name().unwrap(), "pack.zip");
        let mut zip = archive_support::open(&path).unwrap();
        assert!(zip.by_name(".minecraft/versions/pack/mods/cat.jar").is_ok());
        let mut bytes = Vec::new();
        zip.by_name(".minecraft/versions/1.20.1/1.20.1.jar")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"base client");
        assert!(
            zip.by_name(".minecraft/versions/1.20.1/1.20.1.json")
                .is_err()
        );
        assert!(
            !zip.file_names()
                .any(|n| n.contains("other/") || n == ".minecraft/mods/shared.jar")
        );
        assert_eq!(
            zip.by_name("NekoCrate-Launcher.exe")
                .unwrap()
                .unix_mode()
                .unwrap()
                & 0o777,
            0o755
        );
        drop(zip);
        launcher.discard_game_export(&exported.id).unwrap();
        assert!(!path.exists());
        assert!(game.join("versions/pack/mods/cat.jar").is_file());
    }
    #[test]
    fn full_export_round_trips_without_account_files_or_launcher_data() {
        let (dir, launcher, i) = fixture();
        let game = i.game_dir(launcher.root());
        put(&game, "mods/a.jar", "mod");
        put(&game, "saves/world/level.dat", "world");
        put(&game, "versions/pack/.nekocrate/state.txt", "pending");
        put(&game, "launcher_accounts.json", "token");
        put(&game, "launcher_accounts_microsoft_store.json", "token");
        let output = launcher
            .export_game(&i.id, None, None, &CancellationToken::new(), &sink())
            .unwrap();
        let path = launcher.exported_game_path(&output.id).unwrap();
        let zip = archive_support::open(&path).unwrap();
        assert!(
            !zip.file_names()
                .any(|p| p.contains("accounts") || p.contains("database.json"))
        );
        assert_eq!(
            game_import::archive_directories(zip.file_names(), "Imported")[0].prefix,
            ".minecraft/"
        );
        drop(zip);
        fs::create_dir(dir.path().join("restored")).unwrap();
        let imported = launcher
            .import_game(
                game_import::GameImport {
                    path,
                    archive_prefix: Some(".minecraft/".into()),
                    target: None,
                    name: "Restored".into(),
                    directory: Some(dir.path().join("restored")),
                    icon: None,
                },
                &CancellationToken::new(),
                &sink(),
            )
            .unwrap();
        assert_eq!(
            fs::read(
                imported
                    .game_dir(launcher.root())
                    .join("saves/world/level.dat")
            )
            .unwrap(),
            b"world"
        );
        assert!(game.join("launcher_accounts.json").is_file());
    }
    #[test]
    fn copy_version_preserves_shared_working_files_and_refuses_collisions() {
        let (_dir, launcher, i) = fixture();
        let game = i.game_dir(launcher.root());
        put(&game, "saves/world/level.dat", "world");
        put(&game, "config/a.toml", "enabled=true");
        let target = launcher.create_instance("Target", "").unwrap();
        let copied = launcher
            .copy_version(
                &i.id,
                "pack",
                &target.id,
                &CancellationToken::new(),
                &sink(),
            )
            .unwrap();
        assert_eq!(
            fs::read(
                copied
                    .game_dir(launcher.root())
                    .join("saves/world/level.dat")
            )
            .unwrap(),
            b"world"
        );
        assert_eq!(copied.versions.len(), 1);
        assert_eq!(copied.selected_version.as_deref(), Some("pack"));
        assert!(
            launcher
                .copy_version(
                    &i.id,
                    "pack",
                    &target.id,
                    &CancellationToken::new(),
                    &sink()
                )
                .unwrap_err()
                .to_string()
                .contains("同名")
        );
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"world"
        );
    }
    #[test]
    fn cancellation_and_invalid_portable_remove_partial_archives() {
        let (_dir, launcher, i) = fixture();
        let token = CancellationToken::new();
        let copy = token.clone();
        let sink: EventSink = Arc::new(move |e| {
            if e.step.as_deref() == Some("export_files") {
                copy.cancel();
            }
        });
        assert!(
            launcher
                .export_game(&i.id, None, None, &token, &sink)
                .is_err()
        );
        assert_eq!(
            fs::read_dir(launcher.root().join("exports"))
                .unwrap()
                .count(),
            0
        );
        assert!(
            launcher
                .export_game(
                    &i.id,
                    None,
                    Some(PortablePayload {
                        platform: "linux".into(),
                        bytes: b"wrong format".to_vec()
                    }),
                    &CancellationToken::new(),
                    &(Arc::new(|_| {}) as EventSink)
                )
                .is_err()
        );
        assert_eq!(
            fs::read_dir(launcher.root().join("exports"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn export_collects_inherited_configuration_and_legacy_shared_files() {
        let (_dir, launcher, i) = fixture();
        let game = i.game_dir(launcher.root());
        let path = version::metadata_path(&game, "pack").unwrap();
        let mut profile: serde_json::Value = storage::read_json(&path).unwrap();
        profile["inheritsFrom"] = json!("1.20.1");
        profile["libraries"] = json!([{"name":"example:shared:1","downloads":{"artifact":{"path":"example/shared/1/shared-1.jar","url":"https://example.invalid/shared.jar","sha1":"0000000000000000000000000000000000000000","size":6}}}]);
        storage::write_json(&path, &profile).unwrap();
        let parent = version::metadata_path(&game, "1.20.1").unwrap();
        fs::create_dir_all(launcher.root().join("metadata/versions")).unwrap();
        fs::rename(
            parent,
            launcher.root().join("metadata/versions/1.20.1.json"),
        )
        .unwrap();
        put(
            launcher.root(),
            "libraries/example/shared/1/shared-1.jar",
            "shared",
        );
        let output = launcher
            .export_game(&i.id, None, None, &CancellationToken::new(), &sink())
            .unwrap();
        let mut zip =
            archive_support::open(&launcher.exported_game_path(&output.id).unwrap()).unwrap();
        assert!(
            zip.by_name(".minecraft/versions/1.20.1/1.20.1.json")
                .is_ok()
        );
        assert!(
            zip.by_name(".minecraft/libraries/example/shared/1/shared-1.jar")
                .is_ok()
        );
        assert!(
            !game
                .join("libraries/example/shared/1/shared-1.jar")
                .exists()
        );
    }
    #[test]
    fn mac_portable_preserves_bundle_and_executable_mode() {
        let (_dir, launcher, i) = fixture();
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gzip);
        for (path, data, mode) in [
            ("NekoCrate Portable.app/Contents/Info.plist", "plist", 0o644),
            (
                "NekoCrate Portable.app/Contents/MacOS/launcher",
                "executable",
                0o755,
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_mode(mode);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, path, data.as_bytes()).unwrap();
        }
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        let exported = launcher
            .export_game(
                &i.id,
                None,
                Some(PortablePayload {
                    platform: "macos".into(),
                    bytes,
                }),
                &CancellationToken::new(),
                &sink(),
            )
            .unwrap();
        let mut zip =
            archive_support::open(&launcher.exported_game_path(&exported.id).unwrap()).unwrap();
        assert_eq!(
            zip.by_name("NekoCrate Launcher.app/Contents/MacOS/launcher")
                .unwrap()
                .unix_mode()
                .unwrap()
                & 0o777,
            0o755
        );
    }
}
