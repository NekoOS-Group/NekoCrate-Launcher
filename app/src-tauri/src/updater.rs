use anyhow::{Context, Result, bail};
use nekocrate_core::{Launcher, updates};
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    Idle,
    Checking,
    Downloading,
    Verifying,
    Ready,
    Installing,
    Restarting,
    Error,
}

#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub stage: Stage,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub version: Option<String>,
    pub error: Option<String>,
}

struct Package {
    update: Update,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Session {
    progress: Progress,
    cancel: Option<CancellationToken>,
    package: Option<Package>,
}

#[derive(Default)]
pub struct UpdateManager(Mutex<Session>);

#[derive(Serialize)]
pub struct Status {
    #[serde(flatten)]
    release: updates::UpdateStatus,
    install_support: Option<&'static str>,
}

fn unsupported_reason() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("development");
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_none() {
        return Some("package_manager");
    }
    #[cfg(target_os = "macos")]
    if !std::env::current_exe()
        .ok()
        .is_some_and(|path| path.parent().is_some_and(|p| p.ends_with("Contents/MacOS")))
    {
        return Some("unbundled");
    }
    None
}

pub async fn check(launcher: &Launcher) -> Result<Status> {
    Ok(Status {
        release: updates::check(launcher.root(), &launcher.settings()).await?,
        install_support: unsupported_reason(),
    })
}

impl UpdateManager {
    pub fn progress(&self) -> Progress {
        self.0.lock().unwrap().progress.clone()
    }

    pub fn installing(&self) -> bool {
        matches!(self.progress().stage, Stage::Installing | Stage::Restarting)
    }

    fn publish(&self, app: &AppHandle, change: impl FnOnce(&mut Progress)) {
        let progress = {
            let mut session = self.0.lock().unwrap();
            change(&mut session.progress);
            session.progress.clone()
        };
        let _ = app.emit("launcher-update", progress);
    }

    pub fn cancel(&self) {
        if let Some(cancel) = &self.0.lock().unwrap().cancel {
            cancel.cancel();
        }
    }

    pub async fn download(&self, app: &AppHandle, launcher: &Launcher, tag: &str) -> Result<()> {
        if unsupported_reason().is_some() {
            bail!("当前运行方式不支持自动安装，请安装发布版或使用系统软件包管理器");
        }
        let cancel = CancellationToken::new();
        {
            let mut session = self.0.lock().unwrap();
            if session.cancel.is_some()
                || matches!(
                    session.progress.stage,
                    Stage::Installing | Stage::Restarting
                )
            {
                bail!("启动器更新正在进行中");
            }
            if session.package.is_some() && session.progress.version.as_deref() == Some(tag) {
                return Ok(());
            }
            session.package = None;
            session.cancel = Some(cancel.clone());
            session.progress = Progress {
                stage: Stage::Checking,
                version: Some(tag.into()),
                ..Progress::default()
            };
        }
        self.publish(app, |_| {});
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Ok(None),
            result = self.fetch(app, launcher, tag) => result.map(Some),
        };
        {
            let mut session = self.0.lock().unwrap();
            session.cancel = None;
            match &result {
                Ok(Some(_)) => session.progress.stage = Stage::Ready,
                Ok(None) => session.progress = Progress::default(),
                Err(error) => {
                    session.progress.stage = Stage::Error;
                    session.progress.error = Some(error.to_string());
                }
            }
            if let Ok(Some(package)) = result {
                session.package = Some(package);
            }
        }
        self.publish(app, |_| {});
        // Errors are in the persistent progress state as well as the RPC result.
        if let Some(error) = self.progress().error {
            bail!("{error}");
        }
        Ok(())
    }

    async fn fetch(&self, app: &AppHandle, launcher: &Launcher, tag: &str) -> Result<Package> {
        let settings = launcher.settings();
        let release = updates::check(launcher.root(), &settings).await?;
        if !release.available || release.latest_version != tag {
            bail!("更新版本已变化，请重新检查更新");
        }
        let endpoint = release.updater_url.context("该版本未提供应用内更新包")?;
        let proxy = if matches!(settings.proxy_mode.as_str(), "http" | "socks" | "custom") {
            let mut proxy = reqwest::Proxy::all(&settings.proxy_url).context("更新代理配置无效")?;
            let password = launcher.proxy_password()?;
            if !settings.proxy_username.is_empty() && !password.is_empty() {
                proxy = proxy.basic_auth(&settings.proxy_username, &password);
            }
            Some(proxy)
        } else {
            None
        };
        let no_proxy = settings.proxy_mode == "none" || proxy.is_some();
        let exiting_launcher = launcher.clone();
        let exiting_app = app.clone();
        let mut builder = app
            .updater_builder()
            .on_before_exit(move || {
                // Windows invokes this after the installer has started, before exiting.
                let _ = exiting_launcher.release_data_lock_for_restart();
                exiting_app.cleanup_before_exit();
            })
            .endpoints(vec![endpoint.parse()?])?
            .timeout(Duration::from_secs(20))
            .configure_client(move |builder| {
                let builder = builder
                    .connect_timeout(Duration::from_secs(10))
                    .read_timeout(Duration::from_secs(30));
                let builder = if no_proxy {
                    builder.no_proxy()
                } else {
                    builder
                };
                if let Some(proxy) = &proxy {
                    builder.proxy(proxy.clone())
                } else {
                    builder
                }
            });
        // HTTPS and signatures are enforced by the plugin; no insecure overrides.
        builder = builder.restart_after_install(true);
        let mut update = builder
            .build()?
            .check()
            .await
            .context("无法读取更新信息")?
            .context("更新版本已变化，请重新检查更新")?;
        if update.version != release.version
            || !updates::is_update_artifact(update.download_url.as_str(), tag)
        {
            bail!("更新清单与发布版本不一致");
        }
        update.timeout = Some(Duration::from_secs(30 * 60));
        self.publish(app, |p| p.stage = Stage::Downloading);
        let mut downloaded = 0u64;
        let mut last = Instant::now() - Duration::from_secs(1);
        let bytes = update
            .download(
                |chunk, total| {
                    downloaded += chunk as u64;
                    if last.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
                        self.publish(app, |p| {
                            p.downloaded = downloaded;
                            p.total = total;
                        });
                        last = Instant::now();
                    }
                },
                || self.publish(app, |p| p.stage = Stage::Verifying),
            )
            .await
            .context("更新包下载或签名验证失败，请重试")?;
        self.publish(app, |p| {
            p.downloaded = bytes.len() as u64;
            p.total = Some(bytes.len() as u64);
        });
        Ok(Package { update, bytes })
    }

    pub async fn install(&self, app: &AppHandle, launcher: &Launcher) -> Result<()> {
        let package = {
            let mut session = self.0.lock().unwrap();
            let package = session.package.take().context("请先下载启动器更新")?;
            session.progress.stage = Stage::Installing;
            session.progress.error = None;
            package
        };
        self.publish(app, |_| {});
        let installed = tauri::async_runtime::spawn_blocking(move || {
            let result = package.update.install(&package.bytes);
            (package, result)
        })
        .await;
        let (package, result) = match installed {
            Ok(result) => result,
            Err(_) => {
                self.publish(app, |p| {
                    p.stage = Stage::Error;
                    p.error = Some("更新安装失败，请重新下载更新".into());
                });
                bail!("更新安装失败，请重新下载更新");
            }
        };
        if result.is_err() {
            self.0.lock().unwrap().package = Some(package);
            self.publish(app, |p| {
                p.stage = Stage::Ready;
                p.error = Some("更新安装失败，请检查安装目录权限后重试".into());
            });
            bail!("更新安装失败，请检查安装目录权限后重试");
        }
        self.publish(app, |p| p.stage = Stage::Restarting);
        launcher.release_data_lock_for_restart()?;
        app.restart();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tauri_plugin_updater::{Updater, UpdaterExt};

    const PACKAGE: &[u8] = include_bytes!("../tests/fixtures/updater/test.app.tar.gz");
    const PUBLIC: &str = include_str!("../tests/fixtures/updater/test.pub");
    const SIGNATURE: &str = include_str!("../tests/fixtures/updater/test.app.tar.gz.sig");

    fn updater(path: &std::path::Path) -> Updater {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({ "pubkey": PUBLIC.trim(), "requireSignedVersion": true }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        app.updater_builder()
            .executable_path(path)
            .target("test")
            .endpoints(vec!["https://example.invalid/latest.json".parse().unwrap()])
            .unwrap()
            .build()
            .unwrap()
    }

    fn metadata(version: &str) -> serde_json::Value {
        json!({ "version": version, "platforms": { "test": {
            "url": "https://example.invalid/test.app.tar.gz", "signature": SIGNATURE.trim()
        }}})
    }

    #[test]
    fn signed_package_rejects_corruption_version_substitution_and_downgrades() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("Test.app/Contents/MacOS/test");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, b"installed version").unwrap();
        let updater = updater(&binary);
        let update = updater.restore_update(metadata("9.9.9")).unwrap().unwrap();
        update.verify(PACKAGE).unwrap();
        let mut corrupt = PACKAGE.to_vec();
        corrupt[0] ^= 1;
        assert!(update.install(&corrupt).is_err());
        assert_eq!(std::fs::read(&binary).unwrap(), b"installed version");
        let forged = updater.restore_update(metadata("9.9.10")).unwrap().unwrap();
        assert!(forged.verify(PACKAGE).is_err());
        assert!(updater.restore_update(metadata("0.0.1")).unwrap().is_none());
        let mut invalid = metadata("9.9.9");
        invalid["platforms"]["test"]["signature"] = json!("invalid");
        assert!(
            updater
                .restore_update(invalid)
                .unwrap()
                .unwrap()
                .verify(PACKAGE)
                .is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn replaces_a_disposable_app_bundle_without_touching_data() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("Test.app/Contents/MacOS/test");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, b"old executable").unwrap();
        let data = dir.path().join("launcher-data");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(
            data.join("database.json"),
            b"preserved accounts and instances",
        )
        .unwrap();
        let update = updater(&binary)
            .restore_update(metadata("9.9.9"))
            .unwrap()
            .unwrap();
        update.install(PACKAGE).unwrap();
        assert_eq!(std::fs::read(&binary).unwrap(), b"#!/bin/sh\nexit 0\n");
        assert_eq!(
            std::fs::read(data.join("database.json")).unwrap(),
            b"preserved accounts and instances"
        );
        assert!(
            binary
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("Info.plist")
                .is_file()
        );
    }
}
