//! Loader installations shared by standalone profiles and imported modpacks.
use crate::{Launcher, download::Downloader, model::*, storage, version};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;
mod catalog;
mod install_plan;
pub(crate) mod installer;
pub use catalog::LoaderVersion;

tokio::task_local! {
    static INSTALL_PAUSE: tokio::sync::watch::Receiver<bool>;
}
/// Downloads retain their futures while paused. Java processors additionally
/// observe this channel in a separate task so pausing actually stops the child.
pub async fn with_pause<T>(
    pause: tokio::sync::watch::Receiver<bool>,
    operation: impl std::future::Future<Output = T>,
) -> T {
    INSTALL_PAUSE.scope(pause, operation).await
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InstallRecord {
    pub version: String,
    pub minecraft: String,
    pub loader: ModLoader,
    pub completed: bool,
    pub progress: f64,
}

fn validate_kind(kind: &str) -> Result<()> {
    if !["fabric", "forge", "neoforge"].contains(&kind) {
        bail!("不支持此加载器的自动安装");
    }
    Ok(())
}
fn profile_id(game: &str, loader: &ModLoader) -> Result<String> {
    validate_kind(&loader.kind)?;
    crate::archive_support::component(game)?;
    crate::archive_support::component(&loader.version)?;
    let id = if loader.kind == "fabric" {
        crate::fabric::version_id(game, &loader.version)?
    } else {
        format!("{game}-{}-{}", loader.kind, loader.version)
    };
    storage::safe_version(&id)?;
    Ok(id)
}
pub(crate) fn record_path(game: &Path, id: &str) -> Result<PathBuf> {
    storage::safe_version(id)?;
    let mut path = game.to_owned();
    for part in ["versions", id, ".nekocrate", "loader.json"] {
        path.push(part);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("加载器安装路径包含符号链接");
        }
    }
    Ok(path)
}
pub(crate) fn has_record(game: &Path, id: &str) -> bool {
    game.join("versions")
        .join(id)
        .join(".nekocrate/loader.json")
        .exists()
}
pub(crate) fn read_record(game: &Path, id: &str) -> Result<InstallRecord> {
    let record: InstallRecord = storage::read_json(&record_path(game, id)?)?;
    if record.version != id || profile_id(&record.minecraft, &record.loader)? != id {
        bail!("加载器安装记录与版本目录不匹配");
    }
    Ok(record)
}
pub(crate) fn describe(game: &Path, id: &str) -> Result<InstanceVersion> {
    let record = read_record(game, id)?;
    let installed = record.completed && version::metadata_path(game, id)?.is_file();
    Ok(InstanceVersion {
        id: id.into(),
        minecraft_version: Some(record.minecraft),
        loader: Some(record.loader),
        installed,
        download_progress: if installed {
            100.0
        } else {
            record.progress.clamp(0.0, 99.0)
        },
        ..Default::default()
    })
}

impl Launcher {
    pub async fn loader_versions(
        &self,
        game: &str,
        kind: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<LoaderVersion>> {
        catalog::versions(
            self.root(),
            game,
            kind,
            &Downloader::new(self.root(), self.settings())?,
            cancel,
        )
        .await
    }

    pub async fn add_loader_version(
        &self,
        id: &str,
        game_version: &str,
        loader: ModLoader,
        cancel: &CancellationToken,
    ) -> Result<Instance> {
        let _guard = self.0.operation.lock().await;
        let _directory_guard = self.idle(id)?;
        let game = self.available_game_dir(id)?;
        let profile = profile_id(game_version, &loader)?;
        let directory = game.join("versions").join(&profile);
        // A retry selects the same reserved installation; unrelated profiles are never overwritten.
        if directory.exists() {
            read_record(&game, &profile)
                .context("同名版本已经存在，请保留原版本或选择其他加载器版本")?;
        } else {
            if !self
                .loader_versions(game_version, &loader.kind, cancel)
                .await?
                .iter()
                .any(|v| v.version == loader.version)
            {
                bail!("所选加载器不支持此游戏版本，请重新选择");
            }
            crate::download::check_cancel(cancel)?;
            record_path(&game, &profile)?;
            std::fs::create_dir_all(game.join("versions"))?;
            std::fs::create_dir(&directory).context("同名版本已经存在，未覆盖原目录")?;
            let result = (|| -> Result<()> {
                // Directories establish isolation using the existing working-directory resolver.
                for name in ["mods", "config", "saves"] {
                    std::fs::create_dir(directory.join(name))?;
                }
                storage::write_json(
                    &record_path(&game, &profile)?,
                    &InstallRecord {
                        version: profile.clone(),
                        minecraft: game_version.into(),
                        loader,
                        completed: false,
                        progress: 0.0,
                    },
                )
            })();
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&directory);
            }
            result?;
        }
        let mut entry = describe(&game, &profile)?;
        self.mutate(|db| {
            let instance = db
                .instances
                .iter_mut()
                .find(|i| i.id == id)
                .context("实例不存在")?;
            if let Some(existing) = instance.versions.iter_mut().find(|v| v.id == profile) {
                entry.name = existing.name.clone();
                *existing = entry;
            } else {
                instance.versions.push(entry);
            }
            instance.selected_version = Some(profile);
            Ok(instance.clone())
        })
    }

    pub(crate) async fn install_loader(
        &self,
        id: &str,
        selected: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
        repair: bool,
    ) -> Result<()> {
        let _guard = self.0.operation.lock().await;
        let _directory_guard = self.idle(id)?;
        let game = self.available_game_dir(id)?;
        let mut record = read_record(&game, selected)?;
        let instance = self.instance(id)?;
        let sink = Event::scoped(sink, id, None);
        crate::download::check_cancel(cancel)?;
        if version::metadata_path(&game, selected)?.is_file() {
            let resolved = version::resolve(self.root(), &game, selected)?;
            if resolved.loader.as_ref() != Some(&record.loader)
                || resolved.minecraft_version != record.minecraft
            {
                bail!("本地版本配置已改变，未覆盖已有加载器");
            }
        }
        record.completed = false;
        storage::write_json(&record_path(&game, selected)?, &record)?;
        self.set_installed(id, selected, false)?;
        let settings = self.settings();
        let downloader = Downloader::new(self.root(), settings.clone())?.for_installation(
            self.root(),
            id,
            &game,
            selected,
            "game",
        )?;
        let downloader = if repair {
            downloader.with_full_verification()
        } else {
            downloader
        };
        install_plan::LoaderInstall {
            launcher: self,
            id,
            selected,
            game,
            record,
            instance,
            downloader,
            auto_java: settings.auto_java,
            cancel,
            sink: &sink,
            profile: None,
        }
        .run()
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
