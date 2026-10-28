mod archive_support;
pub mod auth;
pub mod crash;
pub mod diagnostics;
pub mod download;
mod download_concurrency;
mod download_stall;
mod download_tasks;
pub mod fabric;
pub mod folder_import;
mod game_options;
mod installer_cache;
mod instance_creation;
pub mod instance_discovery;
pub mod java;
pub mod java_compatibility;
mod language;
pub mod loader;
pub mod memory;
mod migration;
pub mod minecraft;
pub mod model;
#[cfg(test)]
mod modloader_tests;
pub mod modpack;
pub mod network;
mod pack_content;
mod proxy_credentials;
mod recycle;
pub mod resource_manager;
pub mod resources;
mod runtime_compatibility;
pub mod skin;
pub mod storage;
#[cfg(test)]
mod test_http;
pub mod updates;
mod verification;
pub mod version;
pub mod version_management;
pub mod version_metadata;

use anyhow::{Context, Result, bail};
use model::*;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

struct Shared {
    root: PathBuf,
    _lock: fs::File,
    db: Mutex<Database>,
    operation: tokio::sync::Mutex<()>,
    account_operation: tokio::sync::Mutex<()>,
    progress_scanned: tokio::sync::Mutex<HashSet<(String, String)>>,
    games: Mutex<HashMap<String, CancellationToken>>,
    mod_summaries: version_management::SummaryCache,
    modpack_migration_issues: Mutex<HashMap<(String, String), String>>,
    sessions: auth::SessionCache,
}
impl Drop for Shared {
    fn drop(&mut self) {
        // A concurrent fork can retain the open file description until exec.
        // Release ownership with the last Launcher, not the last inherited fd.
        let _ = self._lock.unlock();
    }
}
#[derive(Clone)]
pub struct Launcher(Arc<Shared>);
#[derive(Serialize)]
pub struct Snapshot {
    pub root: String,
    pub system_locale: String,
    pub database: Database,
    pub running: Vec<String>,
}
impl Launcher {
    /// Only call after the host has stopped accepting work and is about to exit.
    /// Relaunch may spawn the replacement before this process has fully exited.
    pub fn release_data_lock_for_restart(&self) -> Result<()> {
        self.0._lock.unlock().context("无法释放启动器数据目录锁")
    }

    pub fn open(root: Option<PathBuf>) -> Result<Self> {
        let root = storage::data_root(root)?;
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("launcher.lock"))?;
        lock.try_lock()
            .context("该数据目录已被另一个 NekoCrate 进程占用，请先关闭它")?;
        let dbpath = root.join("database.json");
        let db = migration::load_database(&root, &dbpath)?;
        for dir in [
            "instances",
            "cache",
            "metadata",
            "assets",
            "libraries",
            "versions",
            "runtimes",
        ] {
            fs::create_dir_all(root.join(dir))?;
        }
        let launcher = Self(Arc::new(Shared {
            root,
            _lock: lock,
            db: Mutex::new(db),
            operation: tokio::sync::Mutex::new(()),
            account_operation: tokio::sync::Mutex::new(()),
            progress_scanned: tokio::sync::Mutex::new(HashSet::new()),
            games: Mutex::new(HashMap::new()),
            mod_summaries: version_management::SummaryCache::default(),
            modpack_migration_issues: Mutex::default(),
            sessions: auth::SessionCache::default(),
        }));
        launcher.migrate_modpack_records();
        launcher.refresh_local_versions()?;
        launcher.clean_old_java_archives();
        if let Err(error) =
            download_tasks::reconcile(launcher.root(), &launcher.snapshot().database.instances)
        {
            let _ = diagnostics::record_error(launcher.root(), "download_cleanup", &error);
        }
        Ok(launcher)
    }
    pub fn root(&self) -> &Path {
        &self.0.root
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            root: self.root().to_string_lossy().into(),
            system_locale: language::system_locale(),
            database: self.0.db.lock().unwrap().clone(),
            running: self.0.games.lock().unwrap().keys().cloned().collect(),
        }
    }
    pub fn settings(&self) -> Settings {
        self.0.db.lock().unwrap().settings.clone()
    }
    pub fn memory_options(&self, id: &str) -> Result<memory::MemoryOptions> {
        Ok(memory::options(self.root(), &self.instance(id)?))
    }
    /// Reconcile linked/managed profiles once at startup, not on every UI snapshot.
    /// Existing packs become launchable without installing them again.
    fn refresh_local_versions(&self) -> Result<()> {
        let mut updates = Vec::new();
        for instance in self.snapshot().database.instances {
            let game = instance.game_dir(self.root());
            if !game.is_dir() {
                continue;
            }
            let Ok(discovered) = self.discover_versions(&game, "") else {
                continue;
            };
            let mut versions = instance.versions.clone();
            for mut found in discovered {
                if let Some(existing) = versions.iter_mut().find(|v| v.id == found.id) {
                    if found.modpack || loader::has_record(&game, &found.id) {
                        found.name = existing.name.clone().or(found.name);
                        *existing = found;
                        continue;
                    }
                    // Readiness is only a cheap discovery hint; a prior completed
                    // install is not downgraded by this scan. Launch validates it.
                    found.installed |= existing.installed;
                    found.name = existing.name.clone();
                    found.modpack = existing.modpack;
                    if existing.modpack {
                        found.installed = existing.installed;
                    }
                    found.download_progress = if found.installed {
                        100.0
                    } else {
                        existing.download_progress
                    };
                    *existing = found;
                } else {
                    versions.push(found);
                }
            }
            if serde_json::to_value(&versions)? != serde_json::to_value(&instance.versions)? {
                updates.push((instance.id, versions));
            }
        }
        if !updates.is_empty() {
            self.mutate(|db| {
                for (id, versions) in updates {
                    if let Some(instance) = db.instances.iter_mut().find(|i| i.id == id) {
                        instance.versions = versions;
                        if instance.selected_version.is_none() {
                            instance.selected_version =
                                instance.versions.first().map(|v| v.id.clone());
                        }
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    }
    /// Backfill old interrupted installs once per process without network work.
    pub async fn recover_download_progress(&self) -> Result<bool> {
        if self.0.operation.try_lock().is_err() {
            return Ok(false);
        }
        let Ok(mut scanned) = self.0.progress_scanned.try_lock() else {
            return Ok(false);
        };
        let mut changed = false;
        for instance in self.snapshot().database.instances {
            for version in &instance.versions {
                let key = (instance.id.clone(), version.id.clone());
                if version.installed || version.download_progress > 0.0 || scanned.contains(&key) {
                    continue;
                }
                let game = instance.game_dir(self.root());
                let progress =
                    match minecraft::cached_installation_files(self.root(), &game, &version.id)
                        .await
                    {
                        Ok(files) => download::local_progress(self.root(), files).await?,
                        Err(_) => {
                            scanned.insert(key);
                            continue;
                        }
                    };
                // A download may have started while we were reading. Its live
                // progress wins; retry this scan on a later idle snapshot.
                let Ok(_guard) = self.0.operation.try_lock() else {
                    return Ok(changed);
                };
                let current = self.instance(&instance.id)?;
                if current
                    .versions
                    .iter()
                    .find(|entry| entry.id == version.id)
                    .is_none_or(|entry| {
                        entry.installed || entry.download_progress != version.download_progress
                    })
                {
                    continue;
                }
                if (progress - version.download_progress).abs() >= 0.01 {
                    self.save_download_progress(&instance.id, &version.id, progress)?;
                    changed = true;
                }
                scanned.insert(key);
            }
        }
        Ok(changed)
    }
    pub fn ui_language(&self) -> &'static str {
        language::resolve(&self.settings().language, &language::system_locale())
    }
    fn mutate<T>(&self, operation: impl FnOnce(&mut Database) -> Result<T>) -> Result<T> {
        let mut db = self.0.db.lock().unwrap();
        let mut candidate = db.clone();
        let result = operation(&mut candidate)?;
        storage::write_json(&self.root().join("database.json"), &candidate)?;
        *db = candidate;
        Ok(result)
    }
    fn idle(&self, id: &str) -> Result<()> {
        if self.0.games.lock().unwrap().contains_key(id) {
            bail!("游戏运行中，请先停止该实例再修改或修复");
        }
        Ok(())
    }
    pub fn instance(&self, id: &str) -> Result<Instance> {
        storage::safe_id(id)?;
        self.0
            .db
            .lock()
            .unwrap()
            .instances
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .context("实例不存在")
    }
    pub fn instance_dir(&self, id: &str) -> Result<PathBuf> {
        self.instance(id)?;
        Ok(self.root().join("instances").join(id))
    }
    pub fn game_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.instance(id)?.game_dir(self.root()))
    }
    /// Reorder metadata only; game processes and directory contents stay intact.
    pub fn move_instance(&self, id: &str, before_id: Option<&str>) -> Result<()> {
        self.mutate(|db| {
            let from = db
                .instances
                .iter()
                .position(|i| i.id == id)
                .context("实例不存在")?;
            let before = before_id
                .map(|target| {
                    db.instances
                        .iter()
                        .position(|i| i.id == target)
                        .context("目标实例不存在，请刷新后重试")
                })
                .transpose()?;
            if before == Some(from) {
                return Ok(());
            }
            let instance = db.instances.remove(from);
            let destination = before
                .map(|index| if index > from { index - 1 } else { index })
                .unwrap_or(db.instances.len());
            db.instances.insert(destination, instance);
            Ok(())
        })
    }
    fn available_game_dir(&self, id: &str) -> Result<PathBuf> {
        let instance = self.instance(id)?;
        let game = instance.game_dir(self.root());
        if instance.game_directory.is_some() && !game.is_dir() {
            bail!(
                "关联的游戏目录不可用，请恢复原路径或重新添加目录：{}",
                game.display()
            );
        }
        Ok(game)
    }
    pub fn add_version(&self, id: &str, version: &str) -> Result<Instance> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后添加版本")?;
        self.idle(id)?;
        storage::safe_version(version)?;
        self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            if !instance.versions.iter().any(|v| v.id == version) {
                instance.versions.push(version::describe(
                    self.root(),
                    &instance.game_dir(self.root()),
                    version,
                ));
            }
            instance.selected_version = Some(version.into());
            Ok(instance.clone())
        })
    }
    pub fn select_version(&self, id: &str, version: &str) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后切换版本")?;
        self.idle(id)?;
        self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            if !instance.versions.iter().any(|v| v.id == version) {
                bail!("此实例未添加该版本");
            }
            instance.selected_version = Some(version.into());
            Ok(())
        })
    }
    fn set_installed(&self, id: &str, version: &str, installed: bool) -> Result<()> {
        if installed {
            self.save_pack_progress(id, version, 100.0)?;
        }
        self.mutate(|db| {
            let entry = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .and_then(|i| i.versions.iter_mut().find(|v| v.id == version))
                .context("实例中不存在该版本")?;
            entry.installed = installed;
            if installed {
                entry.download_progress = 100.0;
            }
            Ok(())
        })
    }
    fn save_download_progress(&self, id: &str, version: &str, progress: f64) -> Result<()> {
        self.save_pack_progress(id, version, progress.clamp(0.0, 99.0))?;
        self.mutate(|db| {
            let entry = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .and_then(|i| i.versions.iter_mut().find(|v| v.id == version))
                .context("实例中不存在该版本")?;
            entry.download_progress = progress.clamp(0.0, 99.0);
            Ok(())
        })
    }
    pub fn save_settings(&self, mut settings: Settings) -> Result<()> {
        // Older "official" meant the original publisher, not a distinct catalog.
        if settings.mod_download_source == "official" {
            settings.mod_download_source = "auto".into();
        }
        self.save_settings_with_vault(settings, &proxy_credentials::SystemVault(self.root()))
    }
    pub fn proxy_password(&self) -> Result<String> {
        use proxy_credentials::ProxyVault;
        let settings = self.settings();
        settings.proxy_password_key.as_deref().map_or_else(
            || Ok(String::new()),
            |key| proxy_credentials::SystemVault(self.root()).read(key),
        )
    }
    fn save_settings_with_vault(
        &self,
        mut settings: Settings,
        vault: &impl proxy_credentials::ProxyVault,
    ) -> Result<()> {
        // Active jobs own a Settings snapshot. Persist future preferences under
        // the database lock without waiting for downloads or changing their plan.
        if !(1..=download_concurrency::MAX_CONCURRENCY).contains(&settings.concurrency)
            || settings.retries > 5
        {
            bail!("并发应为 1–256，重试次数应为 0–5");
        }
        settings.proxy_url = settings.proxy_url.trim().to_owned();
        network::validate(&settings)?;
        if !["system", "zh-CN", "en-US"].contains(&settings.language.as_str()) {
            bail!("不支持的语言选项");
        }
        settings.microsoft_client_id = settings.microsoft_client_id.trim().to_owned();
        if !settings.microsoft_client_id.is_empty() {
            uuid::Uuid::parse_str(&settings.microsoft_client_id).context("Client ID 应为 UUID")?;
        }
        let mut created_key = None;
        let mut obsolete_key = None;
        let result = self.mutate(|db| {
            // Never trust a key supplied by IPC. New secrets use an immutable
            // entry so a failed database write cannot destroy the previous one.
            settings.proxy_password_key = db.settings.proxy_password_key.clone();
            if let Some(password) = settings.proxy_password.take() {
                obsolete_key = settings.proxy_password_key.take();
                if !password.0.is_empty() {
                    let key = uuid::Uuid::new_v4().to_string();
                    vault.write(&key, &password.0)?;
                    created_key = Some(key.clone());
                    settings.proxy_password_key = Some(key);
                }
            }
            db.settings = settings;
            Ok(())
        });
        if result.is_err() {
            if let Some(key) = created_key {
                let _ = vault.remove(&key);
            }
        } else if let Some(key) = obsolete_key {
            let _ = vault.remove(&key);
        }
        result
    }
    pub fn create_instance(&self, name: &str, version: &str) -> Result<Instance> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后创建实例")?;
        self.create_unlocked(name, version, None)
    }
    fn create_unlocked(
        &self,
        name: &str,
        version: &str,
        game_directory: Option<PathBuf>,
    ) -> Result<Instance> {
        let name = name.trim();
        instance_creation::validate(name, version)?;
        let identity = uuid::Uuid::new_v4();
        let official = instance_discovery::official_directory();
        let icon = if game_directory
            .as_deref()
            .is_some_and(|path| instance_discovery::is_official_path(path, official.as_deref()))
        {
            0
        } else {
            identity.as_bytes()[0] % INSTANCE_ICON_COUNT
        };
        let i = Instance {
            id: identity.to_string(),
            name: name.into(),
            icon: Some(icon),
            game_directory,
            versions: if version.is_empty() {
                vec![]
            } else {
                vec![InstanceVersion::pending(version)]
            },
            selected_version: (!version.is_empty()).then(|| version.into()),
            memory_mb: 2048,
            memory_mode: MemoryMode::Auto,
            java_path: None,
            created_at: storage::now(),
            last_played: None,
        };
        let dir = self.root().join("instances").join(&i.id);
        if i.game_directory.is_some() {
            fs::create_dir_all(&dir)?;
        } else {
            fs::create_dir_all(dir.join(".minecraft"))?;
        }
        if let Err(e) = self.mutate(|db| {
            db.instances.push(i.clone());
            Ok(())
        }) {
            let _ = fs::remove_dir_all(dir);
            return Err(e);
        }
        Ok(i)
    }
    pub fn update_instance(
        &self,
        id: &str,
        name: &str,
        memory_mb: u32,
        java_path: Option<String>,
        icon: Option<u8>,
    ) -> Result<()> {
        self.update_instance_memory(
            id,
            name,
            memory::MemoryConfig {
                mode: MemoryMode::Custom,
                custom_mb: memory_mb,
            },
            java_path,
            icon,
        )
    }
    pub fn update_instance_memory(
        &self,
        id: &str,
        name: &str,
        memory: memory::MemoryConfig,
        java_path: Option<String>,
        icon: Option<u8>,
    ) -> Result<()> {
        let memory_mb = memory.custom_mb;
        // Metadata-only edits use the database lock. Install/launch already own
        // an Instance snapshot; saving preferences must not wait for downloads
        // or overwrite their version state, directory or installation plan.
        self.idle(id)?;
        if name.trim().is_empty()
            || name.chars().count() > 80
            || !(512..=65536).contains(&memory_mb)
        {
            bail!("名称应为 1–80 个字符，内存应为 512–65536 MiB");
        }
        if icon.is_some_and(|value| value >= INSTANCE_ICON_COUNT) {
            bail!("请选择有效的实例图标");
        }
        self.mutate(|db| {
            let i = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            i.name = name.trim().into();
            i.memory_mb = memory_mb;
            i.memory_mode = memory.mode;
            i.java_path = java_path.filter(|s| !s.trim().is_empty());
            if let Some(icon) = icon {
                i.icon = Some(icon);
            }
            Ok(())
        })
    }
    fn linkable_directory(&self, source: &Path, except_id: Option<&str>) -> Result<PathBuf> {
        let source = folder_import::resolve_game_directory(source)?;
        self.check_external_directory(&source, except_id)?;
        Ok(source)
    }
    fn check_external_directory(&self, source: &Path, except_id: Option<&str>) -> Result<()> {
        if source.starts_with(self.root()) || self.root().starts_with(source) {
            bail!("游戏目录不能与启动器数据目录重叠，请选择其他位置");
        }
        for existing in &self.snapshot().database.instances {
            if except_id == Some(existing.id.as_str()) {
                continue;
            }
            let directory = existing.game_dir(self.root());
            let directory = directory.canonicalize().unwrap_or(directory);
            if source.starts_with(&directory) || directory.starts_with(source) {
                bail!(
                    "此目录已由实例「{}」使用，不能重复添加或嵌套关联",
                    existing.name
                );
            }
        }
        Ok(())
    }
    fn discover_versions(&self, source: &Path, version: &str) -> Result<Vec<InstanceVersion>> {
        self.discover_versions_cancellable(source, version, None)
    }
    fn discover_versions_cancellable(
        &self,
        source: &Path,
        version: &str,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<InstanceVersion>> {
        let mut versions = vec![];
        if source.join("versions").is_dir() {
            for entry in fs::read_dir(source.join("versions"))? {
                if let Some(cancel) = cancel {
                    download::check_cancel(cancel)?;
                }
                let entry = entry?;
                let id = entry.file_name().to_string_lossy().into_owned();
                if !entry.file_type()?.is_dir() || storage::safe_version(&id).is_err() {
                    continue;
                }
                if let Ok(loader) = loader::describe(source, &id) {
                    versions.push(loader);
                    continue;
                }
                if let Ok(pack) = modpack::describe(source, &id) {
                    versions.push(pack);
                    continue;
                }
                if let Ok(metadata) = storage::read_json::<serde_json::Value>(
                    &entry.path().join(format!("{id}.json")),
                ) && metadata["id"].as_str() == Some(&id)
                {
                    versions.push(version::describe(self.root(), source, &id));
                }
            }
        }
        versions.sort_by(|a, b| a.id.cmp(&b.id));
        if !version.is_empty() && !versions.iter().any(|v| v.id == version) {
            storage::safe_version(version)?;
            versions.push(version::describe(self.root(), source, version));
        }
        Ok(versions)
    }
    /// Change only the association. Never copy, move or delete either game tree.
    /// Previously installed versions must exist unchanged at the new location.
    pub async fn relink_instance(&self, id: &str, source: PathBuf) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let previous = self.instance(id)?;
        let source = self.linkable_directory(&source, Some(id))?;
        let old_game = previous.game_dir(self.root());
        let mut versions = self.discover_versions(&source, "")?;
        for old in &previous.versions {
            if old.installed {
                let old_version = version::resolve(self.root(), &old_game, &old.id)?;
                let new_version = version::resolve(self.root(), &source, &old.id)?;
                if serde_json::to_value(&old_version.version)?
                    != serde_json::to_value(&new_version.version)?
                {
                    bail!("目标目录的版本配置不同：{}，原关联未修改", old.id);
                }
                for (old_path, new_path) in [
                    (
                        version::metadata_path(&old_game, &old.id)?,
                        version::metadata_path(&source, &old.id)?,
                    ),
                    (
                        version::client_path(&old_game, &old_version.version)?,
                        version::client_path(&source, &new_version.version)?,
                    ),
                ] {
                    if !storage::same_file_contents(&old_path, &new_path)? {
                        bail!(
                            "目标目录缺少或包含不同的已安装版本 {}；请先核对文件，原关联未修改",
                            old.id
                        );
                    }
                }
            }
            if let Some(found) = versions.iter_mut().find(|v| v.id == old.id) {
                found.installed = old.installed;
                found.name = old.name.clone();
                found.download_progress = old.download_progress;
            } else if old.installed {
                bail!("目标目录无法识别已安装版本 {}，原关联未修改", old.id);
            } else {
                versions.push(old.clone());
            }
        }
        versions.sort_by_key(|version| {
            previous
                .versions
                .iter()
                .position(|old| old.id == version.id)
                .unwrap_or(usize::MAX)
        });
        let selected = previous
            .selected_version
            .as_ref()
            .filter(|id| versions.iter().any(|v| &v.id == *id))
            .cloned()
            .or_else(|| versions.first().map(|v| v.id.clone()));
        let backup = self.instance_dir(id)?.join("directory-link-before.json");
        storage::write_json(&backup, &previous)?;
        self.mutate(|db| {
            let target = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            target.game_directory = Some(source);
            target.versions = versions;
            target.selected_version = selected;
            Ok(())
        })?;
        self.instance(id)
    }
    pub async fn import_instance(
        &self,
        source: PathBuf,
        name: String,
        version: String,
    ) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        self.import_unlocked(source, name, version)
    }
    fn import_unlocked(&self, source: PathBuf, name: String, version: String) -> Result<Instance> {
        let source = self.linkable_directory(&source, None)?;
        let versions = self.discover_versions(&source, &version)?;
        let name = if name.trim().is_empty() {
            folder_import::default_name(&source)
        } else {
            name
        };
        let i = self.create_unlocked(&name, "", Some(source))?;
        if let Err(e) = self.mutate(|db| {
            let target = db.instances.iter_mut().find(|x| x.id == i.id).unwrap();
            target.selected_version = if version.is_empty() {
                versions.first().map(|v| v.id.clone())
            } else {
                Some(version)
            };
            target.versions = versions;
            Ok(())
        }) {
            self.rollback_new(&i.id)?;
            return Err(e);
        }
        self.instance(&i.id)
    }
    fn rollback_new(&self, id: &str) -> Result<()> {
        self.mutate(|db| {
            db.instances.retain(|i| i.id != id);
            Ok(())
        })?;
        fs::remove_dir_all(self.root().join("instances").join(id))?;
        Ok(())
    }
    pub async fn clone_instance(&self, id: &str) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let original = self.instance(id)?;
        let source = self.available_game_dir(id)?;
        let i = self.create_unlocked(&format!("{} · 副本", original.name), "", None)?;
        let dest = self.game_dir(&i.id)?;
        if let Err(e) =
            tokio::task::spawn_blocking(move || storage::copy_tree(&source, &dest)).await?
        {
            self.rollback_new(&i.id)?;
            return Err(e);
        }
        self.mutate(|db| {
            let target = db.instances.iter_mut().find(|x| x.id == i.id).unwrap();
            target.versions = original.versions.clone();
            target.selected_version = original.selected_version.clone();
            target.memory_mb = original.memory_mb;
            target.memory_mode = original.memory_mode;
            target.java_path = original.java_path.clone();
            target.icon = original.icon;
            Ok(())
        })?;
        self.instance(&i.id)
    }
    pub fn delete_instance(&self, id: &str) -> Result<()> {
        self.delete_instance_with_recycle(id, recycle::move_to_trash)
    }
    pub(crate) fn delete_instance_with_recycle(
        &self,
        id: &str,
        recycle: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后移除实例")?;
        self.idle(id)?;
        let instance = self.instance(id)?;
        let source = self.instance_dir(id)?;
        storage::write_json(&source.join("instance.json"), &instance)?;
        self.mutate_recycling(
            &source,
            |db| {
                db.instances.retain(|i| i.id != id);
                Ok(())
            },
            recycle,
        )?;
        if let Err(error) = download_tasks::remove_owner(self.root(), id, None) {
            let _ = diagnostics::record_error(self.root(), "download_cleanup", &error);
        }
        let game = instance.game_dir(self.root());
        if let Err(error) = resources::remove_version_tasks(&game, id, None) {
            let _ = diagnostics::record_error(self.root(), "download_cleanup", &error);
        }
        Ok(())
    }
    pub async fn versions(&self, cancel: &CancellationToken) -> Result<VersionManifest> {
        minecraft::manifest(
            self.root(),
            &download::Downloader::new(self.root(), self.settings())?,
            cancel,
        )
        .await
    }
    pub fn cached_versions(&self) -> Option<VersionManifest> {
        storage::read_json(&self.root().join("metadata/manifest.json")).ok()
    }
    pub async fn fabric_versions(
        &self,
        game_version: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<fabric::LoaderVersion>> {
        fabric::versions(
            self.root(),
            game_version,
            &download::Downloader::new(self.root(), self.settings())?,
            cancel,
        )
        .await
    }

    pub async fn add_fabric_version(
        &self,
        id: &str,
        game_version: &str,
        loader_version: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let game = self.available_game_dir(id)?;
        let profile_id = fabric::version_id(game_version, loader_version)?;
        let path = version::metadata_path(&game, &profile_id)?;
        let downloader = download::Downloader::new(self.root(), self.settings())?;
        sink(Event::workflow("metadata", "解析 Fabric 版本"));
        let profile = fabric::profile(
            self.root(),
            game_version,
            loader_version,
            &downloader,
            cancel,
        )
        .await?;
        let parent =
            minecraft::version(self.root(), game_version, &downloader, cancel, sink).await?;
        // Resolve everything before touching an existing game tree.
        version::resolve_chain(&[serde_json::to_value(&parent)?, profile.clone()])?;
        download::check_cancel(cancel)?;
        let parent_path = version::metadata_path(&game, game_version)?;
        if !parent_path.exists() {
            storage::write_json(&parent_path, &parent)?;
        }
        if path.exists() {
            let existing: serde_json::Value = storage::read_json(&path)?;
            if version::detect_loader(&existing) != version::detect_loader(&profile)
                || existing["inheritsFrom"].as_str() != Some(game_version)
            {
                bail!("同名版本已经存在且配置不同，请保留原版本或使用其他加载器版本");
            }
        } else {
            storage::write_json(&path, &profile)?;
        }
        let mut entry = version::describe(self.root(), &game, &profile_id);
        entry.minecraft_version = Some(game_version.into());
        entry.loader = Some(ModLoader {
            kind: "fabric".into(),
            version: loader_version.into(),
        });
        self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            if let Some(existing) = instance.versions.iter_mut().find(|v| v.id == profile_id) {
                entry.name = existing.name.clone();
                entry.download_progress = existing.download_progress;
                *existing = entry;
            } else {
                instance.versions.push(entry);
            }
            instance.selected_version = Some(profile_id);
            Ok(instance.clone())
        })
    }
    pub async fn install(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        self.install_with_verification(id, cancel, sink, false)
            .await
    }
    pub async fn repair(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        self.install_with_verification(id, cancel, sink, true).await
    }
    async fn install_with_verification(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
        full_verification: bool,
    ) -> Result<()> {
        if let Some(selected) = self.instance(id)?.selected().filter(|v| v.modpack) {
            return self
                .install_modpack(id, &selected.id, cancel, sink, full_verification)
                .await;
        }
        if let Some(selected) = self.instance(id)?.selected() {
            let game = self.available_game_dir(id)?;
            if loader::has_record(&game, &selected.id) {
                return self
                    .install_loader(id, &selected.id, cancel, sink, full_verification)
                    .await;
            }
        }
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let sink = Event::scoped(sink, id, None);
        sink(Event::workflow("metadata", "解析游戏版本"));
        let instance = self.instance(id)?;
        let selected = instance
            .selected()
            .context("请先为此实例添加并选择一个版本")?;
        let game = self.available_game_dir(id)?;
        download::check_cancel(cancel)?;
        let local: Option<serde_json::Value> =
            storage::read_json(&version::metadata_path(&game, &selected.id)?).ok();
        let is_fabric = local
            .as_ref()
            .and_then(version::detect_loader)
            .is_some_and(|l| l.kind == "fabric");
        if !is_fabric {
            minecraft::check_local_install(&game, &selected.id)?;
        }
        let settings = self.settings();
        let downloader = download::Downloader::new(self.root(), settings.clone())?
            .for_installation(self.root(), id, &game, &selected.id, "game")?;
        let downloader = if full_verification {
            downloader.with_full_verification()
        } else {
            downloader
        };
        let v = if is_fabric {
            sink(Event::new("stage", "解析 Fabric 版本"));
            let mut profile = local.unwrap();
            fabric::prepare_libraries(&mut profile, &downloader, cancel).await?;
            // Save hashes so pause/cancel/restart can reuse exactly the same plan.
            storage::write_json(&version::metadata_path(&game, &selected.id)?, &profile)?;
            let parent_id = profile["inheritsFrom"]
                .as_str()
                .context("Fabric 配置缺少基础版本")?;
            if !version::metadata_path(&game, parent_id)?.is_file() {
                let parent =
                    minecraft::version(self.root(), parent_id, &downloader, cancel, &sink).await?;
                storage::write_json(&version::metadata_path(&game, parent_id)?, &parent)?;
            }
            version::resolve(self.root(), &game, &selected.id)?.version
        } else {
            sink(Event::new("stage", "获取官方版本元数据"));
            minecraft::version(self.root(), &selected.id, &downloader, cancel, &sink).await?
        };
        // Resolving metadata does not change the game tree or its installed state.
        self.set_installed(id, &selected.id, false)?;
        sink(Event::workflow("files", "下载与校验游戏文件"));
        let progress = Arc::new(Mutex::new((
            selected.download_progress.min(99.0),
            std::time::Instant::now(),
        )));
        let captured = progress.clone();
        let launcher = self.clone();
        let instance_id = id.to_owned();
        let version_id = selected.id.clone();
        let forward = Event::scoped(&sink, id, Some("files"));
        let file_sink: EventSink = Arc::new(move |event| {
            if event.kind == "progress" && event.total > 0 {
                let percentage = if event.total_bytes > 0 {
                    event.bytes as f64 / event.total_bytes as f64 * 100.0
                } else {
                    event.completed as f64 / event.total as f64 * 100.0
                };
                let mut state = captured.lock().unwrap();
                state.0 = if event.reset_progress {
                    percentage.clamp(0.0, 99.0)
                } else {
                    state.0.max(percentage.min(99.0))
                };
                if event.reset_progress
                    || state.1.elapsed() >= std::time::Duration::from_secs(1)
                    || event.completed == event.total
                {
                    if let Err(error) =
                        launcher.save_download_progress(&instance_id, &version_id, state.0)
                    {
                        forward(Event::new("warning", format!("保存下载进度失败：{error}")));
                    }
                    state.1 = std::time::Instant::now();
                }
            }
            forward(event);
        });
        let result = minecraft::install_files(&game, &v, &downloader, cancel, &file_sink).await;
        // Also save the final fraction on a quick cancellation or error.
        self.save_download_progress(id, &selected.id, progress.lock().unwrap().0)?;
        result?;
        sink(Event::workflow("java", "准备 Java"));
        let java = java::ensure(
            self.root(),
            java::JavaRequirement::for_instance(&v, &game, instance.java_path.as_deref()).await?,
            instance.java_path.as_deref(),
            settings.auto_java,
            &downloader,
            cancel,
            &Event::scoped(&sink, id, Some("java")),
        )
        .await?;
        download::check_cancel(cancel)?;
        sink(Event::workflow("finalize", "保存安装信息"));
        let version_dir = self.game_dir(id)?.join("versions").join(&selected.id);
        if !is_fabric {
            storage::write_json(&version_dir.join(format!("{}.json", selected.id)), &v)?;
        }
        storage::write_json(&version_dir.join("runtime.json"), &java)?;
        self.set_installed(id, &selected.id, true)?;
        downloader.finish_installation(&sink).await;
        sink(Event::new("complete", "游戏文件与 Java 已准备好"));
        Ok(())
    }
    pub async fn begin_login(&self) -> Result<auth::DeviceChallenge> {
        auth::begin(
            &download::Downloader::new(self.root(), self.settings())?.client,
            &self.settings().microsoft_client_id,
        )
        .await
    }
    pub async fn finish_login(
        &self,
        challenge: auth::DeviceChallenge,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let client = download::Downloader::new(self.root(), self.settings())?.client;
        let session = auth::finish(&client, challenge, cancel).await?;
        // Waiting for a browser authorization does not block existing accounts.
        let _guard = self.0.account_operation.lock().await;
        download::check_cancel(cancel)?;
        self.store_refresh_token(&session.account, session.refresh_token.clone())?;
        let account = session.account.clone();
        self.save_account(account.clone(), true)?;
        self.0.sessions.remember(session);
        Ok(account)
    }
    fn store_refresh_token(&self, account: &Account, token: String) -> Result<()> {
        // Commit storage and memory together while the account operation is
        // locked; cancellation must not leave a detached write after removal.
        auth::save_secret(self.root(), &account.id, &token)?;
        self.0.sessions.remember_refresh(account, token);
        Ok(())
    }
    fn save_account(&self, account: Account, select: bool) -> Result<()> {
        self.mutate(|db| {
            if select {
                db.selected_account = Some(account.id.clone());
            }
            if let Some(existing) = db.accounts.iter_mut().find(|a| a.id == account.id) {
                *existing = account;
            } else {
                db.accounts.push(account);
            }
            Ok(())
        })
    }
    pub fn select_account(&self, id: &str) -> Result<()> {
        self.mutate(|db| {
            if !db.accounts.iter().any(|a| a.id == id) {
                bail!("账户不存在");
            }
            db.selected_account = Some(id.into());
            Ok(())
        })
    }
    pub fn select_guest(&self) -> Result<()> {
        self.mutate(|db| {
            db.selected_account = None;
            Ok(())
        })
    }
    pub fn set_guest_name(&self, name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty()
            || name.len() > 16
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            bail!("游客名称须为 1–16 位英文字母、数字或下划线");
        }
        self.mutate(|db| {
            db.guest_name = name.into();
            Ok(())
        })?;
        Ok(name.into())
    }
    pub fn remove_account(&self, id: &str) -> Result<()> {
        let _guard = self
            .0
            .account_operation
            .try_lock()
            .context("有任务正在使用账户，请稍后移除")?;
        auth::remove_secret(self.root(), id)?;
        self.0.sessions.remove(id);
        self.mutate(|db| {
            db.accounts.retain(|a| a.id != id);
            if db.guest_skin == format!("account:{id}") {
                db.guest_skin = "builtin:steve".into();
            }
            if db.selected_account.as_deref() == Some(id) {
                db.selected_account = db.accounts.first().map(|a| a.id.clone());
            }
            Ok(())
        })
    }
    async fn session(&self, force_refresh: bool) -> Result<auth::MinecraftSession> {
        // Capture the identity before waiting: a later UI selection must never
        // redirect an already requested account operation to another player.
        let id = self
            .snapshot()
            .database
            .selected_account
            .context("请先添加并选择微软账户")?;
        let _guard = self.0.account_operation.lock().await;
        self.session_for_account(&id, force_refresh).await
    }
    // Caller holds account_operation, independently of downloads/installations.
    async fn session_for_account(
        &self,
        id: &str,
        force_refresh: bool,
    ) -> Result<auth::MinecraftSession> {
        let account = self
            .snapshot()
            .database
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .context("所选账户不存在")?;
        if force_refresh {
            self.0.sessions.invalidate_session(id);
        } else if let Some(session) = self.0.sessions.session(&account) {
            return Ok(session);
        }
        let secret = match self.0.sessions.refresh_token(&account) {
            Some(token) => token,
            None => {
                let root = self.root().to_owned();
                let account_id = id.to_owned();
                let token =
                    tokio::task::spawn_blocking(move || auth::load_secret(&root, &account_id))
                        .await??;
                self.0.sessions.remember_refresh(&account, token.clone());
                token
            }
        };
        let client = download::Downloader::new(self.root(), self.settings())?.client;
        let session = auth::refresh(&client, &account, &secret, |token| {
            std::future::ready(self.store_refresh_token(&account, token))
        })
        .await?;
        if session.account.id != id {
            bail!("刷新后账户发生变化，请重新登录");
        }
        self.save_account(session.account.clone(), false)?;
        self.0.sessions.remember(session.clone());
        Ok(session)
    }
    pub async fn refresh_account(&self) -> Result<Account> {
        Ok(self.session(true).await?.account)
    }
    pub async fn account_skin(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let account = self
            .snapshot()
            .database
            .accounts
            .into_iter()
            .find(|account| account.id == id)
            .context("账户不存在")?;
        match account.skin {
            Some(skin) => Ok(Some(
                skin::load(self.root(), &skin, &self.settings()).await?,
            )),
            None => Ok(None),
        }
    }
    async fn launch_session(&self, sink: &EventSink) -> Result<minecraft::LaunchSession> {
        let database = self.snapshot().database;
        if database.selected_account.is_none() {
            sink(Event::new(
                "stage",
                format!("以游客身份 {} 启动", database.guest_name),
            ));
            Ok(minecraft::LaunchSession::guest_named(&database.guest_name))
        } else {
            sink(Event::new("stage", "准备登录会话"));
            // An expired or invalid selected account must surface its error,
            // rather than silently change the player's identity and save data.
            Ok(self.session(false).await?.into())
        }
    }
    pub async fn launch(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: EventSink,
    ) -> Result<u32> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let sink = Event::scoped(&sink, id, None);
        sink(Event::workflow("prepare", "检查启动配置"));
        let instance = self.instance(id)?;
        let game = self.available_game_dir(id)?;
        let selected = instance.selected().context("请先选择游戏版本")?;
        let mut resolved = version::resolve(self.root(), &game, &selected.id)?;
        let settings = self.settings();
        let downloader = download::Downloader::new(self.root(), settings.clone())?
            .for_installation(self.root(), id, &game, &selected.id, "launch")?;
        runtime_compatibility::prepare(&game, &mut resolved.version, &downloader, cancel, &sink)
            .await?;
        version::check_launch_files(self.root(), &game, &resolved)?;
        // Assess Java against the original entry point/libraries: the color
        // overlay must not relax LaunchWrapper's existing Java 8 restriction.
        let java_requirement = java::JavaRequirement::for_instance(
            &resolved.version,
            &game,
            instance.java_path.as_deref(),
        )
        .await?;
        let compatibility_args = runtime_compatibility::prepare_legacy_colors(
            &game,
            &mut resolved,
            &downloader,
            cancel,
            &Event::scoped(&sink, id, Some("prepare")),
        )
        .await?;
        let v = resolved.version;
        let root = self.root().to_owned();
        let native_game = game.clone();
        let native_version = v.clone();
        let native_cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            minecraft::prepare_local_natives(&root, &native_game, &native_version, &native_cancel)
        })
        .await??;
        runtime_compatibility::prepare_openal(&game, &v, &downloader, cancel, &sink).await?;
        sink(Event::workflow("java", "准备 Java"));
        let java = java::ensure(
            self.root(),
            java_requirement,
            instance.java_path.as_deref(),
            settings.auto_java,
            &downloader,
            cancel,
            &Event::scoped(&sink, id, Some("java")),
        )
        .await?;
        sink(Event::workflow("account", "准备游戏身份"));
        downloader.finish_installation(&sink).await;
        let session = tokio::select! {_=cancel.cancelled()=>bail!("启动已取消"),r=self.launch_session(&sink)=>r?};
        sink(Event::workflow("start", "启动游戏"));
        let memory_instance = instance.clone();
        let memory_root = self.root().to_owned();
        let memory =
            tokio::task::spawn_blocking(move || memory::options(&memory_root, &memory_instance))
                .await?;
        let memory_mb = memory.allocation(
            memory::MemoryConfig {
                mode: instance.memory_mode,
                custom_mb: instance.memory_mb,
            },
            &java.arch,
        )?;
        sink(Event::new(
            "info",
            format!(
                "启动内存：{memory_mb} MiB（{}）",
                match instance.memory_mode {
                    MemoryMode::Auto => "自动",
                    MemoryMode::Recommended => "推荐配置",
                    MemoryMode::Minimum => "最小配置",
                    MemoryMode::Custom => "自定义",
                }
            ),
        ));
        let args =
            minecraft::arguments_with_memory(&v, self.root(), &instance, &session, memory_mb)?;
        let working_directory = version::working_directory(&game, &v)?;
        let log_dir = self.instance_dir(id)?.join("launcher-logs");
        fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("{}-{}.log", storage::now(), uuid::Uuid::new_v4()));
        let log = Arc::new(tokio::sync::Mutex::new(
            tokio::fs::File::create(&log_path).await?,
        ));
        let launched_at = std::time::SystemTime::now();
        let launch_version = selected.id.clone();
        let mut command = java::background_command(&java::game_executable(Path::new(&java.path)));
        command
            .args(compatibility_args)
            .args(args)
            .current_dir(&working_directory)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // The JVM also exposes its working directory as user.dir.
            command.current_dir(java::argument_path(&working_directory));
        }
        download::check_cancel(cancel)?;
        game_options::apply_language(&working_directory, &settings)?;
        let mut child = command
            .spawn()
            .context("无法启动 Java 进程，请检查 Java 路径和系统权限")?;
        let pid = child.id().context("无法读取游戏进程 ID")?;
        self.mutate(|db| {
            db.instances
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .last_played = Some(storage::now());
            Ok(())
        })?;
        let cancel = CancellationToken::new();
        self.0
            .games
            .lock()
            .unwrap()
            .insert(id.into(), cancel.clone());
        let secrets = Arc::new(session.log_secrets);
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = tokio::spawn(pipe_logs(
            stdout,
            id.into(),
            sink.clone(),
            log.clone(),
            secrets.clone(),
        ));
        let err = tokio::spawn(pipe_logs(
            stderr,
            id.into(),
            sink.clone(),
            log,
            secrets.clone(),
        ));
        let launcher = self.clone();
        let id = id.to_owned();
        sink(Event {
            instance_id: Some(id.clone()),
            ..Event::new("game_started", format!("游戏已启动 · PID {pid}"))
        });
        tokio::spawn(async move {
            let (status, stopped) = tokio::select! {
                r=child.wait()=>(r, false),
                _=cancel.cancelled()=>{let _=child.kill().await;(child.wait().await, true)}
            };
            let _ = out.await;
            let _ = err.await;
            launcher.0.games.lock().unwrap().remove(&id);
            let unexpected = !stopped
                && !cancel.is_cancelled()
                && !status.as_ref().is_ok_and(|status| status.success());
            let message = match status {
                _ if stopped => "游戏已停止".into(),
                Ok(s) if s.success() => "游戏已正常退出".into(),
                Ok(s) => format!(
                    "游戏已退出（状态 {s}）。如非主动停止，请查看日志，检查 Java、内存和游戏配置。"
                ),
                Err(e) => format!("读取游戏进程状态失败：{e}"),
            };
            let diagnosis = if unexpected {
                let root = launcher.clone();
                let exit = message.clone();
                tokio::task::spawn_blocking(move || {
                    let report = crash::collect(
                        &working_directory,
                        &log_path,
                        launched_at,
                        pid,
                        launch_version,
                        exit,
                        &secrets,
                    );
                    if let Err(error) =
                        storage::write_json(&log_dir.join("last-crash.json"), &report)
                    {
                        let _ = diagnostics::record_error(root.root(), "crash_diagnosis", &error);
                    }
                    report
                })
                .await
                .ok()
            } else {
                None
            };
            sink(Event {
                crash: diagnosis,
                instance_id: Some(id),
                ..Event::new(
                    if unexpected {
                        "game_crash"
                    } else {
                        "game_exit"
                    },
                    message,
                )
            });
        });
        Ok(pid)
    }
    pub async fn retry_download_file(
        &self,
        file: &download::RetryFile,
        id: Option<&str>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let _guard = self.0.operation.lock().await;
        if let Some(id) = id {
            self.instance(id)?;
            self.idle(id)?;
        }
        file.run(cancel, sink).await
    }
    pub fn stop_game(&self, id: &str) -> Result<()> {
        let games = self.0.games.lock().unwrap();
        let token = games.get(id).context("此实例未运行")?;
        token.cancel();
        Ok(())
    }
    pub fn stop_all(&self) {
        for token in self.0.games.lock().unwrap().values() {
            token.cancel();
        }
    }
}

async fn pipe_logs<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    id: String,
    sink: EventSink,
    file: Arc<tokio::sync::Mutex<tokio::fs::File>>,
    secrets: Arc<Vec<String>>,
) {
    let mut reader = BufReader::new(reader);
    let mut bytes = vec![];
    loop {
        bytes.clear();
        let mut overflow = false;
        let mut eof = false;
        loop {
            let available = match reader.fill_buf().await {
                Ok([]) | Err(_) => {
                    eof = true;
                    break;
                }
                Ok(bytes) => bytes,
            };
            let newline = available.iter().position(|b| *b == b'\n');
            let count = newline.map_or(available.len(), |index| index + 1);
            if !overflow && bytes.len() + count <= 65536 {
                bytes.extend_from_slice(&available[..count]);
            } else {
                overflow = true;
                bytes.clear();
            }
            reader.consume(count);
            if newline.is_some() {
                break;
            }
        }
        if eof && bytes.is_empty() && !overflow {
            break;
        }
        let mut line = if overflow {
            "[超长日志行已省略]".to_owned()
        } else {
            java::decode_output(&bytes).trim_end().to_string()
        };
        for secret in secrets.iter().filter(|s| !s.is_empty()) {
            line = line.replace(secret, "[REDACTED]");
        }
        if line.len() > 16384 {
            line = line.chars().take(4096).collect::<String>() + " … [truncated]";
        }
        let mut log = file.lock().await;
        let _ = log.write_all(format!("{line}\n").as_bytes()).await;
        drop(log);
        sink(Event {
            instance_id: Some(id.clone()),
            ..Event::new("log", line)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_install_persists_byte_progress_across_launcher_restarts() {
        use sha1::{Digest, Sha1};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.create_instance("Resume", "1.21.1").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabcdef")
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });
        let mut version = test_http::version_fixture("1.21.1");
        version.downloads.insert(
            "client".into(),
            Artifact {
                url,
                sha1: hex::encode(Sha1::digest(b"abcdefghi")),
                size: 9,
                path: None,
            },
        );
        let index = b"{\"objects\":{}}";
        version.asset_index.sha1 = hex::encode(Sha1::digest(index));
        version.asset_index.size = index.len() as u64;
        fs::create_dir_all(game.join("assets/indexes")).unwrap();
        fs::write(game.join("assets/indexes/1.21.1.json"), index).unwrap();
        let metadata = serde_json::to_vec(&version).unwrap();
        fs::create_dir_all(root.join("metadata/versions")).unwrap();
        fs::write(root.join("metadata/versions/1.21.1.json"), &metadata).unwrap();
        storage::write_json(
            &root.join("metadata/manifest.json"),
            &VersionManifest {
                latest: HashMap::new(),
                versions: vec![VersionEntry {
                    id: version.id,
                    kind: version.kind,
                    url: "http://127.0.0.1:9/must-not-fetch".into(),
                    sha1: hex::encode(Sha1::digest(&metadata)),
                }],
            },
        )
        .unwrap();
        let mut settings = launcher.settings();
        settings.proxy_mode = "none".into();
        launcher.save_settings(settings).unwrap();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let sink: EventSink = Arc::new(move |event| {
            if event.files.iter().any(|file| {
                file.name.ends_with(".jar") && file.status == "downloading" && file.downloaded == 6
            }) {
                token.cancel();
            }
        });
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            launcher.install(&instance.id, &cancel, &sink),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("取消"));
        server.abort();
        drop(launcher);
        let launcher = Launcher::open(Some(root)).unwrap();
        let restored = launcher.instance(&instance.id).unwrap();
        assert!(!restored.selected().unwrap().installed);
        assert!((restored.selected().unwrap().download_progress - 100.0 * 6.0 / 9.0).abs() < 0.01);
        launcher
            .update_instance(&instance.id, "Renamed", 4096, None, None)
            .unwrap();
        assert_eq!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .download_progress,
            restored.selected().unwrap().download_progress
        );
        // Older builds had no persisted progress. Recover without downloading
        // or touching any of the existing game files or partial object bytes.
        launcher
            .save_download_progress(&instance.id, "1.21.1", 0.0)
            .unwrap();
        assert!(launcher.recover_download_progress().await.unwrap());
        assert!(
            (launcher
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .download_progress
                - 100.0 * 6.0 / 9.0)
                .abs()
                < 0.01
        );
        assert!(!launcher.recover_download_progress().await.unwrap());
        assert!(!game.join("versions/1.21.1/1.21.1.jar").exists());
    }

    #[tokio::test]
    async fn unsupported_local_install_fails_before_network_or_changing_installed_state() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("original/.minecraft");
        let version_path = game.join("versions/RLCraft/RLCraft.json");
        let mut metadata = test_http::version_fixture("RLCraft");
        metadata.main_class = "net.minecraft.launchwrapper.Launch".into();
        metadata.arguments = None;
        metadata.minecraft_arguments = Some("--version 1.12.2".into());
        storage::write_json(&version_path, &metadata).unwrap();
        let original = fs::read(&version_path).unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(game.clone(), "RLCraft".into(), String::new())
            .await
            .unwrap();
        launcher
            .set_installed(&instance.id, "RLCraft", true)
            .unwrap();
        // Invalid proxy would fail client construction if preflight ran too late.
        launcher
            .mutate(|db| {
                db.settings.proxy_mode = "http".into();
                db.settings.proxy_url = "invalid proxy".into();
                Ok(())
            })
            .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        let error = launcher
            .install(&instance.id, &CancellationToken::new(), &sink)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("自定义加载器"));
        assert!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
        assert_eq!(fs::read(version_path).unwrap(), original);
        assert!(!launcher.root().join("metadata/manifest.json").exists());
    }

    #[test]
    fn instance_preferences_save_during_another_install_and_survive_its_completion() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let downloading = launcher.create_instance("Downloading", "1.21.1").unwrap();
        let other = launcher.create_instance("Other", "1.20.1").unwrap();
        let operation = launcher.0.operation.try_lock().unwrap();
        let active_snapshot = launcher.instance(&downloading.id).unwrap();
        launcher
            .set_installed(&downloading.id, "1.21.1", false)
            .unwrap();
        launcher
            .update_instance(
                &other.id,
                " Renamed ",
                4096,
                Some("/custom/bin/java".into()),
                Some(3),
            )
            .unwrap();
        assert!(
            launcher
                .update_instance(&other.id, "", 4096, None, Some(3))
                .is_err()
        );
        launcher
            .set_installed(&downloading.id, "1.21.1", true)
            .unwrap();
        assert_eq!(active_snapshot.name, "Downloading");
        assert_eq!(active_snapshot.memory_mb, downloading.memory_mb);
        assert_eq!(active_snapshot.java_path, downloading.java_path);
        drop(operation);
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let saved = launcher.instance(&other.id).unwrap();
        assert_eq!(saved.name, "Renamed");
        assert_eq!(saved.memory_mb, 4096);
        assert_eq!(saved.java_path.as_deref(), Some("/custom/bin/java"));
        assert_eq!(saved.icon, Some(3));
        assert_eq!(
            saved.game_dir(launcher.root()),
            other.game_dir(launcher.root())
        );
        assert_eq!(saved.selected_version, other.selected_version);
        assert!(!saved.selected().unwrap().installed);
        assert!(
            launcher
                .instance(&downloading.id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
    }

    #[test]
    fn instance_metadata_updates_still_reject_a_running_game() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let running = launcher.create_instance("Running", "1.21.1").unwrap();
        let other = launcher.create_instance("Other", "1.21.1").unwrap();
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(running.id.clone(), CancellationToken::new());
        assert!(
            launcher
                .update_instance(&running.id, "Changed", 4096, None, None)
                .is_err()
        );
        launcher
            .update_instance(&other.id, "Other changed", 4096, None, None)
            .unwrap();
        assert_eq!(launcher.instance(&running.id).unwrap().name, "Running");
        assert_eq!(launcher.instance(&other.id).unwrap().name, "Other changed");
    }

    #[test]
    fn settings_can_be_saved_during_download_without_changing_its_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Downloading", "1.21.1").unwrap();
        let operation = launcher.0.operation.try_lock().unwrap();
        let active_download =
            download::Downloader::new(launcher.root(), launcher.settings()).unwrap();
        let url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
        launcher
            .set_installed(&instance.id, "1.21.1", false)
            .unwrap();
        let preferences = Settings {
            language: "en-US".into(),
            apply_language_to_game: true,
            download_source: "bmclapi".into(),
            concurrency: 3,
            retries: 4,
            ..launcher.settings()
        };
        launcher.save_settings(preferences).unwrap();
        // The active operation keeps its original source; the next uses the saved source.
        assert_eq!(
            active_download.urls(url),
            vec![
                url,
                "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
            ]
        );
        let next_download =
            download::Downloader::new(launcher.root(), launcher.settings()).unwrap();
        assert!(next_download.urls(url)[0].starts_with("https://bmclapi2.bangbang93.com/"));
        launcher
            .set_installed(&instance.id, "1.21.1", true)
            .unwrap();
        let invalid = Settings {
            concurrency: 0,
            ..launcher.settings()
        };
        assert!(launcher.save_settings(invalid).is_err());
        drop(operation);
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(launcher.settings().language, "en-US");
        assert!(launcher.settings().apply_language_to_game);
        assert_eq!(launcher.settings().concurrency, 3);
        assert_eq!(launcher.settings().retries, 4);
        assert!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
    }
    #[test]
    fn maximum_download_concurrency_persists_and_rejects_out_of_range_values() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        launcher
            .save_settings(Settings {
                concurrency: 256,
                ..launcher.settings()
            })
            .unwrap();
        for invalid in [0, 257] {
            assert!(
                launcher
                    .save_settings(Settings {
                        concurrency: invalid,
                        ..launcher.settings()
                    })
                    .is_err()
            );
            assert_eq!(launcher.settings().concurrency, 256);
        }
        drop(launcher);
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(launcher.settings().concurrency, 256);
    }

    #[test]
    fn language_preferences_persist_and_invalid_changes_are_not_saved() {
        let temp = tempfile::tempdir().unwrap();
        for preference in ["en-US", "zh-CN", "system"] {
            let launcher = Launcher::open(Some(temp.path().into())).unwrap();
            let settings = Settings {
                language: preference.into(),
                apply_language_to_game: true,
                ..launcher.settings()
            };
            launcher.save_settings(settings).unwrap();
            drop(launcher);
            let launcher = Launcher::open(Some(temp.path().into())).unwrap();
            assert_eq!(launcher.settings().language, preference);
            assert!(launcher.settings().apply_language_to_game);
            let invalid = Settings {
                language: "invalid".into(),
                ..launcher.settings()
            };
            assert!(launcher.save_settings(invalid).is_err());
            assert_eq!(launcher.settings().language, preference);
            assert!(!launcher.snapshot().system_locale.is_empty());
        }
    }
    #[test]
    fn instance_order_persists_without_affecting_running_games_or_directories() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let a = launcher.create_instance("A", "1.21.1").unwrap();
        let b = launcher.create_instance("B", "1.20.1").unwrap();
        let c = launcher.create_instance("C", "").unwrap();
        let directory = launcher.game_dir(&a.id).unwrap();
        fs::write(directory.join("keep.txt"), "saved world").unwrap();
        let running = CancellationToken::new();
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(a.id.clone(), running.clone());
        let operation = launcher.0.operation.try_lock().unwrap();
        launcher.move_instance(&c.id, Some(&a.id)).unwrap();
        launcher.move_instance(&a.id, None).unwrap();
        launcher.move_instance(&a.id, Some(&a.id)).unwrap();
        assert!(!running.is_cancelled());
        assert_eq!(launcher.snapshot().running, std::slice::from_ref(&a.id));
        assert_eq!(
            launcher.instance(&a.id).unwrap().selected_version,
            a.selected_version
        );
        assert_eq!(launcher.game_dir(&a.id).unwrap(), directory);
        assert_eq!(
            fs::read_to_string(directory.join("keep.txt")).unwrap(),
            "saved world"
        );
        drop(operation);
        drop(launcher);
        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        let order: Vec<_> = reopened
            .snapshot()
            .database
            .instances
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(order, [c.id, b.id, a.id]);
    }

    #[test]
    fn stale_instance_reorder_does_not_drop_or_duplicate_entries() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let a = launcher.create_instance("A", "").unwrap();
        let b = launcher.create_instance("B", "").unwrap();
        let database = temp.path().join("database.json");
        let original = fs::read(&database).unwrap();
        assert!(launcher.move_instance("missing", Some(&a.id)).is_err());
        assert!(launcher.move_instance(&a.id, Some("missing")).is_err());
        assert_eq!(fs::read(database).unwrap(), original);
        let order: Vec<_> = launcher
            .snapshot()
            .database
            .instances
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(order, [a.id, b.id]);
    }

    #[cfg(unix)]
    #[test]
    fn directory_lock_follows_launcher_lifetime_even_with_inherited_descriptor() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let owner = launcher.clone();
        // fork() duplicates the open file description until the child execs.
        let inherited = launcher.0._lock.try_clone().unwrap();
        assert!(Launcher::open(Some(root.clone())).is_err());
        drop(launcher);
        assert!(Launcher::open(Some(root.clone())).is_err());
        drop(owner);

        let reopened = Launcher::open(Some(root.clone())).unwrap();
        drop(inherited);
        assert!(Launcher::open(Some(root.clone())).is_err());
        drop(reopened);
        assert!(Launcher::open(Some(root)).is_ok());
    }

    #[tokio::test]
    async fn instance_identity_edits_persist_without_changing_the_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("原名称", "").unwrap();
        let directory = launcher.game_dir(&instance.id).unwrap();
        assert!(instance.icon.is_some_and(|icon| icon < INSTANCE_ICON_COUNT));
        launcher
            .update_instance(&instance.id, "  新名称  ", 4096, None, Some(2))
            .unwrap();
        let updated = launcher.instance(&instance.id).unwrap();
        assert_eq!(updated.name, "新名称");
        assert_eq!(updated.icon, Some(2));
        assert_eq!(launcher.game_dir(&instance.id).unwrap(), directory);

        // Every new material is accepted, including the last catalog entry.
        for icon in 4..INSTANCE_ICON_COUNT {
            launcher
                .update_instance(&instance.id, "新名称", 2048, None, Some(icon))
                .unwrap();
            assert_eq!(launcher.instance(&instance.id).unwrap().icon, Some(icon));
        }
        let selected_icon = Some(INSTANCE_ICON_COUNT - 1);

        // Missing icon fields from older clients must not reset the saved choice.
        launcher
            .update_instance(&instance.id, "新名称", 2048, None, None)
            .unwrap();
        assert_eq!(launcher.instance(&instance.id).unwrap().icon, selected_icon);
        assert!(
            launcher
                .update_instance(
                    &instance.id,
                    "不应保存",
                    8192,
                    None,
                    Some(INSTANCE_ICON_COUNT)
                )
                .is_err()
        );
        let unchanged = launcher.instance(&instance.id).unwrap();
        assert_eq!(unchanged.name, "新名称");
        assert_eq!(unchanged.memory_mb, 2048);
        assert_eq!(unchanged.icon, selected_icon);
        let cloned = launcher.clone_instance(&instance.id).await.unwrap();
        assert_eq!(cloned.icon, selected_icon);
        drop(launcher);

        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(reopened.instance(&instance.id).unwrap().icon, selected_icon);
        assert_eq!(reopened.instance(&instance.id).unwrap().name, "新名称");
        assert_eq!(reopened.game_dir(&instance.id).unwrap(), directory);
        drop(reopened);

        // Existing schema-2 databases contain no icon field.
        let file = temp.path().join("database.json");
        let mut legacy: serde_json::Value = storage::read_json(&file).unwrap();
        legacy["instances"][0]
            .as_object_mut()
            .unwrap()
            .remove("icon");
        storage::write_json(&file, &legacy).unwrap();
        let legacy_launcher = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(legacy_launcher.instance(&instance.id).unwrap().icon, None);
        assert_eq!(
            legacy_launcher.instance(&instance.id).unwrap().name,
            "新名称"
        );
    }

    #[tokio::test]
    async fn repeated_authenticated_launches_reuse_memory_without_persisting_tokens() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = Account {
            id: "account-fixture".into(),
            username: "Fixture".into(),
            client_id: "client-fixture".into(),
            expires_at: storage::now() + 3600,
            owned: true,
            skin: None,
        };
        launcher.save_account(account.clone(), true).unwrap();
        launcher.0.sessions.remember(auth::MinecraftSession {
            account,
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: std::time::Instant::now() + std::time::Duration::from_secs(300),
        });
        let sink: EventSink = Arc::new(|_| {});
        for _ in 0..3 {
            // There is deliberately no Keychain item or authentication server.
            let session = launcher.launch_session(&sink).await.unwrap();
            assert_eq!(session.access_token, "access-fixture");
            assert_eq!(session.username, "Fixture");
        }
        for public in [
            serde_json::to_string(&launcher.snapshot()).unwrap(),
            fs::read_to_string(temp.path().join("database.json")).unwrap(),
        ] {
            assert!(!public.contains("access-fixture"));
            assert!(!public.contains("refresh-fixture"));
        }
    }

    #[tokio::test]
    async fn guest_launch_identity_is_stable_and_never_saved_as_an_account() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let sink: EventSink = Arc::new(|_| {});
        let first = launcher.launch_session(&sink).await.unwrap();
        let second = launcher.launch_session(&sink).await.unwrap();
        assert_eq!(first.username, "Player");
        assert_eq!(first.uuid, "a01e3843e5213998958af459800e4d11");
        assert_eq!(first.uuid, second.uuid);
        assert_eq!(first.user_type, "legacy");
        assert_eq!(first.access_token, "0");
        assert!(first.log_secrets.is_empty());
        assert!(launcher.snapshot().database.accounts.is_empty());
        assert!(launcher.snapshot().database.selected_account.is_none());

        launcher
            .mutate(|db| {
                db.selected_account = Some("missing-account".into());
                Ok(())
            })
            .unwrap();
        let error = launcher.launch_session(&sink).await.err().unwrap();
        assert!(error.to_string().contains("所选账户不存在"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn no_account_launch_reaches_game_process_and_keeps_guest_logs_intact() {
        verify_guest_launch_directory(false).await;
        verify_guest_launch_directory(true).await;
    }

    #[tokio::test]
    async fn guest_can_be_selected_and_renamed_without_losing_microsoft_accounts() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = Account {
            id: "account-fixture".into(),
            username: "Fixture".into(),
            client_id: "client-fixture".into(),
            expires_at: storage::now() + 3600,
            owned: true,
            skin: None,
        };
        launcher.save_account(account.clone(), true).unwrap();
        launcher.0.sessions.remember(auth::MinecraftSession {
            account: account.clone(),
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: std::time::Instant::now() + std::time::Duration::from_secs(300),
        });
        let sink: EventSink = Arc::new(|_| {});
        assert_eq!(launcher.set_guest_name("  Neko_42  ").unwrap(), "Neko_42");
        // Editing a guest's name alone never changes the selected identity.
        assert_eq!(
            launcher.launch_session(&sink).await.unwrap().username,
            "Fixture"
        );
        launcher.select_guest().unwrap();
        let guest = launcher.launch_session(&sink).await.unwrap();
        assert_eq!(guest.username, "Neko_42");
        assert_eq!(guest.uuid, minecraft::LaunchSession::guest().uuid);
        assert_eq!(guest.access_token, "0");
        assert!(guest.log_secrets.is_empty());
        assert_eq!(launcher.snapshot().database.accounts.len(), 1);
        for invalid in [
            "",
            "  ",
            "猫咪",
            "Neko Cat",
            "bad/name",
            "abcdefghijklmnopq",
        ] {
            assert!(launcher.set_guest_name(invalid).is_err());
            assert_eq!(launcher.snapshot().database.guest_name, "Neko_42");
        }
        assert!(launcher.set_guest_name("1234567890123456").is_ok());
        launcher.set_guest_name("Neko_42").unwrap();
        launcher.select_account(&account.id).unwrap();
        assert_eq!(
            launcher.launch_session(&sink).await.unwrap().access_token,
            "access-fixture"
        );
        launcher.select_guest().unwrap();
        drop(launcher);

        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        assert!(reopened.snapshot().database.selected_account.is_none());
        assert_eq!(reopened.snapshot().database.accounts[0].id, account.id);
        assert_eq!(
            reopened.launch_session(&sink).await.unwrap().username,
            "Neko_42"
        );
        // Existing databases keep their previous default Player identity.
        let mut legacy = serde_json::to_value(&reopened.snapshot().database).unwrap();
        legacy.as_object_mut().unwrap().remove("guest_name");
        let legacy: Database = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.guest_name, "Player");
    }

    #[cfg(unix)]
    async fn verify_guest_launch_directory(linked: bool) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        if linked {
            launcher
                .save_account(
                    Account {
                        id: "account-without-tokens".into(),
                        username: "MicrosoftPlayer".into(),
                        client_id: "client-fixture".into(),
                        expires_at: 0,
                        owned: true,
                        skin: None,
                    },
                    true,
                )
                .unwrap();
            launcher.set_guest_name("Local_Neko").unwrap();
            launcher.select_guest().unwrap();
        }
        let java = temp.path().join("java fixture");
        fs::write(
            &java,
            r#"#!/bin/sh
if [ "$2" = "-version" ]; then
  printf '%s\n' 'java version "1.8.0_fixture"' >&2
  exit 0
fi
printf '%s\n' "$@" > launch-arguments.txt
printf '%s\n' 'offline session 0 started'
"#,
        )
        .unwrap();
        fs::set_permissions(&java, fs::Permissions::from_mode(0o700)).unwrap();
        let source = temp.path().join("已有 游戏/.minecraft");
        let instance = if linked {
            fs::create_dir_all(source.join("saves")).unwrap();
            launcher
                .import_instance(source.clone(), "原目录启动".into(), "1.21.1".into())
                .await
                .unwrap()
        } else {
            launcher.create_instance("未登录启动", "1.21.1").unwrap()
        };
        launcher
            .update_instance(
                &instance.id,
                &instance.name,
                1024,
                Some(java.to_string_lossy().into()),
                None,
            )
            .unwrap();
        let version = test_http::version_fixture("1.21.1");
        let game = launcher.game_dir(&instance.id).unwrap();
        if linked {
            assert_eq!(game, source.canonicalize().unwrap());
        }
        let version_dir = game.join("versions/1.21.1");
        fs::create_dir_all(&version_dir).unwrap();
        storage::write_json(&version_dir.join("1.21.1.json"), &version).unwrap();
        fs::write(version_dir.join("1.21.1.jar"), b"fixture client").unwrap();
        launcher
            .mutate(|db| {
                db.instances[0].versions[0].installed = true;
                Ok(())
            })
            .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        let pid = launcher
            .launch(&instance.id, &CancellationToken::new(), sink)
            .await
            .unwrap();
        assert!(pid > 0);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !launcher.snapshot().running.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let text = fs::read_to_string(game.join("launch-arguments.txt")).unwrap();
        let args: Vec<_> = text.lines().collect();
        let game_at = args.iter().position(|arg| *arg == "--gameDir").unwrap();
        assert_eq!(args[game_at + 1], game.to_string_lossy());
        if linked {
            assert!(
                !launcher
                    .instance_dir(&instance.id)
                    .unwrap()
                    .join(".minecraft")
                    .exists()
            );
        }
        for (key, value) in [
            ("--username", if linked { "Local_Neko" } else { "Player" }),
            ("--uuid", "a01e3843e5213998958af459800e4d11"),
            ("--accessToken", "0"),
            ("--userType", "legacy"),
            ("--clientId", ""),
            ("--xuid", ""),
        ] {
            let at = args.iter().position(|arg| *arg == key).unwrap();
            assert_eq!(args[at + 1], value);
        }
        assert!(!text.contains("${"));
        let log = fs::read_dir(
            launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join("launcher-logs"),
        )
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
        assert!(
            fs::read_to_string(log)
                .unwrap()
                .contains("offline session 0 started")
        );
        assert_eq!(
            launcher.snapshot().database.accounts.len(),
            usize::from(linked)
        );
        assert!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .last_played
                .is_some()
        );
    }

    #[tokio::test]
    async fn process_logs_redact_tokens_and_bound_long_lines() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("game.log");
        let file = Arc::new(tokio::sync::Mutex::new(
            tokio::fs::File::create(&path).await.unwrap(),
        ));
        let sink: EventSink = Arc::new(|_| {});
        let input = format!("token=secret-token\n{}\nfinished\n", "x".repeat(100000));
        pipe_logs(
            input.as_bytes(),
            "fixture".into(),
            sink,
            file.clone(),
            Arc::new(vec!["secret-token".into()]),
        )
        .await;
        file.lock().await.flush().await.unwrap();
        let text = tokio::fs::read_to_string(path).await.unwrap();
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("secret-token"));
        assert!(text.contains("超长日志行"));
        assert!(text.contains("finished"));
        assert!(text.len() < 200);
    }

    #[tokio::test]
    async fn imported_directory_stays_in_place_and_explicit_clone_excludes_credentials() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("existing/.minecraft");
        fs::create_dir_all(source.join("saves/world")).unwrap();
        fs::write(source.join("saves/world/level.dat"), b"save-fixture").unwrap();
        fs::write(source.join("launcher_accounts.json"), b"private-fixture").unwrap();
        let launcher = Launcher::open(Some(t.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(
                t.path().join("existing"),
                "迁移测试".into(),
                "1.21.1".into(),
            )
            .await
            .unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        assert_eq!(game, source.canonicalize().unwrap());
        assert!(
            !launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"save-fixture"
        );
        assert_eq!(
            fs::read(game.join("launcher_accounts.json")).unwrap(),
            b"private-fixture"
        );
        assert!(source.join("launcher_accounts.json").exists());
        assert!(!instance.selected().unwrap().installed);
        fs::write(source.join("saves/world/level.dat"), b"updated-in-place").unwrap();
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"updated-in-place"
        );
        let cloned = launcher.clone_instance(&instance.id).await.unwrap();
        assert_eq!(cloned.game_directory, None);
        let clone_game = launcher.game_dir(&cloned.id).unwrap();
        assert_ne!(clone_game, game);
        assert_eq!(
            fs::read(clone_game.join("saves/world/level.dat")).unwrap(),
            b"updated-in-place"
        );
        assert!(!clone_game.join("launcher_accounts.json").exists());
        drop(launcher);
        let reopened = Launcher::open(Some(t.path().join("launcher"))).unwrap();
        assert_eq!(reopened.game_dir(&instance.id).unwrap(), game);
        assert_eq!(
            reopened.instance(&instance.id).unwrap().game_directory,
            Some(game)
        );
    }

    #[test]
    fn data_lock_excludes_other_processes_and_releases_after_owner_drops() {
        const ROOT: &str = "NEKOCRATE_TEST_LOCK_ROOT";
        const EXPECT_OPEN: &str = "NEKOCRATE_TEST_LOCK_EXPECT_OPEN";
        if let Some(root) = std::env::var_os(ROOT) {
            let result = Launcher::open(Some(root.into()));
            let expected = std::env::var(EXPECT_OPEN).unwrap() == "true";
            assert_eq!(result.is_ok(), expected);
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let attempt = |expected: bool| {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::data_lock_excludes_other_processes_and_releases_after_owner_drops",
                    "--nocapture",
                ])
                .env(ROOT, directory.path())
                .env(EXPECT_OPEN, expected.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child lock check failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        };
        let owner = Launcher::open(Some(directory.path().into())).unwrap();
        let shared = owner.clone();
        attempt(false);
        drop(owner);
        attempt(false); // A clone still owns the same database.
        drop(shared);
        attempt(true);
    }

    #[test]
    fn update_restart_hands_off_the_data_lock_before_old_process_exits() {
        let dir = tempfile::tempdir().unwrap();
        let old = Launcher::open(Some(dir.path().into())).unwrap();
        assert!(Launcher::open(Some(dir.path().into())).is_err());
        old.release_data_lock_for_restart().unwrap();
        let replacement = Launcher::open(Some(dir.path().into())).unwrap();
        drop(old);
        // Dropping the old launcher must not unlock the replacement's descriptor.
        assert!(Launcher::open(Some(dir.path().into())).is_err());
        drop(replacement);
        assert!(Launcher::open(Some(dir.path().into())).is_ok());
    }

    #[tokio::test]
    async fn instance_lifecycle_isolated_and_recoverable() {
        let t = tempfile::tempdir().unwrap();
        let l = Launcher::open(Some(t.path().into())).unwrap();
        let a = l.create_instance("世界 A", "1.21.1").unwrap();
        fs::write(
            l.instance_dir(&a.id)
                .unwrap()
                .join(".minecraft/options.txt"),
            b"volume:1",
        )
        .unwrap();
        let b = l.clone_instance(&a.id).await.unwrap();
        assert_ne!(a.id, b.id);
        fs::write(
            l.instance_dir(&b.id)
                .unwrap()
                .join(".minecraft/options.txt"),
            b"volume:0",
        )
        .unwrap();
        assert_eq!(
            fs::read(
                l.instance_dir(&a.id)
                    .unwrap()
                    .join(".minecraft/options.txt")
            )
            .unwrap(),
            b"volume:1"
        );
        let recovered = t.path().join("recycled-instance");
        l.delete_instance_with_recycle(&b.id, |source| {
            fs::rename(source, &recovered)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(l.snapshot().database.instances.len(), 1);
        assert_eq!(
            fs::read(recovered.join(".minecraft/options.txt")).unwrap(),
            b"volume:0"
        );
        assert!(!t.path().join("trash").exists());
        assert!(l.instance_dir("../outside").is_err());
        assert!(Launcher::open(Some(t.path().into())).is_err());
    }
    #[tokio::test]
    async fn multiple_versions_share_one_game_directory_and_keep_separate_states() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.create_instance("日常游戏", "").unwrap();
        assert!(instance.versions.is_empty());
        let game = launcher.game_dir(&instance.id).unwrap();
        fs::write(game.join("options.txt"), b"shared-options").unwrap();
        launcher.add_version(&instance.id, "1.21.1").unwrap();
        launcher
            .set_installed(&instance.id, "1.21.1", true)
            .unwrap();
        launcher.add_version(&instance.id, "1.20.1").unwrap();
        launcher.add_version(&instance.id, "1.20.1").unwrap();
        let updated = launcher.instance(&instance.id).unwrap();
        assert_eq!(updated.versions.len(), 2);
        assert!(!updated.selected().unwrap().installed);
        launcher.select_version(&instance.id, "1.21.1").unwrap();
        assert!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
        assert_eq!(launcher.game_dir(&instance.id).unwrap(), game);
        assert!(launcher.select_version(&instance.id, "missing").is_err());
        assert!(launcher.add_version(&instance.id, "../escape").is_err());
        let cloned = launcher.clone_instance(&instance.id).await.unwrap();
        assert_eq!(cloned.versions.len(), 2);
        assert!(cloned.selected().unwrap().installed);
        assert_ne!(launcher.game_dir(&cloned.id).unwrap(), game);
        assert_eq!(
            fs::read(launcher.game_dir(&cloned.id).unwrap().join("options.txt")).unwrap(),
            b"shared-options"
        );
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(instance.id.clone(), CancellationToken::new());
        assert!(launcher.select_version(&instance.id, "1.20.1").is_err());
        assert!(launcher.add_version(&instance.id, "1.21").is_err());
        launcher.0.games.lock().unwrap().clear();
        drop(launcher);
        let reopened = Launcher::open(Some(root)).unwrap();
        let persisted = reopened.instance(&instance.id).unwrap();
        assert_eq!(persisted.versions.len(), 2);
        assert_eq!(persisted.selected_version.as_deref(), Some("1.21.1"));
        assert!(persisted.selected().unwrap().installed);
    }

    #[tokio::test]
    async fn relink_preserves_identity_and_games_and_rejects_missing_installed_versions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let source = temp.path().join("original/.minecraft");
        fs::create_dir_all(source.join("saves/world")).unwrap();
        fs::write(source.join("saves/world/level.dat"), b"original-world").unwrap();
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.create_instance("Existing", "1.21.1").unwrap();
        launcher
            .update_instance(
                &instance.id,
                "My game",
                4096,
                Some("my-java".into()),
                Some(8),
            )
            .unwrap();
        let old_game = launcher.game_dir(&instance.id).unwrap();
        let relative = PathBuf::from("versions/1.21.1");
        let metadata = crate::test_http::version_fixture("1.21.1");
        storage::write_json(&old_game.join(&relative).join("1.21.1.json"), &metadata).unwrap();
        fs::write(old_game.join(&relative).join("1.21.1.jar"), b"client").unwrap();
        fs::write(old_game.join("keep-me"), b"old-copy").unwrap();
        launcher
            .set_installed(&instance.id, "1.21.1", true)
            .unwrap();
        let before = fs::read(root.join("database.json")).unwrap();
        assert!(
            launcher
                .relink_instance(&instance.id, source.clone())
                .await
                .is_err()
        );
        assert_eq!(fs::read(root.join("database.json")).unwrap(), before);
        storage::write_json(&source.join(&relative).join("1.21.1.json"), &metadata).unwrap();
        fs::write(source.join(&relative).join("1.21.1.jar"), b"other!").unwrap();
        assert!(
            launcher
                .relink_instance(&instance.id, source.clone())
                .await
                .is_err()
        );
        assert_eq!(fs::read(root.join("database.json")).unwrap(), before);
        fs::write(source.join(&relative).join("1.21.1.jar"), b"client").unwrap();
        let linked = launcher
            .relink_instance(&instance.id, source.clone())
            .await
            .unwrap();
        assert_eq!(linked.id, instance.id);
        assert_eq!(linked.name, "My game");
        assert_eq!(linked.memory_mb, 4096);
        assert_eq!(linked.java_path.as_deref(), Some("my-java"));
        assert_eq!(linked.icon, Some(8));
        assert_eq!(linked.selected_version, instance.selected_version);
        assert!(linked.selected().unwrap().installed);
        assert_eq!(linked.game_directory, Some(source.canonicalize().unwrap()));
        assert_eq!(fs::read(old_game.join("keep-me")).unwrap(), b"old-copy");
        assert!(!source.join("keep-me").exists());
        assert_eq!(
            fs::read(source.join("saves/world/level.dat")).unwrap(),
            b"original-world"
        );
        let backup: Instance = storage::read_json(
            &launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join("directory-link-before.json"),
        )
        .unwrap();
        assert_eq!(backup.game_directory, None);
        let second = launcher.create_instance("Other", "").unwrap();
        assert!(
            launcher
                .relink_instance(&second.id, source.clone())
                .await
                .is_err()
        );
        drop(launcher);
        let reopened = Launcher::open(Some(root)).unwrap();
        assert_eq!(
            reopened.game_dir(&instance.id).unwrap(),
            source.canonicalize().unwrap()
        );
        assert!(
            reopened
                .instance(&instance.id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
    }

    #[tokio::test]
    async fn import_discovers_multiple_versions_in_one_minecraft_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("existing/.minecraft");
        for id in ["1.21.1", "1.20.1"] {
            let metadata = crate::test_http::version_fixture(id);
            storage::write_json(
                &source.join("versions").join(id).join(format!("{id}.json")),
                &metadata,
            )
            .unwrap();
        }
        fs::create_dir_all(source.join("saves/world")).unwrap();
        fs::write(source.join("saves/world/level.dat"), b"shared-save").unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(source.clone(), "导入目录".into(), String::new())
            .await
            .unwrap();
        assert_eq!(instance.versions.len(), 2);
        assert!(instance.versions.iter().all(|v| !v.installed));
        assert_eq!(
            fs::read(
                launcher
                    .game_dir(&instance.id)
                    .unwrap()
                    .join("saves/world/level.dat")
            )
            .unwrap(),
            b"shared-save"
        );
        assert!(source.join("saves/world/level.dat").exists());
    }

    #[tokio::test]
    async fn imports_tilde_hidden_directory_without_modifying_source() {
        let home = directories::UserDirs::new().unwrap();
        let temp = tempfile::Builder::new()
            .prefix(".nekocrate-import-test-")
            .tempdir_in(home.home_dir())
            .unwrap();
        let source = temp.path().join("game/minecraft/.minecraft");
        let metadata = crate::test_http::version_fixture("1.21.1");
        storage::write_json(&source.join("versions/1.21.1/1.21.1.json"), &metadata).unwrap();
        fs::create_dir_all(source.join("saves/world")).unwrap();
        fs::write(source.join("saves/world/level.dat"), b"original-world").unwrap();
        // An unrelated nested folder must not displace the selected game directory.
        fs::create_dir_all(source.join("game")).unwrap();
        fs::write(source.join("launcher_accounts.json"), b"private-fixture").unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let typed_path = PathBuf::from("~").join(source.strip_prefix(home.home_dir()).unwrap());
        let instance = launcher
            .import_instance(typed_path, "已有目录".into(), String::new())
            .await
            .unwrap();
        let imported = launcher.game_dir(&instance.id).unwrap();
        assert_eq!(instance.selected_version.as_deref(), Some("1.21.1"));
        assert_eq!(
            fs::read(imported.join("saves/world/level.dat")).unwrap(),
            b"original-world"
        );
        assert_eq!(
            fs::read(source.join("saves/world/level.dat")).unwrap(),
            b"original-world"
        );
        assert_eq!(imported, source.canonicalize().unwrap());
        assert_eq!(
            fs::read(imported.join("launcher_accounts.json")).unwrap(),
            b"private-fixture"
        );
        assert!(
            !launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
    }

    #[tokio::test]
    async fn linked_directories_reject_duplicates_and_overlapping_paths() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("existing/.minecraft");
        fs::create_dir_all(source.join("saves")).unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        launcher
            .import_instance(source.clone(), "原目录".into(), String::new())
            .await
            .unwrap();
        for duplicate in [source.clone(), source.parent().unwrap().to_owned()] {
            let error = launcher
                .import_instance(duplicate, "重复".into(), String::new())
                .await
                .unwrap_err();
            assert!(error.to_string().contains("已由实例"));
        }
        #[cfg(unix)]
        {
            let alias = temp.path().join("linked-alias");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            assert!(
                launcher
                    .import_instance(alias, "别名".into(), String::new())
                    .await
                    .is_err()
            );
        }
        let nested = source.join("nested/.minecraft");
        fs::create_dir_all(nested.join("saves")).unwrap();
        assert!(
            launcher
                .import_instance(nested, "嵌套".into(), String::new())
                .await
                .is_err()
        );
        let managed = launcher.create_instance("托管目录", "").unwrap();
        let managed_dir = launcher.game_dir(&managed.id).unwrap();
        fs::create_dir_all(managed_dir.join("saves")).unwrap();
        assert!(
            launcher
                .import_instance(managed_dir, "自身".into(), String::new())
                .await
                .is_err()
        );
        assert_eq!(launcher.snapshot().database.instances.len(), 2);
    }

    #[tokio::test]
    async fn removing_linked_instance_preserves_files_and_missing_source_is_not_recreated() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("external/.minecraft");
        fs::create_dir_all(source.join("saves/world")).unwrap();
        fs::write(source.join("saves/world/level.dat"), b"precious-world").unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let first = launcher
            .import_instance(source.clone(), "原目录".into(), "1.21.1".into())
            .await
            .unwrap();
        launcher
            .delete_instance_with_recycle(&first.id, crate::recycle::discard_test_fixture)
            .unwrap();
        assert_eq!(
            fs::read(source.join("saves/world/level.dat")).unwrap(),
            b"precious-world"
        );
        let instance = launcher
            .import_instance(source.clone(), "重新关联".into(), "1.21.1".into())
            .await
            .unwrap();
        let offline = temp.path().join("unmounted");
        fs::rename(&source, &offline).unwrap();
        let cancel = CancellationToken::new();
        let sink: EventSink = Arc::new(|_| {});
        let error = launcher
            .install(&instance.id, &cancel, &sink)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("关联的游戏目录不可用"));
        let error = launcher
            .launch(&instance.id, &cancel, sink)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("关联的游戏目录不可用"));
        assert!(launcher.clone_instance(&instance.id).await.is_err());
        assert_eq!(launcher.snapshot().database.instances.len(), 1);
        assert!(!source.exists());
        launcher
            .delete_instance_with_recycle(&instance.id, crate::recycle::discard_test_fixture)
            .unwrap();
        assert_eq!(
            fs::read(offline.join("saves/world/level.dat")).unwrap(),
            b"precious-world"
        );
        assert!(!source.exists());
        assert!(launcher.snapshot().database.instances.is_empty());
    }

    #[tokio::test]
    async fn invalid_import_paths_do_not_create_an_instance() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let missing = launcher
            .import_instance(
                temp.path().join("missing/.minecraft"),
                "missing".into(),
                String::new(),
            )
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("无法访问导入目录"));
        fs::write(temp.path().join("file"), b"not a directory").unwrap();
        let file = launcher
            .import_instance(temp.path().join("file"), "file".into(), String::new())
            .await
            .unwrap_err();
        assert!(file.to_string().contains("必须是一个目录"));
        assert!(launcher.snapshot().database.instances.is_empty());
    }
}
