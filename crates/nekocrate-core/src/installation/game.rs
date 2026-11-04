//! Shared game payload and native/resource extraction plan.
use crate::{
    download::Downloader,
    minecraft,
    model::{AssetIndex, Event, EventSink, Version},
    storage,
};
use anyhow::Result;
use std::path::Path;
use tokio_util::sync::CancellationToken;

/// Used by vanilla, Fabric, Forge/NeoForge and modpack installations. Launch
/// profile publication belongs to the enclosing plan (after any manual files).
pub(crate) struct Payload<'a> {
    pub root: &'a Path,
    pub game: &'a Path,
    pub working: &'a Path,
    pub version: &'a Version,
    pub explicit_java: Option<&'a str>,
    pub auto_java: bool,
    pub downloader: &'a Downloader,
    pub cancel: &'a CancellationToken,
    pub sink: &'a EventSink,
    pub files_sink: &'a EventSink,
}
impl Payload<'_> {
    #[inline(never)]
    pub fn run(&self) -> futures_util::future::BoxFuture<'_, Result<crate::java::JavaInfo>> {
        Box::pin(async move {
            (self.sink)(Event::workflow("files", "下载与校验游戏文件"));
            files(
                self.game,
                self.version,
                self.downloader,
                self.cancel,
                self.files_sink,
            )
            .await?;
            (self.sink)(Event::workflow("java", "准备 Java"));
            let forward = self.sink.clone();
            let sink: EventSink = std::sync::Arc::new(move |mut event| {
                if event.step.is_none() {
                    event.step = Some("java".into());
                }
                forward(event);
            });
            crate::java::ensure(
                self.root,
                crate::java::JavaRequirement::for_working_directory(
                    self.version,
                    self.working,
                    self.explicit_java,
                )
                .await?,
                self.explicit_java,
                self.auto_java,
                self.downloader,
                self.cancel,
                &sink,
            )
            .await
        })
    }
}

/// The verified-file index already records every completed download. An extra
/// index/files/natives journal cannot safely authorize skipping this validation.
// One polling state machine for launch preparation and all installation paths.
#[inline(never)]
pub(crate) fn files<'a>(
    game: &'a Path,
    version: &'a Version,
    downloader: &'a Downloader,
    cancel: &'a CancellationToken,
    sink: &'a EventSink,
) -> futures_util::future::BoxFuture<'a, Result<()>> {
    Box::pin(async move {
        let file = minecraft::asset_index_file(game, version)?;
        downloader.file(&file, cancel, sink).await?;
        let index: AssetIndex = storage::read_json(&file.path)?;
        let files = minecraft::installation_files(game, version, &index)?;
        sink(Event::new(
            "stage",
            format!("校验并下载 {} 个游戏文件", files.len()),
        ));
        downloader.batch(files, cancel, sink).await?;
        sink(Event::workflow("natives", "准备原生库与资源映射"));
        let game = game.to_owned();
        let version = version.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            minecraft::prepare_runtime_files(&game, &version, &index, &cancel)
        })
        .await?
    })
}
