//! Global indexes reference existing files. Only explicitly owned storage is removable.
use crate::{Launcher, java, model::EventSink, storage};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

fn identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}
fn size(path: &Path) -> Result<(u64, usize)> {
    let mut bytes = 0;
    let mut files = 0;
    if !path.try_exists()? {
        return Ok((0, 0));
    }
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            bytes += entry.metadata()?.len();
            files += 1;
        }
    }
    Ok((bytes, files))
}
/// Reject symlinked managed roots, including intermediate directories.
fn owned(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "非法托管路径"
    );
    let mut result = root.to_owned();
    for component in relative.components() {
        result.push(component);
        match fs::symlink_metadata(&result) {
            Ok(meta) => ensure!(!meta.file_type().is_symlink(), "托管目录不能是符号链接"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(result)
}
pub(crate) fn registered_java(root: &Path) -> Vec<PathBuf> {
    fs::read(root.join("java-paths.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
#[derive(Serialize)]
pub struct Runtime {
    #[serde(flatten)]
    pub java: java::JavaInfo,
    pub source: &'static str,
    pub size: u64,
    pub used_by: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct Owner {
    pub id: String,
    pub version: String,
    pub label: String,
}
#[derive(Serialize)]
pub struct ResourceGroup {
    pub directory: PathBuf,
    pub owners: Vec<Owner>,
}
#[derive(Serialize)]
pub struct ResourceIndex {
    pub groups: Vec<ResourceGroup>,
    pub issues: Vec<String>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheKind {
    Metadata,
    Resources,
    Partials,
}
impl CacheKind {
    fn directories(self) -> &'static [&'static str] {
        match self {
            Self::Metadata => &["metadata"],
            Self::Resources => &["runtimes", "cache/skins", "skin-library"],
            Self::Partials => &["cache/objects", "cache/java"],
        }
    }
}
#[derive(Serialize)]
pub struct CacheEntry {
    pub kind: CacheKind,
    pub size: u64,
    pub files: usize,
    pub parts: Vec<CachePart>,
}
#[derive(Serialize)]
pub struct CachePart {
    pub kind: &'static str,
    pub size: u64,
    pub files: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SavedSkin {
    pub id: String,
    pub name: String,
    pub slim: bool,
    // The official service may re-encode a PNG. Keep its returned texture URL
    // to attach account usage to the original imported card after restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_url: Option<String>,
}

impl Launcher {
    pub(crate) fn resource_directory_idle(&self, id: &str, version: &str) -> Result<()> {
        self.idle(id)?;
        let running = self
            .0
            .games
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if running.is_empty() {
            return Ok(());
        }
        let (working, _) = self.content_location(id, version)?;
        let working = identity(&working);
        for running_id in running {
            let instance = self.instance(&running_id)?;
            for version in &instance.versions {
                let (other, _) = self.content_location(&running_id, &version.id)?;
                ensure!(
                    identity(&other) != working,
                    "共享此资源目录的游戏正在运行，请先关闭游戏"
                );
            }
        }
        Ok(())
    }

    fn runtime_users(&self, java_path: &Path) -> Vec<String> {
        let home = self.runtime_home(java_path);
        let java_path = identity(java_path);
        let matches = |path: &str| {
            identity(Path::new(path)) == java_path
                || home
                    .as_ref()
                    .is_some_and(|home| self.runtime_home(Path::new(path)).as_ref() == Some(home))
        };
        let mut users = vec![];
        for instance in self.snapshot().database.instances {
            if instance.java_path.as_ref().is_some_and(|p| matches(p)) {
                users.push(instance.name.clone());
            }
            for version in &instance.versions {
                let file = instance
                    .game_dir(self.root())
                    .join("versions")
                    .join(&version.id)
                    .join("runtime.json");
                let value: serde_json::Value = fs::read(file)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default();
                if value["path"].as_str().is_some_and(&matches) {
                    users.push(format!(
                        "{} · {}",
                        instance.name,
                        version.name.as_deref().unwrap_or(&version.id)
                    ));
                }
            }
        }
        users.sort();
        users.dedup();
        users
    }
    fn runtime_home(&self, path: &Path) -> Option<PathBuf> {
        let root = owned(self.root(), Path::new("runtimes")).ok()?;
        let canonical_root = root.canonicalize().ok()?;
        let path = path.canonicalize().ok()?;
        let first = path
            .strip_prefix(&canonical_root)
            .ok()?
            .components()
            .next()?;
        let home = root.join(first);
        if home.file_name()?.to_string_lossy().starts_with('.') {
            return None;
        }
        let meta = fs::symlink_metadata(&home).ok()?;
        (meta.is_dir() && !meta.file_type().is_symlink()).then_some(home)
    }
    pub async fn resource_java(&self) -> Result<Vec<Runtime>> {
        let mut infos = java::discover(self.root()).await;
        for instance in self.snapshot().database.instances {
            if let Some(path) = instance.java_path.filter(|path| !path.is_empty())
                && !infos
                    .iter()
                    .any(|info| identity(Path::new(&info.path)) == identity(Path::new(&path)))
                && let Ok(info) = java::inspect(Path::new(&path)).await
            {
                infos.push(info);
            }
        }
        let launcher = self.clone();
        tokio::task::spawn_blocking(move || launcher.runtime_inventory(infos)).await?
    }
    fn runtime_inventory(&self, infos: Vec<java::JavaInfo>) -> Result<Vec<Runtime>> {
        let registered = registered_java(self.root());
        let configured = self
            .snapshot()
            .database
            .instances
            .into_iter()
            .filter_map(|i| i.java_path)
            .map(|p| identity(Path::new(&p)))
            .collect::<Vec<_>>();
        let mut runtimes = vec![];
        for info in infos {
            let home = self.runtime_home(Path::new(&info.path));
            let source = if home.is_some() {
                "managed"
            } else if registered
                .iter()
                .any(|p| identity(p) == identity(Path::new(&info.path)))
            {
                "registered"
            } else if configured.contains(&identity(Path::new(&info.path))) {
                "configured"
            } else {
                "system"
            };
            let used_by = self.runtime_users(Path::new(&info.path));
            runtimes.push(Runtime {
                size: home
                    .map(|p| size(&p).map(|s| s.0))
                    .transpose()?
                    .unwrap_or(0),
                source,
                used_by,
                java: info,
            });
        }
        runtimes.sort_by(|a, b| {
            b.java
                .major
                .cmp(&a.java.major)
                .then(a.java.path.cmp(&b.java.path))
        });
        Ok(runtimes)
    }
    pub async fn register_java(&self, path: &Path) -> Result<java::JavaInfo> {
        let info = java::inspect(path).await?;
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let mut paths = registered_java(self.root());
        if !paths
            .iter()
            .any(|p| identity(p) == identity(Path::new(&info.path)))
        {
            paths.push(PathBuf::from(&info.path));
        }
        storage::atomic_write(
            &self.root().join("java-paths.json"),
            &serde_json::to_vec_pretty(&paths)?,
        )?;
        Ok(info)
    }
    pub fn remove_resource_java(&self, path: &Path) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        ensure!(
            self.0.games.lock().unwrap().is_empty(),
            "请先关闭正在运行的游戏"
        );
        ensure!(
            self.runtime_users(path).is_empty(),
            "该 Java 仍被实例引用，请先切换实例的 Java 并重新启动游戏"
        );
        let mut registered = registered_java(self.root());
        let was_registered = registered.iter().any(|p| identity(p) == identity(path));
        registered.retain(|p| identity(p) != identity(path));
        if let Some(home) = self.runtime_home(path) {
            crate::recycle::move_to_trash(&home)?;
        } else {
            ensure!(was_registered, "系统 Java 只能在系统中卸载");
        }
        storage::atomic_write(
            &self.root().join("java-paths.json"),
            &serde_json::to_vec_pretty(&registered)?,
        )?;
        Ok(())
    }
    pub async fn install_resource_java(
        &self,
        major: u32,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<java::JavaInfo> {
        ensure!([8, 17, 21, 25, 26].contains(&major), "不支持的 Java 版本");
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let downloader = crate::download::Downloader::new(self.root(), self.settings())?;
        java::ensure(
            self.root(),
            java::JavaRequirement {
                major,
                arch: crate::minecraft::arch_name(),
                jdk: major >= 9,
                compatibility: None,
                bytecode_ceiling: None,
            },
            None,
            true,
            &downloader,
            cancel,
            sink,
        )
        .await
    }
    pub fn resource_groups(&self, kind: crate::version_management::ContentKind) -> ResourceIndex {
        let mut groups = BTreeMap::<PathBuf, Vec<Owner>>::new();
        let mut issues = vec![];
        for instance in self.snapshot().database.instances {
            for version in &instance.versions {
                match self.content_location(&instance.id, &version.id) {
                    Ok((working, _)) => {
                        let directory = working.join(match kind {
                            crate::version_management::ContentKind::Mods => "mods",
                            crate::version_management::ContentKind::Resourcepacks => {
                                "resourcepacks"
                            }
                            crate::version_management::ContentKind::Shaderpacks => "shaderpacks",
                            _ => "",
                        });
                        groups.entry(identity(&directory)).or_default().push(Owner {
                            id: instance.id.clone(),
                            version: version.id.clone(),
                            label: format!(
                                "{} · {}",
                                instance.name,
                                version.name.as_deref().unwrap_or(&version.id)
                            ),
                        })
                    }
                    Err(error) => {
                        issues.push(format!("{} · {}: {error:#}", instance.name, version.id))
                    }
                }
            }
        }
        ResourceIndex {
            groups: groups
                .into_iter()
                .map(|(directory, owners)| ResourceGroup { directory, owners })
                .collect(),
            issues,
        }
    }
    pub fn resource_cache(&self) -> Result<Vec<CacheEntry>> {
        let mut entries = Vec::new();
        for kind in [
            CacheKind::Metadata,
            CacheKind::Resources,
            CacheKind::Partials,
        ] {
            let paths = self.cache_files(kind)?;
            if matches!(kind, CacheKind::Partials) && paths.is_empty() {
                continue;
            }
            let mut entry = CacheEntry {
                kind,
                files: paths.len(),
                size: 0,
                parts: Vec::new(),
            };
            let mut parts = BTreeMap::<&str, (u64, usize)>::new();
            for path in paths {
                let bytes = fs::symlink_metadata(&path)?.len();
                entry.size += bytes;
                if matches!(kind, CacheKind::Resources) {
                    let key = if path.starts_with(self.root().join("runtimes")) {
                        "java"
                    } else {
                        "skins"
                    };
                    let part = parts.entry(key).or_default();
                    part.0 += bytes;
                    part.1 += 1;
                }
            }
            entry.parts = parts
                .into_iter()
                .map(|(kind, (size, files))| CachePart { kind, size, files })
                .collect();
            entries.push(entry);
        }
        Ok(entries)
    }
    fn cache_files(&self, kind: CacheKind) -> Result<Vec<PathBuf>> {
        let mut files = vec![];
        for directory in kind.directories() {
            let root = owned(self.root(), Path::new(directory))?;
            if !root.try_exists()? {
                continue;
            }
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    // Extraction directories are not installed Java runtimes.
                    !(*directory == "runtimes"
                        && entry.depth() == 1
                        && entry.file_name().to_string_lossy().starts_with('.'))
                })
            {
                let entry = entry?;
                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
        }
        if matches!(kind, CacheKind::Partials) {
            files.extend(crate::download_tasks::local_objects(self.root())?);
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
    pub fn clear_resource_cache(&self, kind: CacheKind) -> Result<()> {
        ensure!(matches!(kind, CacheKind::Partials), "仅允许清理未完成下载");
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let files = self.cache_files(kind)?;
        crate::download_tasks::remove_local_objects(
            self.root(),
            files
                .iter()
                .filter(|p| !p.starts_with(self.root()))
                .cloned()
                .collect(),
        )?;
        for path in files {
            // Re-check the complete path so symlinked parents cannot escape the allowlist.
            if let Ok(relative) = path.strip_prefix(self.root()) {
                fs::remove_file(owned(self.root(), relative)?)?;
            }
        }
        Ok(())
    }
    fn skin_home(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "皮肤标识无效"
        );
        owned(self.root(), &Path::new("skin-library").join(id))
    }
    fn skin_info(&self, id: &str) -> Result<SavedSkin> {
        if matches!(id, "builtin:steve" | "builtin:cirno") {
            return Ok(SavedSkin {
                id: id.into(),
                name: id.trim_start_matches("builtin:").into(),
                slim: id == "builtin:cirno",
                texture_url: None,
            });
        }
        if let Some(account_id) = id.strip_prefix("account:") {
            let account = self
                .snapshot()
                .database
                .accounts
                .into_iter()
                .find(|account| account.id == account_id)
                .context("账户不存在")?;
            return Ok(SavedSkin {
                id: id.into(),
                name: account.username,
                slim: account.skin.as_ref().is_some_and(|skin| skin.slim),
                texture_url: account.skin.map(|skin| skin.url),
            });
        }
        let home = self.skin_home(id)?;
        let skin: SavedSkin =
            serde_json::from_slice(&fs::read(owned(&home, Path::new("skin.json"))?)?)?;
        ensure!(skin.id == id, "皮肤记录与目录不匹配");
        self.saved_skin_bytes(id)?;
        Ok(skin)
    }
    pub fn set_guest_skin(&self, id: &str) -> Result<String> {
        // Selecting a preview does not acquire the download/install job lock.
        self.skin_info(id)?;
        self.mutate(|db| {
            db.guest_skin = id.into();
            Ok(())
        })?;
        Ok(id.into())
    }
    pub async fn guest_skin(&self) -> Result<(SavedSkin, Option<Vec<u8>>)> {
        let id = self.snapshot().database.guest_skin;
        self.resource_skin(&id).await
    }
    pub async fn resource_skin(&self, id: &str) -> Result<(SavedSkin, Option<Vec<u8>>)> {
        let info = self.skin_info(id)?;
        let bytes = if id.starts_with("builtin:") {
            None
        } else if let Some(account) = id.strip_prefix("account:") {
            self.account_skin(account).await?
        } else {
            Some(self.saved_skin_bytes(id)?)
        };
        Ok((info, bytes))
    }
    pub fn saved_skins(&self) -> Result<Vec<SavedSkin>> {
        let root = owned(self.root(), Path::new("skin-library"))?;
        if !root.try_exists()? {
            return Ok(vec![]);
        }
        let mut skins = vec![];
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let home = self.skin_home(&id)?;
            let skin: SavedSkin =
                serde_json::from_slice(&fs::read(owned(&home, Path::new("skin.json"))?)?)?;
            ensure!(skin.id == id, "皮肤记录与目录不匹配");
            skins.push(skin);
        }
        skins.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(skins)
    }
    pub fn import_saved_skin(&self, path: &Path) -> Result<SavedSkin> {
        ensure!(fs::metadata(path)?.len() <= 2 * 1024 * 1024, "皮肤文件过大");
        let bytes = fs::read(path)?;
        self.save_skin_bytes(
            path.file_stem().and_then(|n| n.to_str()).unwrap_or("Skin"),
            &bytes,
            false,
        )
    }
    pub fn save_skin_bytes(&self, name: &str, bytes: &[u8], slim: bool) -> Result<SavedSkin> {
        crate::skin::validate_png(bytes)?;
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let id = hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes));
        let home = self.skin_home(&id)?;
        if owned(&home, Path::new("skin.json"))?.is_file() {
            return Ok(serde_json::from_slice(&fs::read(owned(
                &home,
                Path::new("skin.json"),
            )?)?)?);
        }
        let skin = SavedSkin {
            id,
            name: name.trim().chars().take(80).collect(),
            slim,
            texture_url: None,
        };
        storage::atomic_write(&owned(&home, Path::new("skin.png"))?, bytes)?;
        storage::atomic_write(
            &owned(&home, Path::new("skin.json"))?,
            &serde_json::to_vec_pretty(&skin)?,
        )?;
        Ok(skin)
    }
    pub fn saved_skin_bytes(&self, id: &str) -> Result<Vec<u8>> {
        let path = owned(&self.skin_home(id)?, Path::new("skin.png"))?;
        ensure!(
            fs::metadata(&path)?.len() <= 2 * 1024 * 1024,
            "皮肤文件过大"
        );
        let bytes = fs::read(path)?;
        crate::skin::validate_png(&bytes)?;
        Ok(bytes)
    }
    pub(crate) fn remember_uploaded_skin(&self, bytes: &[u8], url: &str) -> Result<()> {
        let id = hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes));
        let path = owned(&self.skin_home(&id)?, Path::new("skin.json"))?;
        if !path.is_file() {
            return Ok(());
        }
        let mut skin = self.skin_info(&id)?;
        skin.texture_url = Some(url.into());
        storage::atomic_write(&path, &serde_json::to_vec_pretty(&skin)?)
    }
    pub fn edit_saved_skin(&self, id: &str, name: &str, slim: bool) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        ensure!(
            !name.trim().is_empty() && name.chars().count() <= 80,
            "皮肤名称须为 1–80 个字符"
        );
        let home = self.skin_home(id)?;
        ensure!(owned(&home, Path::new("skin.png"))?.is_file(), "皮肤不存在");
        storage::atomic_write(
            &owned(&home, Path::new("skin.json"))?,
            &serde_json::to_vec_pretty(&SavedSkin {
                id: id.to_owned(),
                name: name.trim().to_owned(),
                slim,
                ..self.skin_info(id)?
            })?,
        )
    }
    pub fn delete_saved_skin(&self, id: &str) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        crate::recycle::move_to_trash(&self.skin_home(id)?)?;
        self.mutate(|db| {
            if db.guest_skin == id {
                db.guest_skin = "builtin:steve".into();
            }
            Ok(())
        })
    }
    pub fn export_saved_skin(&self, id: &str, path: &Path) -> Result<()> {
        let bytes = self.saved_skin_bytes(id)?;
        export_skin_png(&bytes, path)
    }
    pub async fn export_account_skin(&self, id: &str, path: &Path) -> Result<()> {
        let bytes = self
            .account_skin(id)
            .await?
            .context("此账户没有自定义皮肤")?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || export_skin_png(&bytes, &path)).await?
    }
}

pub fn export_skin_png(bytes: &[u8], path: &Path) -> Result<()> {
    crate::skin::validate_png(bytes)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "导出目标不是普通文件"
        );
    }
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png")),
        "请选择 PNG 文件名"
    );
    storage::atomic_write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version_management::ContentKind;
    fn fixture() -> (tempfile::TempDir, Launcher) {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        (temp, launcher)
    }
    fn put(root: &Path, path: &str, bytes: &[u8]) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    #[tokio::test]
    async fn guest_skin_selection_persists_without_changing_launch_identity_and_validates_sources()
    {
        let (_temp, launcher) = fixture();
        launcher
            .mutate(|db| {
                db.accounts.push(crate::model::Account {
                    id: "microsoft".into(),
                    username: "Neko".into(),
                    expires_at: 0,
                    client_id: "fixture".into(),
                    owned: true,
                    skin: None,
                });
                db.selected_account = Some("microsoft".into());
                db.guest_name = "Guest_42".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(launcher.snapshot().database.guest_skin, "builtin:steve");
        launcher.set_guest_skin("builtin:cirno").unwrap();
        let (skin, bytes) = launcher.guest_skin().await.unwrap();
        assert_eq!(skin.name, "cirno");
        assert!(skin.slim && bytes.is_none());
        assert_eq!(
            launcher.snapshot().database.selected_account.as_deref(),
            Some("microsoft")
        );
        assert_eq!(launcher.snapshot().database.guest_name, "Guest_42");
        for invalid in [
            "builtin:missing",
            "account:missing",
            "../../database.json",
            &"a".repeat(64),
        ] {
            assert!(launcher.set_guest_skin(invalid).is_err());
            assert_eq!(launcher.snapshot().database.guest_skin, "builtin:cirno");
        }
        launcher.set_guest_skin("account:microsoft").unwrap();
        let (account, bytes) = launcher.guest_skin().await.unwrap();
        assert_eq!(account.name, "Neko");
        assert!(bytes.is_none());
        let original = include_bytes!("../../../app/src/assets/steve.png");
        let local = launcher
            .save_skin_bytes("Imported", original, true)
            .unwrap();
        launcher.set_guest_skin(&local.id).unwrap();
        let root = launcher.root().to_owned();
        drop(launcher);
        let launcher = Launcher::open(Some(root)).unwrap();
        assert_eq!(launcher.snapshot().database.guest_skin, local.id);
        let (selected, bytes) = launcher.guest_skin().await.unwrap();
        assert!(selected.slim);
        assert_eq!(bytes.unwrap(), original);
        launcher
            .edit_saved_skin(&local.id, "Classic", false)
            .unwrap();
        assert!(!launcher.guest_skin().await.unwrap().0.slim);
        launcher.delete_saved_skin(&local.id).unwrap();
        assert_eq!(launcher.snapshot().database.guest_skin, "builtin:steve");
        assert_eq!(
            launcher.snapshot().database.selected_account.as_deref(),
            Some("microsoft")
        );
        let mut legacy = serde_json::to_value(launcher.snapshot().database).unwrap();
        legacy.as_object_mut().unwrap().remove("guest_skin");
        let legacy: crate::model::Database = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.guest_skin, "builtin:steve");
    }
    #[test]
    fn cache_inventory_includes_installed_java_and_only_clears_unfinished_downloads() {
        let (_temp, launcher) = fixture();
        let root = launcher.root();
        let unfinished = [
            "cache/objects/sha1/complete",
            "cache/objects/sha1/resume.part",
            "cache/java/java.tar.gz",
        ];
        let preserved = [
            "cache/verified-files.json",
            "metadata/manifest.json",
            "instances/test/.minecraft/saves/world/level.dat",
            "runtimes/jdk-17/bin/java",
            "runtimes/.install-unfinished/bin/java",
            "cache/skins/account.png",
            "skin-library/test/skin.png",
        ];
        for path in unfinished.into_iter().chain(preserved) {
            put(root, path, b"preserved");
        }
        let entries = launcher.resource_cache().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0].kind, CacheKind::Metadata));
        assert_eq!(entries[0].files, 1);
        assert!(matches!(entries[1].kind, CacheKind::Resources));
        assert_eq!(entries[1].files, 3);
        assert_eq!(entries[1].size, 27);
        assert_eq!(entries[1].parts.len(), 2);
        assert_eq!(entries[1].parts[0].kind, "java");
        assert_eq!(entries[1].parts[0].size, 9);
        assert_eq!(entries[1].parts[1].kind, "skins");
        assert_eq!(entries[1].parts[1].size, 18);
        assert!(matches!(entries[2].kind, CacheKind::Partials));
        assert_eq!(entries[2].files, unfinished.len());
        assert_eq!(entries[2].size, unfinished.len() as u64 * 9);
        assert!(launcher.clear_resource_cache(CacheKind::Resources).is_err());
        assert!(launcher.clear_resource_cache(CacheKind::Metadata).is_err());
        {
            let _operation = launcher.0.operation.try_lock().unwrap();
            assert!(launcher.clear_resource_cache(CacheKind::Partials).is_err());
            for path in unfinished {
                assert!(root.join(path).is_file(), "{path}");
            }
        }
        launcher.clear_resource_cache(CacheKind::Partials).unwrap();
        for path in unfinished {
            assert!(!root.join(path).exists(), "{path}");
        }
        for path in preserved {
            assert!(root.join(path).is_file(), "{path}");
        }
        let entries = launcher.resource_cache().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].files, 3);
        assert_eq!(entries[1].size, 27);
    }
    #[cfg(unix)]
    #[test]
    fn cache_never_follows_symlinks_out_of_owned_storage() {
        use std::os::unix::fs::symlink;
        let (temp, launcher) = fixture();
        put(temp.path(), "external/important", b"keep");
        fs::create_dir_all(launcher.root().join("cache/objects")).unwrap();
        symlink(
            temp.path().join("external"),
            launcher.root().join("cache/objects/linked"),
        )
        .unwrap();
        launcher.clear_resource_cache(CacheKind::Partials).unwrap();
        assert!(temp.path().join("external/important").is_file());
        fs::remove_dir(launcher.root().join("metadata")).unwrap();
        symlink(
            temp.path().join("external"),
            launcher.root().join("metadata"),
        )
        .unwrap();
        assert!(launcher.resource_cache().is_err());
        assert!(owned(launcher.root(), Path::new("../external")).is_err());
        assert!(owned(launcher.root(), Path::new("/tmp")).is_err());
    }
    #[tokio::test]
    async fn account_skin_exports_cached_texture_without_login_or_collecting_a_copy() {
        let (temp, launcher) = fixture();
        let bytes = include_bytes!("../../../app/src/assets/steve.png");
        let hash = "a".repeat(64);
        launcher
            .mutate(|db| {
                db.accounts.push(crate::model::Account {
                    id: "account-fixture".into(),
                    username: "Fixture".into(),
                    client_id: "not-an-oauth-client".into(),
                    expires_at: 0,
                    owned: true,
                    skin: Some(crate::model::AccountSkin {
                        url: format!("https://textures.minecraft.net/texture/{hash}"),
                        slim: true,
                    }),
                });
                db.selected_account = Some("account-fixture".into());
                // An attempted network refresh would fail. The cached texture is sufficient.
                db.settings.proxy_mode = "http".into();
                db.settings.proxy_url = "invalid".into();
                Ok(())
            })
            .unwrap();
        storage::atomic_write(
            &launcher
                .root()
                .join("cache/skins")
                .join(format!("{hash}.png")),
            bytes,
        )
        .unwrap();
        let output = temp.path().join("account.png");
        launcher
            .export_account_skin("account-fixture", &output)
            .await
            .unwrap();
        assert_eq!(fs::read(output).unwrap(), bytes);
        assert!(launcher.saved_skins().unwrap().is_empty());
        assert!(!launcher.root().join("skin-library").exists());
        assert_eq!(launcher.snapshot().database.accounts[0].expires_at, 0);
        assert!(
            launcher
                .export_account_skin("missing", &temp.path().join("missing.png"))
                .await
                .is_err()
        );
        assert!(
            launcher
                .export_account_skin("account-fixture", &temp.path().join("wrong.json"))
                .await
                .is_err()
        );
    }

    #[test]
    fn skin_collection_deduplicates_preserves_model_and_exports_original() {
        let (temp, launcher) = fixture();
        let bytes = include_bytes!("../../../app/src/assets/steve.png");
        let skin = launcher.save_skin_bytes("Steve", bytes, false).unwrap();
        launcher.edit_saved_skin(&skin.id, "旅人", true).unwrap();
        let duplicate = launcher.save_skin_bytes("Other", bytes, false).unwrap();
        assert_eq!(duplicate.name, "旅人");
        assert!(duplicate.slim);
        assert_eq!(launcher.saved_skins().unwrap().len(), 1);
        assert!(launcher.clear_resource_cache(CacheKind::Resources).is_err());
        let output = temp.path().join("export.png");
        launcher.export_saved_skin(&skin.id, &output).unwrap();
        assert_eq!(fs::read(output).unwrap(), bytes);
        assert!(launcher.saved_skin_bytes("../../database.json").is_err());
        assert!(launcher.save_skin_bytes("Bad", b"not png", false).is_err());
        assert!(
            launcher
                .export_saved_skin(&skin.id, &temp.path().join("not-json.json"))
                .is_err()
        );
    }
    #[test]
    fn shared_resource_folders_are_indexed_once_and_running_owner_blocks_edits() {
        let (_temp, launcher) = fixture();
        let a = launcher.create_instance("A", "1.20.1").unwrap();
        let b = launcher.create_instance("B", "1.20.1").unwrap();
        let shared = a.game_dir(launcher.root());
        fs::create_dir_all(shared.join("mods")).unwrap();
        launcher
            .mutate(|db| {
                db.instances
                    .iter_mut()
                    .find(|i| i.id == b.id)
                    .unwrap()
                    .game_directory = Some(shared.clone());
                Ok(())
            })
            .unwrap();
        let index = launcher.resource_groups(ContentKind::Mods);
        assert!(index.issues.is_empty());
        assert_eq!(index.groups.len(), 1);
        assert_eq!(index.groups[0].owners.len(), 2);
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(b.id.clone(), CancellationToken::new());
        assert!(launcher.resource_directory_idle(&a.id, "1.20.1").is_err());
    }
    #[test]
    fn external_java_cannot_be_deleted_and_registration_removal_keeps_file() {
        let (temp, launcher) = fixture();
        put(temp.path(), "external/bin/java", b"java");
        let path = temp.path().join("external/bin/java");
        assert!(launcher.remove_resource_java(&path).is_err());
        storage::atomic_write(
            &launcher.root().join("java-paths.json"),
            &serde_json::to_vec(&vec![path.clone()]).unwrap(),
        )
        .unwrap();
        launcher.remove_resource_java(&path).unwrap();
        assert!(path.is_file());
        assert!(registered_java(launcher.root()).is_empty());
    }
    #[test]
    fn managed_java_removal_protects_all_executables_in_the_same_runtime() {
        let (_temp, launcher) = fixture();
        put(launcher.root(), "runtimes/jdk-17/bin/java", b"java");
        put(launcher.root(), "runtimes/jdk-17/jre/bin/java", b"java");
        let instance = launcher.create_instance("Configured", "1.20.1").unwrap();
        let nested = launcher.root().join("runtimes/jdk-17/jre/bin/java");
        launcher
            .mutate(|db| {
                db.instances
                    .iter_mut()
                    .find(|i| i.id == instance.id)
                    .unwrap()
                    .java_path = Some(nested.to_string_lossy().into_owned());
                Ok(())
            })
            .unwrap();
        let primary = launcher.root().join("runtimes/jdk-17/bin/java");
        assert_eq!(launcher.runtime_users(&primary), vec!["Configured"]);
        assert!(launcher.remove_resource_java(&primary).is_err());
        assert!(primary.is_file());
        assert!(nested.is_file());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn registered_java_is_available_for_auto_selection_without_downloading() {
        use std::os::unix::fs::PermissionsExt;
        let (temp, launcher) = fixture();
        let path = temp.path().join("custom/bin/java");
        put(
            temp.path(),
            "custom/bin/java",
            b"#!/bin/sh\nprintf 'openjdk version \"21.0.9\"\nos.arch = aarch64\n' >&2\n",
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let info = launcher.register_java(&path).await.unwrap();
        assert_eq!(info.major, 21);
        assert!(
            java::discover(launcher.root())
                .await
                .iter()
                .any(|j| Path::new(&j.path) == path)
        );
        assert_eq!(registered_java(launcher.root()).len(), 1);
        launcher.register_java(&path).await.unwrap();
        assert_eq!(registered_java(launcher.root()).len(), 1);
    }
}
