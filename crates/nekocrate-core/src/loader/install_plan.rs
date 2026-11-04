use super::*;
use crate::installation::{Handler, Outcome, Plan, Step, StepFuture};
use std::sync::{Arc, Mutex};

pub(super) struct LoaderInstall<'a> {
    pub launcher: &'a Launcher,
    pub id: &'a str,
    pub selected: &'a str,
    pub game: PathBuf,
    pub record: InstallRecord,
    pub instance: Instance,
    pub downloader: Downloader,
    pub auto_java: bool,
    pub cancel: &'a CancellationToken,
    pub sink: &'a EventSink,
    pub profile: Option<serde_json::Value>,
}
impl LoaderInstall<'_> {
    pub async fn run(mut self) -> Result<()> {
        let plan = Plan(&[
            Step::new("metadata", "解析游戏版本"),
            Step::new("finalize", "完成安装"),
        ]);
        let cancel = self.cancel;
        let sink = self.sink;
        plan.run(&mut self, cancel, sink).await?;
        self.launcher.set_installed(self.id, self.selected, true)?;
        self.downloader.finish_installation(sink).await;
        sink(Event::new("complete", "游戏与加载器已安装"));
        Ok(())
    }
    async fn prepare(&mut self) -> Result<()> {
        let launcher = self.launcher;
        let id = self.id;
        let selected = self.selected;
        let game = &self.game;
        let record = &mut self.record;
        let instance = &self.instance;
        let downloader = &self.downloader;
        let cancel = self.cancel;
        let sink = self.sink;
        let state = Arc::new(Mutex::new((record.clone(), std::time::Instant::now())));
        let captured = state.clone();
        let forward = sink.clone();
        let progress_launcher = launcher.clone();
        let instance_id = id.to_owned();
        let version_id = selected.to_owned();
        let game_copy = game.to_owned();
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
                        progress_launcher.save_download_progress(
                            &instance_id,
                            &version_id,
                            state.0.progress,
                        )
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
            root: launcher.root(),
            game,
            working_directory: &version::instance_working_directory(game, instance, selected)?,
            minecraft: &record.minecraft,
            loader: Some(&record.loader),
            id: selected,
            explicit_java: instance.java_path.as_deref(),
            downloader,
            cancel,
            sink: &progress,
            auto_java: self.auto_java,
        }
        .run()
        .await;
        record.progress = state.lock().unwrap().0.progress;
        storage::write_json(&record_path(game, selected)?, &record)?;
        launcher.save_download_progress(id, selected, record.progress)?;

        self.profile = Some(result?);
        Ok(())
    }
    async fn publish(&mut self) -> Result<()> {
        let profile = self.profile.as_ref().context("加载器启动配置尚未准备好")?;
        storage::write_json(&version::metadata_path(&self.game, self.selected)?, profile)?;
        Ok(())
    }
}
impl Handler for LoaderInstall<'_> {
    fn checkpoint(&mut self, step: Step) -> Result<()> {
        if step.id == "finalize" {
            self.record.completed = true;
            self.record.progress = 100.0;
            storage::write_json(&record_path(&self.game, self.selected)?, &self.record)?;
        }
        Ok(())
    }
    fn execute<'a>(&'a mut self, step: Step) -> StepFuture<'a> {
        Box::pin(async move {
            match step.id {
                "metadata" => self.prepare().await?,
                "finalize" => self.publish().await?,
                _ => bail!("未知加载器安装步骤"),
            }
            Ok(Outcome::Complete)
        })
    }
}
