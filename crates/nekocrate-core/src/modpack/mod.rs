//! Local modpack manifests are data, never executable installer instructions.
use crate::{
    Launcher,
    download::{Downloader, FileSpec},
    model::*,
    storage, version,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;
mod archive;
mod content_download;
mod install_files;

mod provider;
mod record;
pub(crate) use record::describe;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackFile {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub path: Option<String>,
    pub urls: Vec<String>,
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
    pub optional: bool,
    pub project: Option<u64>,
    pub file: Option<u64>,
    pub manual_url: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub format: String,
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: Option<ModLoader>,
    pub files: Vec<PackFile>,
    pub overrides: Vec<String>,
    pub manifest: Value,
}
#[derive(Serialize)]
pub struct Preview {
    pub pack: Pack,
}
#[derive(Clone, Serialize, Deserialize)]
struct Job {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    instance: String,
    version: String,
    name: String,
    archive: PathBuf,
    pack: Pack,
    #[serde(default)]
    overrides_done: bool,
    #[serde(default)]
    files_done: bool,
    #[serde(default)]
    completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_profile: Option<Value>,
    #[serde(default)]
    overridden: Option<std::collections::HashSet<String>>,
}
impl Job {
    fn override_paths(&self) -> Result<std::collections::HashSet<String>> {
        self.overridden
            .clone()
            .map(Ok)
            .unwrap_or_else(|| archive::override_paths(&self.archive, &self.pack))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PackStatus {
    pub name: String,
    pub missing: Vec<PackFile>,
    pub completed: bool,
}
fn job_path(game: &Path, version: &str) -> Result<PathBuf> {
    storage::safe_version(version)?;
    archive::managed_destination(
        &game.join("versions").join(version),
        ".nekocrate/modpack.json",
    )
}

pub(crate) fn content_pack(game: &Path, version: &str) -> Result<Option<Pack>> {
    storage::safe_version(version)?;
    // Ordinary pending versions may not have a directory yet. Only resolve the
    // managed path when there is actually an installation record to inspect.
    if !game
        .join("versions")
        .join(version)
        .join(".nekocrate/modpack.json")
        .is_file()
    {
        return Ok(None);
    }
    Ok(Some(record::read_job(game, version)?.pack))
}

fn pack_folder_name(name: &str) -> Result<&str> {
    let name = name.trim();
    storage::safe_version(name)
        .and_then(|()| archive::relative(name).map(|_| ()))
        .with_context(|| format!("整合包名称不能用作版本目录：{name}"))?;
    if ["manifest", "modrinth.index", "runtime"].contains(&name.to_lowercase().as_str()) {
        bail!("整合包名称与保留文件冲突：{name}");
    }
    Ok(name)
}

fn check_duplicate(instance: &Instance, game: &Path, name: &str) -> Result<()> {
    let name_key = name.to_lowercase();
    let mut duplicate = instance
        .versions
        .iter()
        .any(|v| v.id.to_lowercase() == name_key);
    match std::fs::read_dir(game.join("versions")) {
        Ok(entries) => {
            for entry in entries {
                duplicate |= entry?.file_name().to_string_lossy().to_lowercase() == name_key;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if duplicate {
        bail!("版本名称重复：{name}，请修改整合包名称");
    }
    Ok(())
}

impl Pack {
    fn manifest_name(&self) -> &'static str {
        if self.format == "modrinth" {
            "modrinth.index.json"
        } else {
            "manifest.json"
        }
    }
}

// Only directories created by this preparation are removed on failure/cancellation.
struct Preparation {
    directory: PathBuf,
    committed: bool,
}
impl Drop for Preparation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
impl Launcher {
    pub fn preview_modpack(&self, path: &Path) -> Result<Preview> {
        let pack = archive::parse(path)?;
        Ok(Preview { pack })
    }
    pub async fn prepare_modpack(
        &self,
        id: &str,
        path: &Path,
        name: &str,
        optional: &[String],
    ) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let version = pack_folder_name(name)?.to_owned();
        let game = self.available_game_dir(id)?;
        check_duplicate(&self.instance(id)?, &game, &version)?;
        let size = tokio::fs::metadata(path).await?;
        if !size.is_file() || size.len() > 8 * 1024 * 1024 * 1024 {
            bail!("整合包文件超过大小限制");
        }
        let dir = game.join("versions").join(&version);
        std::fs::create_dir_all(game.join("versions"))?;
        // Atomic reservation: even an untracked folder must never be overwritten.
        std::fs::create_dir(&dir).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::anyhow!("版本名称重复：{version}，请修改整合包名称")
            } else {
                error.into()
            }
        })?;
        let mut preparation = Preparation {
            directory: dir.clone(),
            committed: false,
        };
        let state = dir.join(".nekocrate");
        std::fs::create_dir(&state)?;
        let archive = state.join("source.zip");
        // Snapshot first: a changed source file cannot change the approved install midway.
        tokio::fs::copy(path, &archive).await?;
        let source = archive.clone();
        let mut pack = tokio::task::spawn_blocking(move || archive::parse(&source)).await??;
        if version.eq_ignore_ascii_case(&pack.minecraft) {
            bail!("整合包名称不能与基础游戏版本相同：{version}");
        }
        let profile_name = format!("{}.json", version.to_lowercase());
        let reserved = [
            profile_name.as_str(),
            "manifest.json",
            "modrinth.index.json",
        ];
        let overrides = archive::override_paths(&archive, &pack)?;
        if pack
            .files
            .iter()
            .filter_map(|f| f.path.as_deref())
            .chain(overrides.iter().map(String::as_str))
            .any(|path| reserved.contains(&path.to_lowercase().as_str()))
        {
            bail!("整合包不能覆盖版本配置或原始清单");
        }
        if optional
            .iter()
            .any(|key| !pack.files.iter().any(|f| &f.key == key && f.optional))
        {
            bail!("可选文件选择已失效，请重新打开整合包");
        }
        pack.files
            .retain(|f| !f.optional || optional.contains(&f.key));
        let job = Job {
            instance: String::new(),
            version: version.clone(),
            name: version.clone(),
            archive,
            pack,
            overrides_done: false,
            files_done: false,
            completed: false,
            pending_profile: None,
            overridden: Some(overrides),
        };
        std::fs::create_dir_all(dir.join("mods"))?;
        std::fs::create_dir_all(dir.join("config"))?;
        storage::write_json(&dir.join(job.pack.manifest_name()), &job.pack.manifest)?;
        record::write_job(&game, &version, &job)?;
        record::write_progress(&game, &version, 0.0)?;
        let result = self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            instance.versions.push(InstanceVersion {
                id: version.clone(),
                name: Some(job.name.clone()),
                minecraft_version: Some(job.pack.minecraft.clone()),
                loader: job.pack.loader.clone(),
                modpack: true,
                ..Default::default()
            });
            instance.selected_version = Some(version);
            Ok(instance.clone())
        });
        preparation.committed = result.is_ok();
        if preparation.committed && path.starts_with(self.root().join("cache/modpacks")) {
            // The managed online archive has now been copied into this version.
            // Associate both copies with the installation; never delete user imports.
            let hash = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                let task = crate::download_tasks::DownloadTask::new(
                    self.root(),
                    id,
                    &game,
                    &job.version,
                    "game",
                )?;
                task.track(
                    &self.root().join("cache/objects"),
                    &[FileSpec {
                        url: String::new(),
                        path: path.to_owned(),
                        hash,
                        sha256: false,
                        size: size.len(),
                    }],
                )
                .await?;
                task.track_extra(vec![path.to_owned(), job.archive.clone()])
                    .await?;
            }
        }
        result
    }
    fn pack_job(&self, id: &str, version: &str) -> Result<Job> {
        let instance = self.instance(id)?;
        if !instance.versions.iter().any(|v| v.id == version) {
            bail!("版本不存在");
        }
        if let Some(issue) = self.modpack_migration_issue(id, version) {
            bail!("{issue}");
        }
        record::read_job(&self.available_game_dir(id)?, version)
    }

    pub(crate) fn cached_modpack_manifest(
        &self,
        id: &str,
        version: &str,
    ) -> Result<(PathBuf, Value)> {
        let job = record::read_job(&self.available_game_dir(id)?, version)
            .or_else(|_| self.legacy_pack_job(id, version).map(|(_, job)| job))?;
        Ok((
            PathBuf::from(format!(
                "{}::{}",
                job.archive.display(),
                job.pack.manifest_name()
            )),
            job.pack.manifest,
        ))
    }
    pub async fn modpack_status(&self, id: &str, version: &str) -> Result<PackStatus> {
        let job = self.pack_job(id, version)?;
        let game = self.available_game_dir(id)?.join("versions").join(version);
        let overridden = if job.overrides_done {
            job.override_paths()?
        } else {
            std::collections::HashSet::new()
        };
        let mut missing = vec![];
        for file in &job.pack.files {
            if file.urls.is_empty() && file.path.is_some() {
                let valid = match &file.path {
                    Some(path) => {
                        let destination = archive::installed_destination(&game, path)?;
                        if overridden.contains(&path.to_lowercase()) && destination.is_file() {
                            true
                        } else {
                            Downloader::verified(&destination, &file.sha1, false, file.size).await?
                        }
                    }
                    None => false,
                };
                if !valid {
                    missing.push(file.clone());
                }
            }
        }
        Ok(PackStatus {
            name: job.name,
            missing,
            completed: job.completed,
        })
    }
    pub async fn supply_modpack_files(
        &self,
        id: &str,
        version: &str,
        paths: Vec<PathBuf>,
    ) -> Result<usize> {
        let _supply = self.0.pack_import.lock().await;
        let _operation = self.0.operation.try_lock().ok();
        if _operation.is_none()
            && self.0.active_modpack.lock().unwrap().as_ref() != Some(&(id.into(), version.into()))
        {
            bail!("请等待当前任务完成。");
        }
        self.idle(id)?;
        let job = self.pack_job(id, version)?;
        if job.completed {
            bail!("整合包已完成安装");
        }
        let target = self.available_game_dir(id)?.join("versions").join(version);
        let mut matched = 0;
        for path in paths {
            for file in job
                .pack
                .files
                .iter()
                .filter(|f| f.urls.is_empty() && f.path.is_some())
            {
                if Downloader::verified(&path, &file.sha1, false, file.size).await? {
                    let out =
                        archive::installed_destination(&target, file.path.as_deref().unwrap())?;
                    tokio::fs::create_dir_all(out.parent().unwrap()).await?;
                    if path != out {
                        let temporary = archive::managed_destination(
                            &target,
                            &format!(".nekocrate/manual-imports/{}", uuid::Uuid::new_v4()),
                        )?;
                        tokio::fs::create_dir_all(temporary.parent().unwrap()).await?;
                        let staged = ManualImportFile(temporary);
                        tokio::fs::copy(&path, &staged.0).await?;
                        if !Downloader::verified(&staged.0, &file.sha1, false, file.size).await? {
                            bail!("所选文件与整合包声明的文件校验值不匹配");
                        }
                        if !file.sha512.is_empty() {
                            verify_sha512(&staged.0, &file.sha512, &CancellationToken::new())
                                .await?;
                        }
                        let out =
                            archive::installed_destination(&target, file.path.as_deref().unwrap())?;
                        tokio::fs::rename(&staged.0, out).await?;
                    }
                    matched += 1;
                    break;
                }
            }
        }
        if matched == 0 {
            bail!("所选文件与整合包声明的文件校验值不匹配");
        }
        Ok(matched)
    }
    pub(super) async fn install_modpack(
        &self,
        id: &str,
        selected: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
        repair: bool,
    ) -> Result<bool> {
        let _guard = self.0.operation.lock().await;
        *self.0.active_modpack.lock().unwrap() = Some((id.into(), selected.into()));
        let _active = ActiveModpack(&self.0.active_modpack);
        self.idle(id)?;
        let mut job = self.pack_job(id, selected)?;
        let game = self.available_game_dir(id)?;
        let target = game.join("versions").join(selected);
        let instance = self.instance(id)?;
        crate::download::check_cancel(cancel)?;
        let sink = Event::scoped(sink, id, None);
        let overridden = job.override_paths()?;
        job.overridden = Some(overridden.clone());
        record::write_job(&game, selected, &job)?;
        let settings = self.settings();
        let downloader = Downloader::new(self.root(), settings.clone())?.for_installation(
            self.root(),
            id,
            &game,
            selected,
            "game",
        )?;
        downloader
            .retain_until_installed(vec![job.archive.clone()])
            .await?;
        let downloader = if repair {
            downloader.with_full_verification()
        } else {
            downloader
        };
        sink(Event::workflow("pack_files", "解析并下载整合包文件"));
        let key = if job.pack.format == "curseforge" {
            Some(crate::curseforge::key()?.to_owned())
        } else {
            None
        };
        let downloader = if let Some(key) = &key {
            downloader.with_curseforge_key(self.root(), &settings, key)?
        } else {
            downloader
        };
        job.completed = false;
        record::write_job(&game, selected, &job)?;
        self.set_installed(id, selected, false)?;
        if repair {
            job.files_done = false;
        }
        if job.files_done {
            // Recover if files disappeared between an interruption and resuming.
            job.files_done = job.pack.files.iter().all(|file| {
                file.path.as_ref().is_some_and(|path| {
                    overridden.contains(&path.to_lowercase())
                        || archive::installed_destination(&target, path).is_ok_and(|p| p.is_file())
                })
            });
        }
        if !job.files_done {
            install_files::ContentInstall {
                launcher: self,
                id,
                selected,
                game: &game,
                target: &target,
                downloader: &downloader,
                cancel,
                sink: &sink,
            }
            .run(&mut job, key)
            .await?;
            job.files_done = true;
            record::write_job(&game, selected, &job)?;
        }
        self.save_download_progress(id, selected, 70.0)?;
        sink(Event::workflow("pack_overrides", "写入整合包配置"));
        if !job.overrides_done {
            let source = job.archive.clone();
            let pack = job.pack.clone();
            let out = target.clone();
            let token = cancel.clone();
            tokio::task::spawn_blocking(move || archive::extract(&source, &pack, &out, &token))
                .await??;
            job.overrides_done = true;
            record::write_job(&game, selected, &job)?;
        }
        self.save_download_progress(id, selected, 80.0)?;
        sink(Event::workflow("pack_game", "安装游戏与加载器"));
        let pack_sink = sink.clone();
        let loader_sink: EventSink = Arc::new(move |mut event| {
            event.step = Some("pack_game".into());
            pack_sink(event);
        });
        let profile = if let Some(profile) = job.pending_profile.as_ref().filter(|_| !repair) {
            profile.clone()
        } else {
            crate::loader::installer::Install {
                root: self.root(),
                game: &game,
                working_directory: &version::instance_working_directory(
                    &game, &instance, selected,
                )?,
                minecraft: &job.pack.minecraft,
                loader: job.pack.loader.as_ref(),
                id: selected,
                explicit_java: instance.java_path.as_deref(),
                downloader: &downloader,
                cancel,
                sink: &loader_sink,
                auto_java: settings.auto_java,
            }
            .run()
            .await?
        };
        crate::download::check_cancel(cancel)?;
        sink(Event::workflow("java", "Java 已准备好"));
        job.pending_profile = Some(profile.clone());
        record::write_job(&game, selected, &job)?;
        // Imports can run beside network work, but final publication waits for
        // their atomic copies and final hash checks to finish.
        let _manual = self.0.pack_import.lock().await;
        let status = self.modpack_status(id, selected).await?;
        if !status.missing.is_empty() {
            sink(Event {
                pack_missing: Some(status),
                step: Some("pack_files".into()),
                ..Event::new("pack_manual", "自动安装已完成，等待手动导入文件")
            });
            return Ok(false);
        }
        sink(Event::workflow("finalize", "完成整合包安装"));
        // Publish the launch profile only after all dependencies and overrides exist.
        storage::write_json(&version::metadata_path(&game, selected)?, &profile)?;
        job.completed = true;
        job.pending_profile = None;
        record::write_job(&game, selected, &job)?;
        self.set_installed(id, selected, true)?;
        downloader.finish_installation(&sink).await;
        sink(Event::new("complete", "整合包已安装"));
        Ok(true)
    }
}
struct ActiveModpack<'a>(&'a std::sync::Mutex<Option<(String, String)>>);
impl Drop for ActiveModpack<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}
struct ManualImportFile(PathBuf);
impl Drop for ManualImportFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn verify_sha512(path: &Path, expected: &str, cancel: &CancellationToken) -> Result<()> {
    use ring::digest::{Context, SHA512};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Context::new(&SHA512);
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        crate::download::check_cancel(cancel)?;
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    if hex::encode(hasher.finish()) != expected {
        bail!(
            "整合包文件 SHA-512 校验失败：{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    Ok(())
}
