pub mod auth;
pub mod download;
pub mod java;
pub mod minecraft;
pub mod model;
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
        let db: Database = if dbpath.exists() {
            storage::read_json(&dbpath)?
        } else {
            Database::default()
        };
        if db.schema != 1 {
            bail!("不支持的数据格式版本 {}", db.schema);
        }
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
        if version.len() > 80 || storage::safe_relative(version)?.components().count() != 1 {
            bail!("游戏版本不合法");
        }
        let i = Instance {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            version: version.into(),
            memory_mb: 2048,
            java_path: None,
            created_at: storage::now(),
            installed: false,
            last_played: None,
        };
        let dir = self.root().join("instances").join(&i.id);
        fs::create_dir_all(dir.join("game"))?;
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
        self.mutate(|db| {
            let i = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            i.name = name.trim().into();
            i.memory_mb = memory_mb;
            i.java_path = java_path.filter(|s| !s.trim().is_empty());
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
        let source = if source.join(".minecraft").is_dir() {
            source.join(".minecraft")
        } else if source.join("minecraft").is_dir() {
            source.join("minecraft")
        } else if source.join("game").is_dir() {
            source.join("game")
        } else {
            source
        };
        if !["saves", "mods", "versions", "options.txt"]
            .iter()
            .any(|p| source.join(p).exists())
        {
            bail!("请选择含 saves、mods、versions 或 options.txt 的游戏目录");
        }
        let i = self.create_unlocked(&name, &version)?;
        let dest = self.root().join("instances").join(&i.id).join("game");
        let result =
            tokio::task::spawn_blocking(move || storage::copy_tree(&source, &dest)).await?;
        if let Err(e) = result {
            self.rollback_new(&i.id)?;
            return Err(e);
        }
        Ok(i)
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
        let i = self.create_unlocked(&format!("{} · 副本", original.name), &original.version)?;
        let source = self.root().join("instances").join(id).join("game");
        let dest = self.root().join("instances").join(&i.id).join("game");
        if let Err(e) =
            tokio::task::spawn_blocking(move || storage::copy_tree(&source, &dest)).await?
        {
            self.rollback_new(&i.id)?;
            return Err(e);
        }
        self.mutate(|db| {
            let target = db.instances.iter_mut().find(|x| x.id == i.id).unwrap();
            target.memory_mb = original.memory_mb;
            target.java_path = original.java_path.clone();
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
        self.mutate(|db| {
            db.instances
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .installed = false;
            Ok(())
        })?;
        let settings = self.settings();
        let downloader = download::Downloader::new(self.root(), settings.clone())?;
        sink(Event::new("stage", "获取官方版本元数据"));
        let v =
            minecraft::version(self.root(), &instance.version, &downloader, cancel, sink).await?;
        minecraft::install_files(
            self.root(),
            &self.instance_dir(id)?.join("game"),
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
        storage::write_json(&self.instance_dir(id)?.join("version.json"), &v)?;
        storage::write_json(&self.instance_dir(id)?.join("runtime.json"), &java)?;
        self.mutate(|db| {
            db.instances
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .installed = true;
            Ok(())
        })?;
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
    pub async fn launch(
        &self,
        id: &str,
        cancel: &CancellationToken,
        sink: EventSink,
    ) -> Result<u32> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let instance = self.instance(id)?;
        if !instance.installed {
            bail!("请先安装或修复此实例");
        }
        let v: Version = storage::read_json(&self.instance_dir(id)?.join("version.json"))?;
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
        sink(Event::new("stage", "刷新登录并验证 Java 版权益"));
        let session =
            tokio::select! {_=cancel.cancelled()=>bail!("启动已取消"),r=self.session()=>r?};
        let args = minecraft::arguments(&v, self.root(), &instance, &session)?;
        let log_dir = self.instance_dir(id)?.join("launcher-logs");
        fs::create_dir_all(&log_dir)?;
        let log = Arc::new(tokio::sync::Mutex::new(
            tokio::fs::File::create(log_dir.join(format!("{}.log", storage::now()))).await?,
        ));
        let mut command = tokio::process::Command::new(&java.path);
        command
            .args(args)
            .current_dir(self.instance_dir(id)?.join("game"))
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
        let secrets = Arc::new(vec![session.access_token, session.refresh_token]);
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
        let game = launcher.instance_dir(&instance.id).unwrap().join("game");
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"save-fixture"
        );
        assert!(!game.join("launcher_accounts.json").exists());
        assert!(source.join("launcher_accounts.json").exists());
        assert!(!instance.installed);
    }

    #[tokio::test]
    async fn instance_lifecycle_isolated_and_recoverable() {
        let t = tempfile::tempdir().unwrap();
        let l = Launcher::open(Some(t.path().into())).unwrap();
        let a = l.create_instance("世界 A", "1.21.1").unwrap();
        fs::write(
            l.instance_dir(&a.id).unwrap().join("game/options.txt"),
            b"volume:1",
        )
        .unwrap();
        let b = l.clone_instance(&a.id).await.unwrap();
        assert_ne!(a.id, b.id);
        fs::write(
            l.instance_dir(&b.id).unwrap().join("game/options.txt"),
            b"volume:0",
        )
        .unwrap();
        assert_eq!(
            fs::read(l.instance_dir(&a.id).unwrap().join("game/options.txt")).unwrap(),
            b"volume:1"
        );
        l.delete_instance(&b.id).unwrap();
        assert_eq!(l.snapshot().database.instances.len(), 1);
        assert_eq!(fs::read_dir(t.path().join("trash")).unwrap().count(), 1);
        assert!(l.instance_dir("../outside").is_err());
        assert!(Launcher::open(Some(t.path().into())).is_err());
    }
}
