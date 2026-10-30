//! Resolve metadata and feed one live download queue; manual files never stop it.
use super::*;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

pub(super) struct ContentInstall<'a> {
    pub launcher: &'a Launcher,
    pub id: &'a str,
    pub selected: &'a str,
    pub game: &'a Path,
    pub target: &'a Path,
    pub downloader: &'a Downloader,
    pub cancel: &'a CancellationToken,
    pub sink: &'a EventSink,
}
impl ContentInstall<'_> {
    pub async fn run(&self, job: &mut Job, key: Option<String>) -> Result<()> {
        let sink = Event::scoped(self.sink, self.id, Some("pack_files"));
        let overridden = job.overridden.clone().unwrap_or_default();
        let total = job.pack.files.len();
        let mut mirrors = HashMap::new();
        for file in &job.pack.files {
            if let Some(path) = &file.path {
                mirrors.insert(
                    archive::installed_destination(self.target, path)?,
                    file.urls.clone(),
                );
            }
        }
        let downloader =
            self.downloader
                .clone()
                .with_mirrors(mirrors)
                .with_cache(archive::managed_destination(
                    self.target,
                    ".nekocrate/downloads",
                )?);
        let api = crate::network::client_builder(self.launcher.root(), &self.launcher.settings())?
            .redirect(reqwest::redirect::Policy::none())
            // Large concurrent CF bulk replies stalled on HTTP/2 through a
            // local proxy; HTTP/1.1 completes reliably. Only metadata uses it.
            .http1_only()
            .build()?;
        let (pack_tx, pack_rx) = mpsc::channel::<Vec<PackFile>>(2);
        let (file_tx, file_rx) = mpsc::channel(2);
        let mut checkpoint = job.clone();
        let resolver = async {
            let sender = pack_tx;
            provider::resolve(
                &mut job.pack,
                key,
                &api,
                self.cancel,
                &sink,
                Some(&sender),
                |pack| {
                    checkpoint.pack = pack.clone();
                    record::write_job(self.game, self.selected, &checkpoint)
                },
            )
            .await
        };
        let relay = async {
            let mut batches = pack_rx;
            let sender = file_tx;
            let mut targets = HashSet::new();
            while let Some(batch) = batches.recv().await {
                crate::download::check_cancel(self.cancel)?;
                let mut files = Vec::new();
                let mut manual = false;
                for file in batch {
                    let relative = file.path.as_deref().context("整合包文件尚未解析")?;
                    let path = archive::installed_destination(self.target, relative)?;
                    if !targets.insert(path.to_string_lossy().to_lowercase()) {
                        bail!("整合包有多个文件写入同一位置");
                    }
                    // The archive will provide these files in the overrides step.
                    if overridden.contains(&relative.to_lowercase()) {
                        continue;
                    }
                    if file.urls.is_empty() {
                        if !Downloader::verified(&path, &file.sha1, false, file.size).await? {
                            manual = true;
                        }
                        continue;
                    }
                    files.push(FileSpec {
                        url: file.urls[0].clone(),
                        path,
                        hash: file.sha1,
                        sha256: false,
                        size: file.size,
                    });
                }
                if manual {
                    let mut status = self.launcher.modpack_status(self.id, self.selected).await?;
                    status.missing.retain(|file| {
                        !file
                            .path
                            .as_ref()
                            .is_some_and(|path| overridden.contains(&path.to_lowercase()))
                    });
                    sink(Event {
                        pack_missing: Some(status),
                        ..Event::new("pack_manual", "部分文件需手动导入，其余文件继续下载")
                    });
                }
                if !files.is_empty() {
                    downloader
                        .import_cached_files(self.launcher.root(), &files, self.cancel)
                        .await?;
                    tokio::select! {
                        _ = self.cancel.cancelled() => bail!("操作已取消"),
                        result = sender.send(files) => result.context("整合包下载队列已关闭")?,
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        };
        let launcher = self.launcher.clone();
        let instance = self.id.to_owned();
        let version = self.selected.to_owned();
        let forward = sink.clone();
        let recovered = self
            .launcher
            .instance(self.id)?
            .versions
            .iter()
            .find(|v| v.id == self.selected)
            .map(|v| v.download_progress)
            .unwrap_or(0.0)
            .clamp(0.0, 99.0);
        let state = Arc::new(std::sync::Mutex::new((
            recovered,
            std::time::Instant::now(),
        )));
        let progress_state = state.clone();
        let progress: EventSink = Arc::new(move |mut event| {
            if event.kind == "progress" && total > 0 {
                // The manifest's fixed file count includes not-yet-resolved entries.
                // Growing the queue must not report 100% for its first small batch.
                let mut state = progress_state.lock().unwrap();
                state.0 = state
                    .0
                    .max((event.completed as f64 / total as f64 * 70.0).min(70.0));
                event.overall_progress = Some(state.0);
                if state.1.elapsed().as_secs_f32() >= 1.0 {
                    let _ = launcher.save_download_progress(&instance, &version, state.0);
                    state.1 = std::time::Instant::now();
                }
            }
            forward(event);
        });
        // These futures are polled together (not detached tasks), so pausing the
        // installer also pauses resolution and the live downloads consistently.
        let (resolved, relayed, downloaded) = tokio::join!(
            resolver,
            relay,
            downloader.batch_stream(file_rx, self.cancel, &progress)
        );
        self.launcher
            .save_download_progress(self.id, self.selected, state.lock().unwrap().0)?;
        downloaded?;
        relayed?;
        resolved?;
        for file in &job.pack.files {
            if let Some(path) = &file.path
                && !file.sha512.is_empty()
                && !overridden.contains(&path.to_lowercase())
            {
                let destination = archive::installed_destination(self.target, path)?;
                if file.urls.is_empty()
                    && !Downloader::verified(&destination, &file.sha1, false, file.size).await?
                {
                    continue;
                }
                verify_sha512(&destination, &file.sha512, self.cancel).await?;
            }
        }
        Ok(())
    }
}
