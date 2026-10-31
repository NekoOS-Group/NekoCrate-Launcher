//! Loader installations shared by standalone profiles and imported modpacks.
use crate::{Launcher, download::Downloader, model::*, storage, version};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
mod catalog;
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
        self.idle(id)?;
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
        self.idle(id)?;
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
        sink(Event::workflow("metadata", "解析游戏版本"));
        let state = Arc::new(Mutex::new((record.clone(), std::time::Instant::now())));
        let captured = state.clone();
        let forward = sink.clone();
        let launcher = self.clone();
        let instance_id = id.to_owned();
        let version_id = selected.to_owned();
        let game_copy = game.clone();
        let progress: EventSink = Arc::new(move |mut event| {
            let mut state = captured.lock().unwrap();
            let step = event.step.as_deref().unwrap_or("metadata");
            let start = match step {
                "loader" => 5.0,
                "files" => 45.0,
                "natives" => 90.0,
                "java" => 95.0,
                _ => 0.0,
            };
            let span = match step {
                "loader" => 40.0,
                "files" => 45.0,
                _ => 0.0,
            };
            let fraction = if event.total_bytes > 0 {
                event.bytes as f64 / event.total_bytes as f64
            } else if event.total > 0 {
                event.completed as f64 / event.total as f64
            } else {
                0.0
            };
            state.0.progress = state
                .0
                .progress
                .max(start + span * fraction.clamp(0.0, 1.0))
                .min(99.0);
            event.overall_progress = Some(state.0.progress);
            if state.1.elapsed().as_secs_f32() >= 1.0 || event.kind == "workflow" {
                let saved = record_path(&game_copy, &version_id)
                    .and_then(|p| storage::write_json(&p, &state.0))
                    .and_then(|_| {
                        launcher.save_download_progress(&instance_id, &version_id, state.0.progress)
                    });
                if let Err(error) = saved {
                    forward(Event::new("warning", format!("保存下载进度失败：{error}")));
                }
                state.1 = std::time::Instant::now();
            }
            drop(state);
            forward(event);
        });
        let result = installer::Install {
            root: self.root(),
            game: &game,
            working_directory: &version::instance_working_directory(&game, &instance, selected)?,
            minecraft: &record.minecraft,
            loader: Some(&record.loader),
            id: selected,
            explicit_java: instance.java_path.as_deref(),
            downloader: &downloader,
            cancel,
            sink: &progress,
            auto_java: settings.auto_java,
        }
        .run()
        .await;
        record.progress = state.lock().unwrap().0.progress;
        storage::write_json(&record_path(&game, selected)?, &record)?;
        self.save_download_progress(id, selected, record.progress)?;
        let profile = result?;
        crate::download::check_cancel(cancel)?;
        sink(Event::workflow("finalize", "完成安装"));
        storage::write_json(&version::metadata_path(&game, selected)?, &profile)?;
        record.completed = true;
        record.progress = 100.0;
        storage::write_json(&record_path(&game, selected)?, &record)?;
        self.set_installed(id, selected, true)?;
        downloader.finish_installation(&sink).await;
        sink(Event::new("complete", "游戏与加载器已安装"));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
