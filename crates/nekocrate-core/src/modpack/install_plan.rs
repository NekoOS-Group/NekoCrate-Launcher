use super::*;
use crate::installation::{Handler, Outcome, Plan, Step, StepFuture};
use futures_util::future::BoxFuture;

pub(super) struct InstallPack<'a> {
    pub launcher: &'a Launcher,
    pub id: &'a str,
    pub selected: &'a str,
    pub game: &'a Path,
    pub target: &'a Path,
    pub instance: &'a Instance,
    pub job: &'a mut Job,
    pub settings: &'a Settings,
    pub downloader: &'a Downloader,
    pub cancel: &'a CancellationToken,
    pub sink: &'a EventSink,
    pub key: Option<String>,
    pub repair: bool,
}
impl InstallPack<'_> {
    pub async fn run(&mut self) -> Result<bool> {
        let plan = Plan(&[
            Step::silent("pack_files"),
            Step::new("pack_overrides", "写入整合包配置"),
            Step::new("pack_game", "安装游戏与加载器"),
            Step::silent("publish"),
        ]);
        let cancel = self.cancel;
        let sink = self.sink;
        if plan.run(self, cancel, sink).await? == Outcome::Waiting {
            return Ok(false);
        }
        self.launcher.set_installed(self.id, self.selected, true)?;
        self.downloader.finish_installation(sink).await;
        sink(Event::new("complete", "整合包已安装"));
        Ok(true)
    }
    async fn game(&mut self, restore: bool) -> Result<bool> {
        let pack_sink = self.sink.clone();
        let loader_sink: EventSink = Arc::new(move |mut event| {
            event.step = Some("pack_game".into());
            pack_sink(event);
        });
        let working = version::instance_working_directory(self.game, self.instance, self.selected)?;
        let installer = crate::loader::installer::Install {
            root: self.launcher.root(),
            game: self.game,
            working_directory: &working,
            minecraft: &self.job.pack.minecraft,
            loader: self.job.pack.loader.as_ref(),
            id: self.selected,
            explicit_java: self.instance.java_path.as_deref(),
            downloader: self.downloader,
            cancel: self.cancel,
            sink: &loader_sink,
            auto_java: self.settings.auto_java,
        };
        if restore {
            if !installer
                .restore(
                    self.job
                        .pending_profile
                        .as_ref()
                        .context("启动配置尚未准备好")?,
                )
                .await?
            {
                return Ok(false);
            }
        } else {
            self.job.pending_profile = Some(installer.run().await?);
        }
        crate::download::check_cancel(self.cancel)?;
        (self.sink)(Event::workflow("java", "Java 已准备好"));
        Ok(true)
    }
}
impl Handler for InstallPack<'_> {
    fn restore<'a>(&'a mut self, step: Step) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            match step.id {
                "pack_files" if self.job.files_done && !self.repair => {
                    for file in &self.job.pack.files {
                        let Some(path) = &file.path else {
                            return Ok(false);
                        };
                        if self.job.overridden.as_ref().is_some_and(|paths| paths.contains(&path.to_lowercase()))
                            // Manual dependencies are checked at publication. Missing ones
                            // must not restart successful resolution/download work.
                            || file.urls.is_empty()
                        {
                            continue;
                        }
                        let spec = FileSpec {
                            path: archive::installed_destination(self.target, path)?,
                            hash: file.sha1.clone(),
                            sha256: false,
                            size: file.size,
                            url: String::new(),
                        };
                        if !self
                            .downloader
                            .verify(&spec.path, &spec, self.cancel)
                            .await?
                        {
                            return Ok(false);
                        }
                    }
                    self.launcher
                        .save_download_progress(self.id, self.selected, 70.0)?;
                    Ok(true)
                }
                // Overrides are user-owned after extraction, even during repair.
                "pack_overrides" if self.job.overrides_done => {
                    self.launcher
                        .save_download_progress(self.id, self.selected, 80.0)?;
                    Ok(true)
                }
                "pack_game" if self.job.pending_profile.is_some() && !self.repair => {
                    self.game(true).await
                }
                _ => Ok(false),
            }
        })
    }
    fn checkpoint(&mut self, step: Step) -> Result<()> {
        match step.id {
            "pack_files" => self.job.files_done = true,
            "pack_overrides" => self.job.overrides_done = true,
            "pack_game" => {} // The prepared launch profile is the checkpoint data.
            "publish" => {
                self.job.completed = true;
                self.job.pending_profile = None;
            }
            _ => bail!("未知整合包安装步骤"),
        }
        record::write_job(self.game, self.selected, self.job)
    }
    fn execute<'a>(&'a mut self, step: Step) -> StepFuture<'a> {
        Box::pin(async move {
            match step.id {
                "pack_files" => {
                    install_files::ContentInstall {
                        launcher: self.launcher,
                        id: self.id,
                        selected: self.selected,
                        game: self.game,
                        target: self.target,
                        downloader: self.downloader,
                        cancel: self.cancel,
                        sink: self.sink,
                    }
                    .run(self.job, self.key.clone())
                    .await?;
                    self.launcher
                        .save_download_progress(self.id, self.selected, 70.0)?;
                }
                "pack_overrides" => {
                    let source = self.job.archive.clone();
                    let pack = self.job.pack.clone();
                    let out = self.target.to_owned();
                    let token = self.cancel.clone();
                    tokio::task::spawn_blocking(move || {
                        archive::extract(&source, &pack, &out, &token)
                    })
                    .await??;
                    self.launcher
                        .save_download_progress(self.id, self.selected, 80.0)?;
                }
                "pack_game" => {
                    self.game(false).await?;
                }
                "publish" => {
                    // Wait for concurrent manual imports and their final hash checks.
                    let _manual = self.launcher.0.pack_import.lock().await;
                    let status = self.launcher.modpack_status(self.id, self.selected).await?;
                    if !status.missing.is_empty() {
                        (self.sink)(Event {
                            pack_missing: Some(status),
                            step: Some("pack_files".into()),
                            ..Event::new("pack_manual", "自动安装已完成，等待手动导入文件")
                        });
                        return Ok(Outcome::Waiting);
                    }
                    (self.sink)(Event::workflow("finalize", "完成整合包安装"));
                    storage::write_json(
                        &version::metadata_path(self.game, self.selected)?,
                        self.job
                            .pending_profile
                            .as_ref()
                            .context("启动配置尚未准备好")?,
                    )?;
                }
                _ => bail!("未知整合包安装步骤"),
            }
            Ok(Outcome::Complete)
        })
    }
}
