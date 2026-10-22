pub mod auth;
pub mod download;
pub mod java;
mod migration;
pub mod minecraft;
pub mod model;
pub mod skin;
pub mod storage;
#[cfg(test)]
mod test_http;

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use model::*;
use serde::Serialize;
use std::{
    collections::HashMap,
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
    games: Mutex<HashMap<String, CancellationToken>>,
}
impl Drop for Shared {
    fn drop(&mut self) {
        // A concurrent fork can retain the open file description until exec.
        // Release ownership with the last Launcher, not the last inherited fd.
        let _ = FileExt::unlock(&self._lock);
    }
}
#[derive(Clone)]
pub struct Launcher(Arc<Shared>);
#[derive(Serialize)]
pub struct Snapshot {
    pub root: String,
    pub database: Database,
    pub running: Vec<String>,
}
impl Launcher {
    pub fn open(root: Option<PathBuf>) -> Result<Self> {
        let root = root
            .or_else(|| std::env::var_os("NEKOCRATE_DATA_DIR").map(PathBuf::from))
            .or_else(|| {
                directories::ProjectDirs::from("dev", "NekoCrate", "NekoCrate")
                    .map(|p| p.data_local_dir().to_owned())
            })
            .context("无法确定数据目录")?;
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("launcher.lock"))?;
        lock.try_lock_exclusive()
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
            "trash",
        ] {
            fs::create_dir_all(root.join(dir))?;
        }
        Ok(Self(Arc::new(Shared {
            root,
            _lock: lock,
            db: Mutex::new(db),
            operation: tokio::sync::Mutex::new(()),
            games: Mutex::new(HashMap::new()),
        })))
    }
    pub fn root(&self) -> &Path {
        &self.0.root
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            root: self.root().to_string_lossy().into(),
            database: self.0.db.lock().unwrap().clone(),
            running: self.0.games.lock().unwrap().keys().cloned().collect(),
        }
    }
    pub fn settings(&self) -> Settings {
        self.0.db.lock().unwrap().settings.clone()
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
        Ok(self.instance_dir(id)?.join(".minecraft"))
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
                instance.versions.push(InstanceVersion {
                    id: version.into(),
                    installed: false,
                });
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
        self.mutate(|db| {
            let entry = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .and_then(|i| i.versions.iter_mut().find(|v| v.id == version))
                .context("实例中不存在该版本")?;
            entry.installed = installed;
            Ok(())
        })
    }
    pub fn save_settings(&self, mut settings: Settings) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请完成后修改设置")?;
        if !(1..=32).contains(&settings.concurrency) || settings.retries > 5 {
            bail!("并发应为 1–32，重试次数应为 0–5");
        }
        if !["official", "auto", "bmclapi"].contains(&settings.download_source.as_str()) {
            bail!("未知下载源");
        }
        settings.microsoft_client_id = settings.microsoft_client_id.trim().to_owned();
        if !settings.microsoft_client_id.is_empty() {
            uuid::Uuid::parse_str(&settings.microsoft_client_id).context("Client ID 应为 UUID")?;
        }
        self.mutate(|db| {
            db.settings = settings;
            Ok(())
        })
    }
    pub fn create_instance(&self, name: &str, version: &str) -> Result<Instance> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后创建实例")?;
        self.create_unlocked(name, version)
    }
    fn create_unlocked(&self, name: &str, version: &str) -> Result<Instance> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            bail!("实例名称应为 1–80 个字符");
        }
        if !version.is_empty() {
            storage::safe_version(version)?;
        }
        let i = Instance {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            icon: None,
            versions: if version.is_empty() {
                vec![]
            } else {
                vec![InstanceVersion {
                    id: version.into(),
                    installed: false,
                }]
            },
            selected_version: (!version.is_empty()).then(|| version.into()),
            memory_mb: 2048,
            java_path: None,
            created_at: storage::now(),
            last_played: None,
        };
        let dir = self.root().join("instances").join(&i.id);
        fs::create_dir_all(dir.join(".minecraft"))?;
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
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后修改实例")?;
        self.idle(id)?;
        if name.trim().is_empty()
            || name.chars().count() > 80
            || !(512..=65536).contains(&memory_mb)
        {
            bail!("名称应为 1–80 个字符，内存应为 512–65536 MiB");
        }
        if icon.is_some_and(|value| value > 3) {
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
            i.java_path = java_path.filter(|s| !s.trim().is_empty());
            if let Some(icon) = icon {
                i.icon = Some(icon);
            }
            Ok(())
        })
    }
    pub async fn import_instance(
        &self,
        source: PathBuf,
        name: String,
        version: String,
    ) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        let source = storage::expand_user_path(&source)?;
        let source = source
            .canonicalize()
            .with_context(|| format!("无法访问导入目录：{}", source.display()))?;
        if !source.is_dir() {
            bail!("导入来源必须是一个目录：{}", source.display());
        }
        let is_game_directory = |path: &Path| {
            ["saves", "mods", "versions", "options.txt"]
                .iter()
                .any(|entry| path.join(entry).exists())
        };
        let source = if is_game_directory(&source) {
            source
        } else if source.join(".minecraft").is_dir() {
            source.join(".minecraft")
        } else if source.join("minecraft").is_dir() {
            source.join("minecraft")
        } else if source.join("game").is_dir() {
            source.join("game")
        } else {
            source
        };
        if !is_game_directory(&source) {
            bail!("请选择含 saves、mods、versions 或 options.txt 的游戏目录");
        }
        let mut versions = vec![];
        if source.join("versions").is_dir() {
            for entry in fs::read_dir(source.join("versions"))? {
                let entry = entry?;
                let id = entry.file_name().to_string_lossy().into_owned();
                if !entry.file_type()?.is_dir() || storage::safe_version(&id).is_err() {
                    continue;
                }
                if let Ok(metadata) =
                    storage::read_json::<Version>(&entry.path().join(format!("{id}.json")))
                    && metadata.id == id
                    && metadata.inherits_from.is_none()
                {
                    versions.push(InstanceVersion {
                        id,
                        installed: false,
                    });
                }
            }
        }
        versions.sort_by(|a, b| a.id.cmp(&b.id));
        if !version.is_empty() && !versions.iter().any(|v| v.id == version) {
            storage::safe_version(&version)?;
            versions.push(InstanceVersion {
                id: version.clone(),
                installed: false,
            });
        }
        let i = self.create_unlocked(&name, "")?;
        let dest = self.root().join("instances").join(&i.id).join(".minecraft");
        let result =
            tokio::task::spawn_blocking(move || storage::copy_tree(&source, &dest)).await?;
        if let Err(e) = result {
            self.rollback_new(&i.id)?;
            return Err(e);
        }
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
        let i = self.create_unlocked(&format!("{} · 副本", original.name), "")?;
        let source = self.root().join("instances").join(id).join(".minecraft");
        let dest = self.root().join("instances").join(&i.id).join(".minecraft");
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
            target.java_path = original.java_path.clone();
            target.icon = original.icon;
            Ok(())
        })?;
        self.instance(&i.id)
    }
    pub fn delete_instance(&self, id: &str) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后移除实例")?;
        self.idle(id)?;
        let instance = self.instance(id)?;
        let source = self.instance_dir(id)?;
        let dest = self
            .root()
            .join("trash")
            .join(format!("{}-{id}", storage::now()));
        storage::write_json(&source.join("instance.json"), &instance)?;
        fs::rename(&source, &dest)?;
        if let Err(e) = self.mutate(|db| {
            db.instances.retain(|i| i.id != id);
            Ok(())
        }) {
            fs::rename(dest, source)?;
            return Err(e);
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
    pub async fn install(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let instance = self.instance(id)?;
        let selected = instance
            .selected()
            .context("请先为此实例添加并选择一个版本")?;
        self.set_installed(id, &selected.id, false)?;
        let settings = self.settings();
        let downloader = download::Downloader::new(self.root(), settings.clone())?;
        sink(Event::new("stage", "获取官方版本元数据"));
        let v = minecraft::version(self.root(), &selected.id, &downloader, cancel, sink).await?;
        minecraft::install_files(
            self.root(),
            &self.instance_dir(id)?.join(".minecraft"),
            &v,
            &downloader,
            cancel,
            sink,
        )
        .await?;
        let java = java::ensure(
            self.root(),
            v.java_version.as_ref().map_or(8, |j| j.major),
            instance.java_path.as_deref(),
            settings.auto_java,
            &downloader,
            cancel,
            sink,
        )
        .await?;
        download::check_cancel(cancel)?;
        let version_dir = self.game_dir(id)?.join("versions").join(&selected.id);
        storage::write_json(&version_dir.join(format!("{}.json", selected.id)), &v)?;
        storage::write_json(&version_dir.join("runtime.json"), &java)?;
        self.set_installed(id, &selected.id, true)?;
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
        let _guard = self.0.operation.lock().await;
        let client = download::Downloader::new(self.root(), self.settings())?.client;
        let session = auth::finish(&client, challenge, cancel).await?;
        download::check_cancel(cancel)?;
        auth::save_secret(self.root(), &session.account.id, &session.refresh_token)?;
        self.save_account(session.account.clone())?;
        Ok(session.account)
    }
    fn save_account(&self, account: Account) -> Result<()> {
        self.mutate(|db| {
            db.accounts.retain(|a| a.id != account.id);
            db.selected_account = Some(account.id.clone());
            db.accounts.push(account);
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
    pub fn remove_account(&self, id: &str) -> Result<()> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在使用账户，请稍后移除")?;
        auth::remove_secret(self.root(), id)?;
        self.mutate(|db| {
            db.accounts.retain(|a| a.id != id);
            if db.selected_account.as_deref() == Some(id) {
                db.selected_account = db.accounts.first().map(|a| a.id.clone());
            }
            Ok(())
        })
    }
    async fn session(&self) -> Result<auth::MinecraftSession> {
        let snapshot = self.snapshot();
        let id = snapshot
            .database
            .selected_account
            .context("请先添加并选择微软账户")?;
        let account = snapshot
            .database
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .context("所选账户不存在")?;
        let secret = auth::load_secret(self.root(), &id)?;
        let client = download::Downloader::new(self.root(), self.settings())?.client;
        let session = auth::refresh(&client, &account, &secret, |token| {
            auth::save_secret(self.root(), &id, token)
        })
        .await?;
        if session.account.id != id {
            bail!("刷新后账户发生变化，请重新登录");
        }
        self.save_account(session.account.clone())?;
        Ok(session)
    }
    pub async fn refresh_account(&self) -> Result<Account> {
        let _guard = self.0.operation.lock().await;
        Ok(self.session().await?.account)
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
            Some(skin) => Ok(Some(skin::load(self.root(), &skin).await?)),
            None => Ok(None),
        }
    }
    async fn launch_session(&self, sink: &EventSink) -> Result<minecraft::LaunchSession> {
        if self.snapshot().database.selected_account.is_none() {
            sink(Event::new("stage", "以未登录身份 Player 启动"));
            Ok(minecraft::LaunchSession::guest())
        } else {
            sink(Event::new("stage", "刷新登录并验证 Java 版权益"));
            // An expired or invalid selected account must surface its error,
            // rather than silently change the player's identity and save data.
            Ok(self.session().await?.into())
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
        let instance = self.instance(id)?;
        let selected = instance.selected().context("请先选择游戏版本")?;
        if !selected.installed {
            bail!("请先安装或修复所选版本");
        }
        let v: Version = storage::read_json(
            &self
                .game_dir(id)?
                .join("versions")
                .join(&selected.id)
                .join(format!("{}.json", selected.id)),
        )?;
        if v.id != selected.id {
            bail!("版本元数据与所选版本不一致，请修复该版本");
        }
        let settings = self.settings();
        let downloader = download::Downloader::new(self.root(), settings.clone())?;
        let java = java::ensure(
            self.root(),
            v.java_version.as_ref().map_or(8, |j| j.major),
            instance.java_path.as_deref(),
            settings.auto_java,
            &downloader,
            cancel,
            &sink,
        )
        .await?;
        let session = tokio::select! {_=cancel.cancelled()=>bail!("启动已取消"),r=self.launch_session(&sink)=>r?};
        let args = minecraft::arguments(&v, self.root(), &instance, &session)?;
        let log_dir = self.instance_dir(id)?.join("launcher-logs");
        fs::create_dir_all(&log_dir)?;
        let log = Arc::new(tokio::sync::Mutex::new(
            tokio::fs::File::create(log_dir.join(format!("{}.log", storage::now()))).await?,
        ));
        let mut command = tokio::process::Command::new(&java.path);
        command
            .args(args)
            .current_dir(self.instance_dir(id)?.join(".minecraft"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        download::check_cancel(cancel)?;
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
        let err = tokio::spawn(pipe_logs(stderr, id.into(), sink.clone(), log, secrets));
        let launcher = self.clone();
        let id = id.to_owned();
        sink(Event {
            instance_id: Some(id.clone()),
            ..Event::new("game_started", format!("游戏已启动 · PID {pid}"))
        });
        tokio::spawn(async move {
            let status = tokio::select! {r=child.wait()=>r,_=cancel.cancelled()=>{let _=child.kill().await;child.wait().await}};
            let _ = out.await;
            let _ = err.await;
            launcher.0.games.lock().unwrap().remove(&id);
            let message = match status {
                Ok(s) if s.success() => "游戏已正常退出".into(),
                Ok(s) => format!(
                    "游戏已退出（状态 {s}）。如非主动停止，请查看日志，检查 Java、内存和游戏配置。"
                ),
                Err(e) => format!("读取游戏进程状态失败：{e}"),
            };
            sink(Event {
                instance_id: Some(id),
                ..Event::new("game_exit", message)
            });
        });
        Ok(pid)
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
            String::from_utf8_lossy(&bytes).trim_end().to_string()
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
        assert_eq!(instance.icon, None);
        launcher
            .update_instance(&instance.id, "  新名称  ", 4096, None, Some(2))
            .unwrap();
        let updated = launcher.instance(&instance.id).unwrap();
        assert_eq!(updated.name, "新名称");
        assert_eq!(updated.icon, Some(2));
        assert_eq!(launcher.game_dir(&instance.id).unwrap(), directory);

        // Missing icon fields from older clients must not reset the saved choice.
        launcher
            .update_instance(&instance.id, "新名称", 2048, None, None)
            .unwrap();
        assert_eq!(launcher.instance(&instance.id).unwrap().icon, Some(2));
        assert!(
            launcher
                .update_instance(&instance.id, "不应保存", 8192, None, Some(4))
                .is_err()
        );
        let unchanged = launcher.instance(&instance.id).unwrap();
        assert_eq!(unchanged.name, "新名称");
        assert_eq!(unchanged.memory_mb, 2048);
        assert_eq!(unchanged.icon, Some(2));
        let cloned = launcher.clone_instance(&instance.id).await.unwrap();
        assert_eq!(cloned.icon, Some(2));
        drop(launcher);

        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(reopened.instance(&instance.id).unwrap().icon, Some(2));
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
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let java = temp.path().join("java fixture");
        fs::write(
            &java,
            r#"#!/bin/sh
if [ "$1" = "-version" ]; then
  printf '%s\n' 'java version "1.8.0_fixture"' >&2
  exit 0
fi
printf '%s\n' "$@" > launch-arguments.txt
printf '%s\n' 'offline session 0 started'
"#,
        )
        .unwrap();
        fs::set_permissions(&java, fs::Permissions::from_mode(0o700)).unwrap();
        let instance = launcher.create_instance("未登录启动", "1.21.1").unwrap();
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
        let version_dir = game.join("versions/1.21.1");
        fs::create_dir_all(&version_dir).unwrap();
        storage::write_json(&version_dir.join("1.21.1.json"), &version).unwrap();
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
        for (key, value) in [
            ("--username", "Player"),
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
        assert!(launcher.snapshot().database.accounts.is_empty());
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
    async fn migration_copies_saves_and_excludes_credentials() {
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
        let game = launcher
            .instance_dir(&instance.id)
            .unwrap()
            .join(".minecraft");
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"save-fixture"
        );
        assert!(!game.join("launcher_accounts.json").exists());
        assert!(source.join("launcher_accounts.json").exists());
        assert!(!instance.selected().unwrap().installed);
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
        l.delete_instance(&b.id).unwrap();
        assert_eq!(l.snapshot().database.instances.len(), 1);
        assert_eq!(fs::read_dir(t.path().join("trash")).unwrap().count(), 1);
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
        assert!(!imported.join("launcher_accounts.json").exists());
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
