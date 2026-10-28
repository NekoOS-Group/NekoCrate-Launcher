//! A pack's source and resumable state travel with its version directory.
use super::*;
use std::fs;

pub(super) fn read_job(game: &Path, version: &str) -> Result<Job> {
    let path = job_path(game, version)?;
    let mut job: Job = storage::read_json(&path)?;
    if job.version != version || job.archive != Path::new("source.zip") {
        bail!("整合包安装目标不匹配");
    }
    job.archive = archive::managed_destination(
        &game.join("versions").join(version),
        ".nekocrate/source.zip",
    )?;
    Ok(job)
}

pub(super) fn write_job(game: &Path, version: &str, job: &Job) -> Result<()> {
    let mut saved = job.clone();
    saved.instance.clear();
    saved.version = version.into();
    saved.archive = "source.zip".into();
    storage::write_json(&job_path(game, version)?, &saved)
}

pub(super) fn write_progress(game: &Path, version: &str, progress: f64) -> Result<()> {
    let directory = game.join("versions").join(version);
    storage::write_json(
        &archive::managed_destination(&directory, ".nekocrate/progress.json")?,
        &progress,
    )
}

pub(crate) fn describe(game: &Path, version: &str) -> Result<InstanceVersion> {
    let job = read_job(game, version)?;
    let directory = game.join("versions").join(version);
    let progress = storage::read_json::<f64>(&directory.join(".nekocrate/progress.json"))
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let installed = job.completed && version::metadata_path(game, version)?.is_file();
    Ok(InstanceVersion {
        id: version.into(),
        name: Some(job.name),
        minecraft_version: Some(job.pack.minecraft),
        loader: job.pack.loader,
        modpack: true,
        installed,
        download_progress: if installed { 100.0 } else { progress.min(99.0) },
    })
}

impl Launcher {
    pub(crate) fn save_pack_progress(
        &self,
        instance: &str,
        version: &str,
        progress: f64,
    ) -> Result<()> {
        let instance = self.instance(instance)?;
        if instance
            .versions
            .iter()
            .any(|v| v.id == version && v.modpack)
        {
            write_progress(&instance.game_dir(self.root()), version, progress)?;
        }
        Ok(())
    }

    pub(crate) fn modpack_migration_issue(&self, instance: &str, version: &str) -> Option<String> {
        self.0
            .modpack_migration_issues
            .lock()
            .unwrap()
            .get(&(instance.into(), version.into()))
            .cloned()
    }

    pub(super) fn legacy_pack_job(&self, instance: &str, version: &str) -> Result<(PathBuf, Job)> {
        storage::safe_id(instance)?;
        storage::safe_version(version)?;
        let base = self.root().join("modpacks");
        let scoped = base.join(instance).join(format!("{version}.json"));
        let path = if scoped.try_exists()? {
            scoped
        } else {
            base.join(format!("{version}.json"))
        };
        let job: Job = storage::read_json(&path)?;
        if job.instance != instance || job.version != version {
            bail!("整合包安装目标不匹配");
        }
        Ok((path, job))
    }

    /// Runs with the data-directory lock, before the launcher accepts UI work.
    pub(crate) fn migrate_modpack_records(&self) {
        for instance in self.snapshot().database.instances {
            for version in &instance.versions {
                if !version.modpack {
                    continue;
                }
                let game = instance.game_dir(self.root());
                let marker = game
                    .join("versions")
                    .join(&version.id)
                    .join(".nekocrate/migration.json");
                let local_valid = read_job(&game, &version.id).is_ok();
                let mut origin = version.clone();
                if let Ok(value) = storage::read_json::<Value>(&marker)
                    && value["instance"].as_str() == Some(&instance.id)
                    && let Some(old) = value["version"].as_str()
                {
                    origin.id = old.into();
                } else if local_valid {
                    continue;
                }
                if local_valid && self.legacy_pack_job(&instance.id, &origin.id).is_err() {
                    let _ = fs::remove_file(marker);
                    continue;
                }
                let result = self.migrate_pack_record(&instance, &origin);
                if let Err(error) = result {
                    self.0
                        .modpack_migration_issues
                        .lock()
                        .unwrap()
                        .insert((instance.id.clone(), version.id.clone()), error.to_string());
                }
            }
        }
    }

    fn migrate_pack_record(&self, instance: &Instance, version: &InstanceVersion) -> Result<()> {
        let (legacy_path, job) = self.legacy_pack_job(&instance.id, &version.id)?;
        let game = instance.game_dir(self.root());
        let old = game.join("versions").join(&version.id);
        let name = if !version.installed
            && !job.completed
            && !version::metadata_path(&game, &version.id)?.is_file()
            && version
                .id
                .strip_prefix("pack-")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        {
            pack_folder_name(&job.name)?
        } else {
            &version.id
        };
        if name == job.pack.minecraft {
            bail!("整合包名称不能与基础游戏版本相同：{name}");
        }
        let target = game.join("versions").join(name);
        let marker =
            serde_json::json!({"instance":instance.id,"version":version.id,"archive":job.archive});
        // The marker moves with the folder and lets a restart finish a rename
        // interrupted between the filesystem move and database publication.
        let resumed = !old.try_exists()?
            && target.is_dir()
            && storage::read_json::<Value>(&target.join(".nekocrate/migration.json"))
                .is_ok_and(|value| value == marker);
        if name != version.id && !resumed {
            check_duplicate(instance, &game, name)?;
        }
        let directory = if resumed { &target } else { &old };
        let state = archive::managed_destination(directory, ".nekocrate")?;
        if state.try_exists()?
            && !resumed
            && !storage::read_json::<Value>(&state.join("migration.json"))
                .is_ok_and(|value| value == marker)
        {
            bail!("整合包安装目标不匹配");
        }
        fs::create_dir_all(&state)?;
        storage::write_json(&state.join("migration.json"), &marker)?;
        let cache_root = self.root().join("modpacks").canonicalize()?;
        let source = if job.archive.try_exists()? {
            job.archive.canonicalize()?
        } else {
            job.archive.clone()
        };
        if !source.starts_with(&cache_root)
            || source.file_name() != Some(std::ffi::OsStr::new("source.zip"))
            || source
                .strip_prefix(&cache_root)?
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            bail!("整合包缓存路径无效");
        }
        let saved = archive::managed_destination(directory, ".nekocrate/source.zip")?;
        if !saved.try_exists()? {
            let temporary = state.join("source.zip.migrating");
            fs::copy(&source, &temporary).context("原始整合包缓存不存在，无法迁移")?;
            fs::rename(temporary, &saved)?;
        } else if source.is_file() && !storage::same_file_contents(&source, &saved)? {
            bail!("版本目录中的整合包缓存与原记录不一致，未覆盖文件");
        }
        // Preserve legacy partial transfers too, before the game directory can
        // be moved away from the old launcher's cache.
        for file in &job.pack.files {
            let (Some(path), Some(url)) = (&file.path, file.urls.first()) else {
                continue;
            };
            let destination = archive::installed_destination(directory, path)?;
            if destination.is_file() {
                continue;
            }
            let spec = FileSpec {
                path: destination,
                url: url.clone(),
                hash: file.sha1.clone(),
                sha256: false,
                size: file.size,
            };
            let old_cache =
                crate::download::object_path(&self.root().join("cache/objects"), &spec)?;
            let cache = archive::managed_destination(directory, ".nekocrate/downloads")?;
            let local_cache = crate::download::object_path(&cache, &spec)?;
            for (from, to) in [
                (old_cache.clone(), local_cache.clone()),
                (
                    old_cache.with_extension("part"),
                    local_cache.with_extension("part"),
                ),
            ] {
                if from.is_file() && !to.try_exists()? {
                    fs::create_dir_all(to.parent().context("无效下载目标")?)?;
                    let temporary = to.with_extension("migrating");
                    fs::copy(from, &temporary)?;
                    fs::rename(temporary, to)?;
                }
            }
        }
        let manifest = archive::managed_destination(directory, job.pack.manifest_name())?;
        if !manifest.try_exists()? {
            storage::write_json(&manifest, &job.pack.manifest)?;
        }
        let mut local = job.clone();
        local.instance.clear();
        local.archive = "source.zip".into();
        local.version = name.into();
        storage::write_json(
            &archive::managed_destination(directory, ".nekocrate/modpack.json")?,
            &local,
        )?;
        storage::write_json(
            &archive::managed_destination(directory, ".nekocrate/progress.json")?,
            &version.download_progress,
        )?;
        if name != version.id && !resumed {
            check_duplicate(instance, &game, name)?;
            fs::rename(&old, &target)?;
        }
        let result = self.mutate(|db| {
            let tracked = db
                .instances
                .iter_mut()
                .find(|i| i.id == instance.id)
                .context("实例不存在")?;
            let entry = tracked
                .versions
                .iter_mut()
                .find(|v| v.id == version.id || v.id == name)
                .context("版本不存在")?;
            entry.id = name.into();
            if tracked.selected_version.as_deref() == Some(&version.id) {
                tracked.selected_version = Some(name.into());
            }
            Ok(())
        });
        if let Err(error) = result {
            if name != version.id {
                fs::rename(&target, &old).with_context(|| {
                    format!("保存迁移结果失败，版本文件保留在：{}", target.display())
                })?;
            }
            write_job(&game, &version.id, &job)?;
            return Err(error);
        }
        // Remove only the old launcher-owned source and record after publishing
        // the local copy. User mods/configs and unrelated cache entries are untouched.
        if source.try_exists()? {
            fs::remove_file(&source)?;
        }
        if let Some(parent) = source.parent() {
            let _ = fs::remove_dir(parent);
        }
        fs::remove_file(&legacy_path)?;
        let _ = fs::remove_file(legacy_path.with_extension("json.bak"));
        let _ = fs::remove_file(target.join(".nekocrate/migration.json"));
        let _ = fs::remove_file(target.join(".nekocrate/migration.json.bak"));
        Ok(())
    }
}
