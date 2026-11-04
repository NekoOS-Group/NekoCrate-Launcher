use super::{Handler, Outcome, Plan, Step, StepFuture, game::Payload};
use crate::{
    Launcher, download::Downloader, fabric, java::JavaInfo, minecraft, model::*, storage, version,
};
use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub(crate) async fn install(
    launcher: &Launcher,
    id: &str,
    instance: Instance,
    game: PathBuf,
    repair: bool,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<bool> {
    sink(Event::workflow("metadata", "解析游戏版本"));
    let selected = instance
        .selected()
        .context("请先为此实例添加并选择一个版本")?;
    crate::download::check_cancel(cancel)?;
    let local: Option<serde_json::Value> =
        storage::read_json(&version::metadata_path(&game, &selected.id)?).ok();
    let fabric = local
        .as_ref()
        .and_then(version::detect_loader)
        .is_some_and(|l| l.kind == "fabric");
    // Reject unsupported imported profiles before constructing the network
    // client or writing in somebody else's version directory.
    if !fabric {
        minecraft::check_local_install(&game, &selected.id)?;
    }
    let plan = Plan(&[
        Step::silent("metadata"),
        Step::silent("payload"),
        Step::new("finalize", "保存安装信息"),
    ]);
    let settings = launcher.settings();
    let downloader = Downloader::new(launcher.root(), settings.clone())?.for_installation(
        launcher.root(),
        id,
        &game,
        &selected.id,
        "game",
    )?;
    let downloader = if repair {
        downloader.with_full_verification()
    } else {
        downloader
    };
    let mut task = Official {
        launcher,
        id,
        instance,
        game,
        downloader,
        cancel,
        sink,
        auto_java: settings.auto_java,
        version: None,
        runtime: None,
        fabric,
        local,
    };
    plan.run(&mut task, cancel, sink).await?;
    task.downloader.finish_installation(sink).await;
    sink(Event::new("complete", "游戏文件与 Java 已准备好"));
    Ok(true)
}

struct Official<'a> {
    launcher: &'a Launcher,
    id: &'a str,
    instance: Instance,
    game: PathBuf,
    downloader: Downloader,
    cancel: &'a CancellationToken,
    sink: &'a EventSink,
    auto_java: bool,
    version: Option<Version>,
    runtime: Option<JavaInfo>,
    fabric: bool,
    local: Option<serde_json::Value>,
}
impl Handler for Official<'_> {
    fn checkpoint(&mut self, step: Step) -> Result<()> {
        if step.id == "finalize" {
            let selected = self.instance.selected().context("游戏版本不存在")?;
            self.launcher.set_installed(self.id, &selected.id, true)?;
        }
        Ok(())
    }
    fn execute<'a>(&'a mut self, step: Step) -> StepFuture<'a> {
        Box::pin(async move {
            match step.id {
                "metadata" => self.metadata().await?,
                "payload" => self.payload().await?,
                "finalize" => self.commit().await?,
                _ => anyhow::bail!("未知官方版本安装步骤"),
            }
            Ok(Outcome::Complete)
        })
    }
}
impl Official<'_> {
    async fn metadata(&mut self) -> Result<()> {
        let selected = self.instance.selected().context("游戏版本不存在")?;
        let path = version::metadata_path(&self.game, &selected.id)?;
        let v = if self.fabric {
            (self.sink)(Event::new("stage", "解析 Fabric 版本"));
            let mut profile = self.local.take().context("Fabric 配置缺失")?;
            fabric::prepare_libraries(&mut profile, &self.downloader, self.cancel).await?;
            storage::write_json(&path, &profile)?;
            let parent_id = profile["inheritsFrom"]
                .as_str()
                .context("Fabric 配置缺少基础版本")?;
            if !version::metadata_path(&self.game, parent_id)?.is_file() {
                let parent = minecraft::version(
                    self.launcher.root(),
                    parent_id,
                    &self.downloader,
                    self.cancel,
                    self.sink,
                )
                .await?;
                storage::write_json(&version::metadata_path(&self.game, parent_id)?, &parent)?;
            }
            version::resolve(self.launcher.root(), &self.game, &selected.id)?.version
        } else {
            (self.sink)(Event::new("stage", "获取官方版本元数据"));
            minecraft::version(
                self.launcher.root(),
                &selected.id,
                &self.downloader,
                self.cancel,
                self.sink,
            )
            .await?
        };
        self.launcher.set_installed(self.id, &selected.id, false)?;
        self.version = Some(v);
        Ok(())
    }

    async fn payload(&mut self) -> Result<()> {
        let selected = self.instance.selected().context("游戏版本不存在")?;
        let progress = Arc::new(Mutex::new((
            selected.download_progress.min(99.0),
            std::time::Instant::now(),
        )));
        let captured = progress.clone();
        let launcher = self.launcher.clone();
        let instance_id = self.id.to_owned();
        let version_id = selected.id.clone();
        let forward = Event::scoped(self.sink, self.id, Some("files"));
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
        let result = Payload {
            root: self.launcher.root(),
            game: &self.game,
            working: &version::instance_working_directory(
                &self.game,
                &self.instance,
                &selected.id,
            )?,
            version: self.version.as_ref().context("游戏版本尚未解析")?,
            explicit_java: self.instance.java_path.as_deref(),
            auto_java: self.auto_java,
            downloader: &self.downloader,
            cancel: self.cancel,
            sink: self.sink,
            files_sink: &file_sink,
        }
        .run()
        .await;
        self.launcher
            .save_download_progress(self.id, &selected.id, progress.lock().unwrap().0)?;
        self.runtime = Some(result?);
        Ok(())
    }

    async fn commit(&self) -> Result<()> {
        let selected = self.instance.selected().context("游戏版本不存在")?;
        let directory = self.game.join("versions").join(&selected.id);
        if !self.fabric {
            storage::write_json(
                &version::metadata_path(&self.game, &selected.id)?,
                self.version.as_ref().context("游戏版本尚未解析")?,
            )?;
        }
        storage::write_json(
            &directory.join("runtime.json"),
            self.runtime.as_ref().context("Java 尚未准备好")?,
        )?;
        Ok(())
    }
}
