use crate::model::{Event, EventSink, FileProgress, Settings};
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt, stream::FuturesUnordered};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex, OnceCell},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct FileSpec {
    pub url: String,
    pub path: PathBuf,
    pub hash: String,
    pub sha256: bool,
    pub size: u64,
}
impl FileSpec {
    fn progress(&self, status: &str, downloaded: u64, total: u64) -> FileProgress {
        FileProgress {
            error: None,
            retryable: false,
            id: self.path.to_string_lossy().into_owned(),
            name: self
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            downloaded,
            total,
            status: status.into(),
            cached: false,
        }
    }
    fn emit(&self, sink: &EventSink, status: &str, downloaded: u64, total: u64) {
        sink(Event {
            files: vec![self.progress(status, downloaded, total)],
            ..Event::new("file", "")
        });
    }
    fn complete(&self, sink: &EventSink, total: u64, cached: bool) {
        let mut progress = self.progress("done", total, total);
        progress.cached = cached;
        sink(Event {
            files: vec![progress],
            ..Event::new("file", "")
        });
    }
}

/// Trusted retry handles remain in the backend; the UI only submits a listed file ID.
#[derive(Clone)]
pub struct RetryFile {
    downloader: Downloader,
    file: FileSpec,
}
impl RetryFile {
    pub fn id(&self) -> String {
        self.file.path.to_string_lossy().into_owned()
    }
    pub async fn run(&self, cancel: &CancellationToken, sink: &EventSink) -> Result<()> {
        self.downloader
            .batch(vec![self.file.clone()], cancel, sink)
            .await
    }
}
pub struct DownloadFailures {
    pub files: Vec<RetryFile>,
    message: String,
}
impl std::fmt::Display for DownloadFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::fmt::Debug for DownloadFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for DownloadFailures {}

#[derive(Clone)]
pub struct Downloader {
    pub client: reqwest::Client,
    network_root: Arc<PathBuf>,
    stall_policy: crate::download_stall::StallPolicy,
    cache: PathBuf,
    settings: Settings,
    metrics: Arc<TransferMetrics>,
    sources: Arc<Mutex<std::collections::HashMap<String, Arc<OnceCell<usize>>>>>,
    locks: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>>,
    verification: Arc<crate::verification::VerificationCache>,
    full_verification: bool,
    mirrors: Arc<std::collections::HashMap<PathBuf, Vec<String>>>,
    curseforge: Option<(reqwest::Client, reqwest::header::HeaderValue)>,
    task: Option<Arc<crate::download_tasks::DownloadTask>>,
}
#[derive(Default)]
struct TransferMetrics {
    bytes: AtomicU64,
    retries: AtomicUsize,
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn partial_size(path: &Path) -> Result<u64> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => bail!("无效下载目标"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn range_total(response: &reqwest::Response, offset: u64, expected: u64) -> Result<u64> {
    let range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("bytes "))
        .and_then(|header| header.split_once('/'));
    if let Some((interval, total)) = range
        && let Some((start, end)) = interval.split_once('-')
        && let (Ok(start), Ok(end), Ok(total)) = (
            start.parse::<u64>(),
            end.parse::<u64>(),
            total.parse::<u64>(),
        )
        && start == offset
        && end >= start
        && end.checked_add(1) == Some(total)
        && (expected == 0 || total == expected)
        && response
            .content_length()
            .is_none_or(|length| length == total - start)
    {
        return Ok(total);
    }
    bail!("下载源返回的续传范围无效")
}

struct BatchProgress {
    files: std::collections::HashMap<String, (u64, u64, bool)>,
    bytes: u64,
    total_bytes: u64,
    completed: usize,
    last: std::time::Instant,
}
struct FinalProgress {
    sink: EventSink,
    state: Arc<std::sync::Mutex<BatchProgress>>,
}
impl Drop for FinalProgress {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let event = self.state.lock().unwrap().event();
        (self.sink)(event);
    }
}
impl BatchProgress {
    fn new(files: &[FileProgress]) -> Self {
        Self {
            files: files
                .iter()
                .map(|file| {
                    (
                        file.id.clone(),
                        (file.downloaded, file.total, file.status == "done"),
                    )
                })
                .collect(),
            bytes: files.iter().map(|file| file.downloaded).sum(),
            total_bytes: files.iter().map(|file| file.total).sum(),
            completed: files.iter().filter(|file| file.status == "done").count(),
            last: std::time::Instant::now() - Duration::from_secs(1),
        }
    }
    fn update(&mut self, event: &Event) -> Option<Event> {
        let mut reset = false;
        for file in &event.files {
            let Some(previous) = self.files.get_mut(&file.id) else {
                continue;
            };
            self.bytes -= previous.0;
            self.total_bytes -= previous.1;
            self.completed -= usize::from(previous.2);
            let total = file.total.max(previous.1);
            let downloaded = if matches!(file.status.as_str(), "error" | "cancelled") {
                previous.0.max(file.downloaded)
            } else {
                file.downloaded
            };
            reset |= downloaded.min(total) < previous.0;
            *previous = (downloaded.min(total), total, file.status == "done");
            self.bytes += previous.0;
            self.total_bytes += previous.1;
            self.completed += usize::from(previous.2);
        }
        if reset
            || self.last.elapsed() >= Duration::from_millis(200)
            || self.completed == self.files.len()
        {
            self.last = std::time::Instant::now();
            Some(Event {
                reset_progress: reset,
                ..self.event()
            })
        } else {
            None
        }
    }
    fn event(&self) -> Event {
        Event {
            completed: self.completed,
            total: self.files.len(),
            bytes: self.bytes,
            total_bytes: self.total_bytes,
            ..Event::new("progress", "下载与校验游戏文件")
        }
    }
}

impl Downloader {
    pub fn new(root: &Path, settings: Settings) -> Result<Self> {
        Ok(Self {
            network_root: Arc::new(root.to_owned()),
            stall_policy: Default::default(),
            client: crate::network::client_builder(root, &settings)?
                .connect_timeout(Duration::from_secs(15))
                // Large game/JRE files may take minutes on slower networks.
                // Bound stalls rather than the total transfer duration.
                .read_timeout(Duration::from_secs(45))
                .build()?,
            cache: root.join("cache/objects"),
            metrics: Arc::default(),
            settings,
            locks: Arc::new(Mutex::new(Default::default())),
            sources: Arc::new(Mutex::new(Default::default())),
            verification: crate::verification::VerificationCache::shared(root),
            full_verification: false,
            mirrors: Arc::default(),
            curseforge: None,
            task: None,
        })
    }
    pub(crate) fn for_installation(
        mut self,
        root: &Path,
        instance: &str,
        game: &Path,
        version: &str,
        task: &str,
    ) -> Result<Self> {
        self.task = Some(crate::download_tasks::DownloadTask::new(
            root, instance, game, version, task,
        )?);
        Ok(self)
    }

    /// Call only after the complete installation transaction has been published.
    pub(crate) async fn finish_installation(&self, sink: &EventSink) {
        let Some(task) = &self.task else {
            return;
        };
        match task.complete().await {
            Ok(paths) => {
                for path in paths {
                    self.verification.forget(&path);
                }
                self.flush_verification(sink).await;
            }
            Err(error) => sink(Event::new(
                "warning",
                format!("清理安装暂存文件失败：{error:#}"),
            )),
        }
    }

    pub(crate) async fn index_installed(&self, file: &FileSpec) -> Result<()> {
        // Publication may rename/hard-link a staged file, changing its identity.
        if !self
            .verification
            .verify(
                &file.path,
                &file.hash,
                file.sha256,
                file.size,
                false,
                &CancellationToken::new(),
            )
            .await?
        {
            bail!("安装后的文件校验失败：{}", file.path.display());
        }
        Ok(())
    }

    pub(crate) async fn retain_until_installed(&self, paths: Vec<PathBuf>) -> Result<()> {
        if let Some(task) = &self.task {
            task.track_extra(paths).await?;
        }
        Ok(())
    }

    pub(crate) async fn forget_staging(&self, path: &Path) {
        self.verification.forget_tree(path).await;
    }

    pub(crate) async fn index_local(&self, path: &Path) -> Result<()> {
        self.verification.index_local(path).await
    }
    pub(crate) fn with_mirrors(
        mut self,
        mirrors: std::collections::HashMap<PathBuf, Vec<String>>,
    ) -> Self {
        self.mirrors = Arc::new(mirrors);
        self
    }
    /// Pack downloads keep resumable bytes beside the version, so the directory
    /// can be moved to another launcher without losing an interrupted transfer.
    pub(crate) fn with_cache(mut self, directory: PathBuf) -> Self {
        self.cache = directory;
        self
    }

    pub(crate) fn with_curseforge_key(
        mut self,
        root: &Path,
        settings: &Settings,
        key: &str,
    ) -> Result<Self> {
        let mut header = reqwest::header::HeaderValue::from_str(key)?;
        header.set_sensitive(true);
        self.curseforge = Some((
            crate::network::client_builder(root, settings)?
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(45))
                .build()?,
            header,
        ));
        Ok(self)
    }
    async fn file_response(&self, url: &str, offset: u64) -> Result<reqwest::Response> {
        let mut url = reqwest::Url::parse(url)?;
        for _ in 0..6 {
            let mut request = self
                .curseforge
                .as_ref()
                .map_or(&self.client, |(client, _)| client)
                .get(url.clone())
                .header(reqwest::header::ACCEPT_ENCODING, "identity");
            if offset > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            if let Some((_, header)) = &self.curseforge
                && url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("edge.forgecdn.net" | "mediafilez.forgecdn.net")
                )
            {
                request = request.header("x-api-key", header);
            }
            let response = request.send().await?;
            if self.curseforge.is_none() || !response.status().is_redirection() {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("下载跳转缺少地址")?
                .to_str()?;
            let next = url.join(location)?;
            if next.scheme() != "https" || !next.username().is_empty() || next.password().is_some()
            {
                bail!("拒绝不安全的下载跳转");
            }
            url = next;
        }
        bail!("下载跳转次数过多")
    }

    // Cloning reqwest::Client shares its connection pool. A stalled HTTP/2
    // stream can otherwise keep retrying on the very same bad connection.
    fn reconnect(&mut self) -> Result<()> {
        let client = |redirects| {
            crate::network::client_builder(&self.network_root, &self.settings)?
                .redirect(redirects)
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(45))
                .build()
                .map_err(anyhow::Error::from)
        };
        self.client = client(reqwest::redirect::Policy::default())?;
        if let Some((connection, _)) = &mut self.curseforge {
            *connection = client(reqwest::redirect::Policy::none())?;
        }
        Ok(())
    }

    async fn watched_response(
        &self,
        url: &str,
        offset: u64,
        stalls: u32,
    ) -> Result<reqwest::Response> {
        let wait = self.stall_policy.wait(Duration::ZERO, stalls, true);
        tokio::time::timeout(wait, self.file_response(url, offset))
            .await
            .map_err(|_| crate::download_stall::Stalled(wait))?
    }

    pub(crate) async fn import_cached_files(
        &self,
        root: &Path,
        files: &[FileSpec],
        cancel: &CancellationToken,
    ) -> Result<()> {
        for file in files {
            check_cancel(cancel)?;
            if file.path.is_file() {
                continue;
            }
            let old = object_path(&root.join("cache/objects"), file)?;
            let new = self.cache_path(file)?;
            for (source, target) in [
                (old.clone(), new.clone()),
                (old.with_extension("part"), new.with_extension("part")),
            ] {
                if target.try_exists()? || !source.is_file() {
                    continue;
                }
                tokio::fs::create_dir_all(target.parent().context("无效下载目标")?).await?;
                let temporary =
                    Partial(target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4())));
                tokio::fs::copy(&source, &temporary.0).await?;
                tokio::fs::rename(&temporary.0, target).await?;
            }
        }
        Ok(())
    }
    pub fn urls(&self, url: &str) -> Vec<String> {
        crate::network::urls(&self.settings, url)
    }
    pub fn with_full_verification(mut self) -> Self {
        self.full_verification = true;
        self
    }
    async fn verify(
        &self,
        path: &Path,
        file: &FileSpec,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        self.verification
            .verify(
                path,
                &file.hash,
                file.sha256,
                file.size,
                self.full_verification,
                cancel,
            )
            .await
    }
    async fn flush_verification(&self, sink: &EventSink) {
        if let Err(error) = self.verification.flush().await {
            sink(Event::new(
                "warning",
                format!("保存文件校验记录失败：{error}"),
            ));
        }
    }
    async fn ordered_urls(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<String>> {
        let urls = self.urls(url);
        let key = reqwest::Url::parse(url)?
            .host_str()
            .unwrap_or_default()
            .to_owned();
        self.order_candidates(&key, urls, cancel).await
    }
    async fn order_candidates(
        &self,
        key: &str,
        mut urls: Vec<String>,
        cancel: &CancellationToken,
    ) -> Result<Vec<String>> {
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        if urls.len() < 2 {
            return Ok(urls);
        }
        // Race response headers once per host and task, not large response bodies
        // or every asset. Concurrent downloads share the same in-flight probe.
        let choice = self
            .sources
            .lock()
            .await
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();
        let selected = tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            index = choice.get_or_init(|| async {
                let mut probes: futures_util::stream::FuturesUnordered<_> = urls.iter().enumerate().map(|(index, url)| async move {
                    let response = self.client.head(url).timeout(Duration::from_secs(3)).send().await;
                    (index, response.is_ok_and(|r| r.status().is_success()))
                }).collect();
                while let Some((index, success)) = probes.next().await {
                    if success { return index; }
                }
                0 // Both probes failed: normal requests still get retries/fallback.
            }) => *index,
        };
        urls.swap(0, selected);
        Ok(urls)
    }
    pub async fn json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<T> {
        let mut last = String::new();
        for candidate in self.ordered_urls(url, cancel).await? {
            for attempt in 0..=self.settings.retries {
                let result: Result<T> = tokio::select! {
                    _=cancel.cancelled()=>bail!("操作已取消"),
                    r=async { Ok(self.client.get(&candidate).timeout(Duration::from_secs(30)).send().await?.error_for_status()?.json().await?) }=>r,
                };
                match result {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        let certificate = crate::network::is_certificate_error(&e);
                        last = format!("{:#}", crate::network::explain_error(e));
                        if certificate {
                            break;
                        }
                    }
                }
                if attempt < self.settings.retries {
                    tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), _=tokio::time::sleep(Duration::from_millis(300 * (1<<attempt)))=>{} }
                }
            }
        }
        bail!("无法获取元数据，请检查网络或切换下载源：{last}")
    }
    /// Maven profiles can omit hashes; fetch the repository's small SHA-1 sidecar
    /// through the same proxy/source policy before adding a file to the cache.
    pub async fn maven_sha1(&self, url: &str, cancel: &CancellationToken) -> Result<String> {
        let mut last = String::new();
        for candidate in self.ordered_urls(&format!("{url}.sha1"), cancel).await? {
            for _ in 0..=self.settings.retries {
                let result: Result<String> = tokio::select! {
                    _ = cancel.cancelled() => bail!("操作已取消"),
                    r = async {
                        let mut response = self.client.get(&candidate).timeout(Duration::from_secs(30)).send().await?.error_for_status()?;
                        let mut bytes = Vec::new();
                        while let Some(chunk) = response.chunk().await? {
                            if bytes.len() + chunk.len() > 1024 { bail!("Maven 校验文件过大"); }
                            bytes.extend_from_slice(&chunk);
                        }
                        let text = std::str::from_utf8(&bytes)?;
                        let hash = text.split_whitespace().next().unwrap_or("");
                        if hash.len() != 40 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) { bail!("Maven 校验值无效"); }
                        Ok(hash.to_ascii_lowercase())
                    } => r,
                };
                match result {
                    Ok(hash) => return Ok(hash),
                    Err(error) => {
                        let certificate = crate::network::is_certificate_error(&error);
                        last = format!("{:#}", crate::network::explain_error(error));
                        if certificate {
                            break;
                        }
                    }
                }
            }
        }
        bail!("无法获取依赖校验值：{last}")
    }
    pub async fn verified(path: &Path, hash: &str, sha256: bool, size: u64) -> Result<bool> {
        crate::verification::verify_full(path, hash, sha256, size).await
    }
    pub async fn file(
        &self,
        file: &FileSpec,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        check_cancel(cancel)?;
        if let Some(task) = &self.task {
            task.track(&self.cache, std::slice::from_ref(file)).await?;
        }
        let bytes = self.local_bytes(file).await?;
        let result = self.file_with_progress(file, bytes, cancel, sink).await;
        self.flush_verification(sink).await;
        result.map_err(|error| {
            if cancel.is_cancelled() {
                return error;
            }
            DownloadFailures {
                files: vec![RetryFile {
                    downloader: self.clone(),
                    file: file.clone(),
                }],
                message: crate::diagnostics::redact(&format!("{error:#}")),
            }
            .into()
        })
    }
    /// A successfully consumed installer is not a reusable game dependency.
    /// Remove its target and content-addressed copy, but never partial downloads.
    pub(crate) async fn discard_installer(&self, file: &FileSpec) -> Result<()> {
        // Scoped installations retain all staging until the complete transaction commits.
        if self.task.is_some() {
            return Ok(());
        }
        let cache = self.cache_path(file)?;
        for path in [&file.path, &cache] {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            self.verification.forget(path);
        }
        self.verification.flush().await
    }
    async fn file_with_progress(
        &self,
        file: &FileSpec,
        _bytes: u64,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let result = self.file_inner(file, cancel, sink).await;
        match &result {
            Ok(cached) => {
                let total = if file.size > 0 {
                    file.size
                } else {
                    tokio::fs::metadata(&file.path).await.map_or(0, |m| m.len())
                };
                file.complete(sink, total, *cached);
            }
            Err(error) => {
                let mut progress = file.progress(
                    if cancel.is_cancelled() {
                        "cancelled"
                    } else {
                        "error"
                    },
                    self.local_bytes(file).await.unwrap_or(0),
                    file.size,
                );
                progress.retryable = !cancel.is_cancelled();
                progress.error = progress.retryable.then(|| {
                    crate::diagnostics::redact(&format!("{error:#}"))
                        .chars()
                        .take(1200)
                        .collect()
                });
                sink(Event {
                    files: vec![progress],
                    ..Event::new("file", "")
                });
            }
        }
        result.map(|_| ())
    }
    fn cache_path(&self, file: &FileSpec) -> Result<PathBuf> {
        object_path(&self.cache, file)
    }
    // A cheap inventory of available bytes, not a claim of verified completion.
    // Hash checks below still decide whether these bytes can actually be reused.
    async fn local_bytes(&self, file: &FileSpec) -> Result<u64> {
        available_bytes(&self.cache_path(file)?, file).await
    }
    async fn partial_bytes(&self, file: &FileSpec) -> Result<u64> {
        let bytes = partial_size(&self.cache_path(file)?.with_extension("part")).await?;
        Ok(if file.size > 0 {
            bytes.min(file.size)
        } else {
            bytes
        })
    }
    async fn copy_cached(
        &self,
        file: &FileSpec,
        cache: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        check_cancel(cancel)?;
        tokio::fs::create_dir_all(file.path.parent().context("无效下载目标")?).await?;
        let temp = Partial(
            file.path
                .with_extension(format!("{}.part", uuid::Uuid::new_v4())),
        );
        self.verification
            .copy_verified(cache, &temp.0, &file.hash, file.sha256, cancel)
            .await?;
        if file.path.exists() {
            tokio::fs::remove_file(&file.path).await?;
        }
        tokio::fs::rename(&temp.0, &file.path).await?;
        self.verification.forget(&temp.0);
        self.verification
            .remember(&file.path, &file.hash, file.sha256)
            .await?;
        Ok(())
    }
    async fn verify_visible(
        &self,
        path: &Path,
        file: &FileSpec,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<bool> {
        if !self.full_verification
            && self
                .verification
                .matches_receipt(path, &file.hash, file.sha256, file.size)
                .await
        {
            return Ok(true);
        }
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            file.emit(
                sink,
                "verifying",
                if file.size > 0 {
                    metadata.len().min(file.size)
                } else {
                    metadata.len()
                },
                file.size.max(metadata.len()),
            );
        }
        self.verify(path, file, cancel).await
    }
    async fn restore_cached(
        &self,
        file: &FileSpec,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<bool> {
        check_cancel(cancel)?;
        let cache = self.cache_path(file)?;
        if self.verify_visible(&file.path, file, cancel, sink).await? {
            return Ok(true);
        }
        if self.verify_visible(&cache, file, cancel, sink).await? {
            file.emit(sink, "reusing", file.size, file.size);
            self.copy_cached(file, &cache, cancel).await?;
            return Ok(true);
        }
        for source in self
            .verification
            .candidates(&file.hash, file.sha256, file.size)
            .await
        {
            if source == file.path
                || source == cache
                || source.extension().is_some_and(|e| e == "part")
            {
                continue;
            }
            check_cancel(cancel)?;
            match self.verify_visible(&source, file, cancel, sink).await {
                Ok(true) => {
                    file.emit(sink, "reusing", file.size, file.size);
                    match self.copy_cached(file, &source, cancel).await {
                        Ok(()) => return Ok(true),
                        Err(_) if !cancel.is_cancelled() => self.verification.forget(&source),
                        Err(error) => return Err(error),
                    }
                }
                Ok(false) | Err(_) if !cancel.is_cancelled() => self.verification.forget(&source),
                Err(error) => return Err(error),
                Ok(false) => check_cancel(cancel)?,
            }
        }
        Ok(false)
    }
    async fn file_inner(
        &self,
        file: &FileSpec,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<bool> {
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        let cache = self.cache_path(file)?;
        let key = format!("{}-{}", file.sha256, file.hash);
        let lock = self
            .locks
            .lock()
            .await
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard =
            tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), guard=lock.lock()=>guard };
        if self.restore_cached(file, cancel, sink).await? {
            return Ok(true);
        }
        let bytes = self.partial_bytes(file).await?;
        file.emit(sink, "queued", bytes, file.size.max(bytes));
        // Partial network objects survive cancellation and process restarts.
        // They are never considered complete until the expected hash matches.
        let part = cache.with_extension("part");
        let cached = self.verify_visible(&part, file, cancel, sink).await?;
        if !cached {
            tokio::fs::create_dir_all(cache.parent().unwrap()).await?;
            let mut errors = vec![];
            let mut done = false;
            let mut connection = self.clone();
            let mut stalls = 0;
            'sources: for url in self.ordered_urls(&file.url, cancel).await? {
                for attempt in 0..=self.settings.retries {
                    let download = async {
                        let mut offset = partial_size(&part).await?;
                        if file.size > 0 && offset >= file.size {
                            tokio::fs::remove_file(&part).await?;
                            offset = 0;
                        }
                        let mut response =
                            connection.watched_response(&url, offset, stalls).await?;
                        // A stale partial or a server without range support can
                        // safely restart, never append a whole response to it.
                        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
                            && offset > 0
                        {
                            sink(Event::new(
                                "warning",
                                format!(
                                    "下载源拒绝续传范围，重新下载此文件：{}",
                                    file.path.file_name().unwrap_or_default().to_string_lossy()
                                ),
                            ));
                            tokio::fs::remove_file(&part).await?;
                            offset = 0;
                            response = connection.watched_response(&url, offset, stalls).await?;
                        }
                        let response = response.error_for_status()?;
                        let length = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                            match range_total(&response, offset, file.size) {
                                Ok(total) => total,
                                Err(error) => {
                                    let _ = tokio::fs::remove_file(&part).await;
                                    return Err(error);
                                }
                            }
                        } else {
                            if offset > 0 {
                                sink(Event::new(
                                    "warning",
                                    format!(
                                        "下载源未支持续传，重新下载此文件：{}",
                                        file.path.file_name().unwrap_or_default().to_string_lossy()
                                    ),
                                ));
                            }
                            offset = 0;
                            response.content_length().unwrap_or(0)
                        };
                        let total = if file.size > 0 { file.size } else { length };
                        file.emit(sink, "downloading", offset, total);
                        let mut stream = response.bytes_stream();
                        let mut out = tokio::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .append(offset > 0)
                            .truncate(offset == 0)
                            .open(&part)
                            .await?;
                        let mut bytes = offset;
                        let mut last = std::time::Instant::now();
                        let mut cadence = Duration::ZERO;
                        loop {
                            let wait = self.stall_policy.wait(cadence, stalls, false);
                            let started = tokio::time::Instant::now();
                            let next = tokio::time::timeout(wait, stream.next())
                                .await
                                .map_err(|_| crate::download_stall::Stalled(wait))?;
                            let Some(chunk) = next else { break };
                            // Only network waiting counts; exclude local writes,
                            // hashing and event delivery from the cadence estimate.
                            let elapsed = started.elapsed();
                            cadence = if cadence.is_zero() {
                                elapsed
                            } else {
                                (cadence * 3 + elapsed) / 4
                            };
                            let chunk = chunk?;
                            if self.settings.adaptive_concurrency {
                                self.metrics
                                    .bytes
                                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            }
                            bytes += chunk.len() as u64;
                            if total > 0 && bytes > total {
                                drop(out);
                                let _ = tokio::fs::remove_file(&part).await;
                                bail!("文件大小超过元数据声明");
                            }
                            out.write_all(&chunk).await?;
                            if bytes == offset + chunk.len() as u64
                                || last.elapsed() > Duration::from_millis(200)
                            {
                                // Report bytes only after Tokio has finished its
                                // pending write, so cancellation can resume them.
                                out.flush().await?;
                                file.emit(sink, "downloading", bytes, total);
                                last = std::time::Instant::now();
                            }
                        }
                        out.flush().await?;
                        drop(out);
                        file.emit(sink, "verifying", bytes, total);
                        if !self.verify(&part, file, cancel).await? {
                            tokio::fs::remove_file(&part).await?;
                            bail!("下载文件校验失败");
                        }
                        Ok::<_, anyhow::Error>(())
                    };
                    match tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), r=download=>r }
                    {
                        Ok(()) => {
                            done = true;
                            break 'sources;
                        }
                        Err(e) => {
                            let stalled =
                                e.downcast_ref::<crate::download_stall::Stalled>().is_some();
                            let certificate = crate::network::is_certificate_error(&e);
                            let e = crate::network::explain_error(e);
                            // Invalid responses/hash failures may have discarded
                            // the partial. Reflect the actual retained bytes.
                            let bytes = self.partial_bytes(file).await?;
                            file.emit(sink, "queued", bytes, file.size.max(bytes));
                            if self.settings.adaptive_concurrency && !stalled {
                                self.metrics.retries.fetch_add(1, Ordering::Relaxed);
                            }
                            errors.push(format!("{e:#}"));
                            // A new attempt cannot fix this certificate. Still
                            // allow a separately verified mirror to succeed.
                            if certificate || attempt == self.settings.retries {
                                break;
                            }
                            if stalled {
                                stalls += 1;
                                let message = format!(
                                    "{e}；保留 {bytes} 字节并重新连接：{}",
                                    file.path.file_name().unwrap_or_default().to_string_lossy()
                                );
                                sink(Event::new("warning", &message));
                                let root = self.network_root.clone();
                                let worker = connection.clone();
                                // Client/TLS setup and log I/O must not block all
                                // other download futures on the runtime thread.
                                connection = tokio::task::spawn_blocking(move || {
                                    let _ = crate::diagnostics::record_download_recovery(
                                        &root, &message,
                                    );
                                    let mut worker = worker;
                                    worker.reconnect()?;
                                    Ok::<_, anyhow::Error>(worker)
                                })
                                .await??;
                            }
                            sink(Event::new(
                                "retry",
                                format!(
                                    "重试 {}（{}/{}）",
                                    file.path.file_name().unwrap_or_default().to_string_lossy(),
                                    attempt + 1,
                                    self.settings.retries + 1
                                ),
                            ));
                        }
                    }
                    tokio::select! { _=cancel.cancelled()=>bail!("操作已取消"), _=tokio::time::sleep(Duration::from_millis(300*(1<<attempt)))=>{} }
                }
            }
            if !done {
                bail!("下载失败 {}：{}", file.path.display(), errors.join("；"));
            }
        }
        if cache.exists() {
            tokio::fs::remove_file(&cache).await?;
        }
        tokio::fs::rename(&part, &cache).await?;
        self.verification.forget(&part);
        self.verification
            .remember(&cache, &file.hash, file.sha256)
            .await?;
        file.emit(sink, "installing", file.size, file.size);
        self.copy_cached(file, &cache, cancel).await?;
        Ok(cached)
    }
    pub async fn batch(
        &self,
        files: Vec<FileSpec>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        check_cancel(cancel)?;
        if let Some(task) = &self.task {
            task.track(&self.cache, &files).await?;
        }
        let result = self.batch_inner(files, cancel, sink).await;
        self.flush_verification(sink).await;
        result
    }
    async fn batch_inner(
        &self,
        files: Vec<FileSpec>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let mut unique = std::collections::BTreeMap::new();
        for f in files {
            unique.insert(f.path.clone(), f);
        }
        let inventory = futures_util::stream::iter(unique.into_values().map(|file| async move {
            let bytes = self.local_bytes(&file).await?;
            let reused = !self.full_verification
                && self
                    .verification
                    .matches_receipt(&file.path, &file.hash, file.sha256, file.size)
                    .await;
            let mut progress = file.progress(
                if reused { "done" } else { "queued" },
                bytes,
                file.size.max(bytes),
            );
            progress.cached = reused;
            Ok::<_, anyhow::Error>((file, progress))
        }))
        .buffered(8)
        .try_collect::<Vec<_>>();
        let inventoried = tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            result = inventory => result?,
        };
        let (files, planned): (Vec<_>, Vec<_>) = inventoried.into_iter().unzip();
        let aggregate = Arc::new(std::sync::Mutex::new(BatchProgress::new(&planned)));
        let _final_progress = FinalProgress {
            sink: sink.clone(),
            state: aggregate.clone(),
        };
        let state = aggregate.clone();
        let forward = sink.clone();
        let sink: EventSink = Arc::new(move |event| {
            let progress = if event.kind == "file" {
                state.lock().unwrap().update(&event)
            } else {
                None
            };
            forward(event);
            if let Some(progress) = progress {
                forward(progress);
            }
        });
        // Seed the queue before hash/network work, including entries later in
        // the queue. Resume must not briefly display every existing file as 0.
        for chunk in planned.chunks(256) {
            sink(Event {
                files: chunk.to_vec(),
                ..Event::new("file_plan", "")
            });
        }
        sink(Event {
            reset_progress: true,
            ..aggregate.lock().unwrap().event()
        });
        // The initial size inventory already accounts for all local data.
        // Check/reuse each file as its slot opens; missing files no longer wait
        // for a second pass over the entire local installation.
        sink(Event::new("stage", "复用缓存并下载缺失文件"));
        let mut completed = 0;
        let mut failures = Vec::new();
        let mut errors = Vec::new();
        let mut queue = files.into_iter().zip(planned).peekable();
        let mut active = FuturesUnordered::new();
        let mut concurrency = crate::download_concurrency::Concurrency::new(
            if self.settings.adaptive_concurrency {
                crate::download_concurrency::MAX_CONCURRENCY
            } else {
                self.settings.concurrency
            },
            self.settings.adaptive_concurrency,
            self.metrics.bytes.load(Ordering::Relaxed),
            self.metrics.retries.load(Ordering::Relaxed),
        );
        let period = Duration::from_secs(1);
        let mut sampled_at = tokio::time::Instant::now();
        let mut sample = tokio::time::interval_at(sampled_at + period, period);
        sample.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            check_cancel(cancel)?;
            while active.len() < concurrency.limit {
                let Some((file, known)) = queue.next() else {
                    break;
                };
                let sink = sink.clone();
                active.push(async move {
                    let mut result = self
                        .file_with_progress(&file, known.downloaded, cancel, &sink)
                        .await;
                    if let Some(urls) = self.mirrors.get(&file.path) {
                        for url in urls.iter().filter(|u| **u != file.url) {
                            if result.is_ok() {
                                break;
                            }
                            check_cancel(cancel)?;
                            let alternative = FileSpec {
                                url: url.clone(),
                                ..file.clone()
                            };
                            result = self
                                .file_with_progress(&alternative, known.downloaded, cancel, &sink)
                                .await;
                        }
                    }
                    Ok::<_, anyhow::Error>((file, result))
                });
            }
            if active.is_empty() {
                break;
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => bail!("操作已取消"),
                result = active.next() => {
                    let (file, result) = result.expect("active downloads exist")?;
                    match result {
                        Ok(()) => completed += 1,
                        Err(error) => {
                            errors.push(crate::diagnostics::redact(&format!("{error:#}")));
                            failures.push(RetryFile {downloader:self.clone(),file});
                        }
                    }
                }
                _ = sample.tick(), if self.settings.adaptive_concurrency && queue.peek().is_some() => {
                    let now = tokio::time::Instant::now();
                    concurrency.observe(self.metrics.bytes.load(Ordering::Relaxed), self.metrics.retries.load(Ordering::Relaxed), completed, now - sampled_at, active.len());
                    sampled_at = now;
                }
            }
        }
        if !failures.is_empty() {
            return Err(DownloadFailures {
                message: format!(
                    "{} 个文件下载失败，其余文件已保留。{}",
                    failures.len(),
                    errors.first().map(String::as_str).unwrap_or("")
                ),
                files: failures,
            }
            .into());
        }
        Ok(())
    }
}

pub(crate) fn object_path(cache: &Path, file: &FileSpec) -> Result<PathBuf> {
    let expected = if file.sha256 { 64 } else { 40 };
    if file.hash.len() != expected || !file.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("资源校验值无效");
    }
    Ok(cache
        .join(if file.sha256 { "sha256" } else { "sha1" })
        .join(&file.hash[..2])
        .join(&file.hash))
}

async fn available_bytes(cache: &Path, file: &FileSpec) -> Result<u64> {
    for path in [&file.path, cache] {
        if let Ok(meta) = tokio::fs::symlink_metadata(path).await
            && meta.is_file()
            && (file.size == 0 || meta.len() == file.size)
        {
            return Ok(meta.len());
        }
    }
    let bytes = partial_size(&cache.with_extension("part")).await?;
    Ok(if file.size > 0 {
        bytes.min(file.size)
    } else {
        bytes
    })
}

pub async fn local_progress(root: &Path, files: Vec<FileSpec>) -> Result<f64> {
    let unique: std::collections::BTreeMap<_, _> = files
        .into_iter()
        .map(|file| (file.path.clone(), file))
        .collect();
    let mut checks = futures_util::stream::iter(unique.into_values().map(|file| async move {
        let cache = object_path(&root.join("cache/objects"), &file)?;
        let weight = file.size.max(1);
        let bytes = available_bytes(&cache, &file).await?.min(weight);
        Ok::<_, anyhow::Error>((bytes, weight))
    }))
    .buffer_unordered(8);
    let (mut bytes, mut total) = (0u64, 0u64);
    while let Some(result) = checks.next().await {
        let (available, weight) = result?;
        bytes += available;
        total += weight;
    }
    Ok(if total == 0 {
        0.0
    } else {
        (bytes as f64 / total as f64 * 100.0).min(99.0)
    })
}

pub fn artifact_file(a: &crate::model::Artifact, path: PathBuf) -> FileSpec {
    FileSpec {
        url: a.url.clone(),
        path,
        hash: a.sha1.clone(),
        sha256: false,
        size: a.size,
    }
}
pub fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("操作已取消")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn stalled_download_reconnects_and_resumes_only_the_missing_bytes() {
        use tokio::net::TcpListener;
        for headers in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured = requests.clone();
            let (release, wait) = tokio::sync::oneshot::channel::<()>();
            let server = tokio::spawn(async move {
                let mut held = Vec::new();
                for attempt in 0..2 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = [0; 4096];
                    let size = stream.read(&mut request).await.unwrap();
                    captured
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&request[..size]).to_lowercase());
                    if attempt == 0 {
                        if !headers {
                            stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabcdef")
                                .await
                                .unwrap();
                        }
                        held.push(stream); // Keep TCP open indefinitely, like a stalled CDN stream.
                    } else if headers {
                        stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabcdefghi")
                            .await
                            .unwrap();
                    } else {
                        stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 6-8/9\r\n\r\nghi").await.unwrap();
                    }
                }
                let _ = wait.await;
            });
            let mut d = Downloader::new(
                temp.path(),
                Settings {
                    proxy_mode: "none".into(),
                    retries: 1,
                    ..Settings::default()
                },
            )
            .unwrap();
            d.stall_policy = crate::download_stall::StallPolicy {
                idle: Duration::from_millis(150),
                headers: Duration::from_millis(200),
                ceiling: Duration::from_secs(1),
            };
            let file = FileSpec {
                url,
                path: temp.path().join("game/example.jar"),
                hash: hex::encode(Sha1::digest(b"abcdefghi")),
                sha256: false,
                size: 9,
            };
            let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
            let captured = events.clone();
            let sink: EventSink = Arc::new(move |e| captured.lock().unwrap().push(e));
            tokio::time::timeout(
                Duration::from_secs(4),
                d.file(&file, &CancellationToken::new(), &sink),
            )
            .await
            .unwrap()
            .unwrap();
            release.send(()).unwrap();
            server.await.unwrap();
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            assert_eq!(requests[1].contains("range: bytes=6-"), !headers);
            assert_eq!(fs::read(&file.path).unwrap(), b"abcdefghi");
            assert_eq!(
                d.metrics.retries.load(Ordering::Relaxed),
                0,
                "one stalled connection must not halve healthy global concurrency"
            );
            assert!(
                fs::read_to_string(temp.path().join("logs/launcher.log"))
                    .unwrap()
                    .contains("download_recovery")
            );
            if !headers {
                let events = events.lock().unwrap();
                let retained = events
                    .iter()
                    .flat_map(|e| &e.files)
                    .skip_while(|f| f.downloaded < 6);
                assert!(
                    retained.into_iter().all(|f| f.downloaded >= 6),
                    "reconnect must preserve progress"
                );
            }
        }
    }

    #[tokio::test]
    async fn slow_but_progressing_download_is_not_restarted() {
        let temp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                assert!(request.len() <= 16 * 1024);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n")
                .await
                .unwrap();
            for byte in b"abcdefghi" {
                tokio::time::sleep(Duration::from_millis(70)).await;
                stream.write_all(&[*byte]).await.unwrap();
            }
        });
        let mut d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                retries: 0,
                ..Settings::default()
            },
        )
        .unwrap();
        d.stall_policy = crate::download_stall::StallPolicy {
            idle: Duration::from_millis(200),
            headers: Duration::from_secs(1),
            ceiling: Duration::from_secs(1),
        };
        let file = FileSpec {
            url,
            path: temp.path().join("game/slow.jar"),
            hash: hex::encode(Sha1::digest(b"abcdefghi")),
            sha256: false,
            size: 9,
        };
        let sink: EventSink = Arc::new(|e| assert_ne!(e.kind, "retry"));
        d.file(&file, &CancellationToken::new(), &sink)
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(fs::read(&file.path).unwrap(), b"abcdefghi");
    }

    #[tokio::test]
    async fn consumed_installer_removes_both_copies_and_receipts_only() {
        let root = tempfile::tempdir().unwrap();
        let (url, server) = crate::test_http::serve(vec![
            (200, "installer-fixture".into()),
            (200, "library-fixture".into()),
        ])
        .await;
        let downloader = Downloader::new(
            root.path(),
            Settings {
                proxy_mode: "none".into(),
                download_source: "official".into(),
                retries: 0,
                ..Settings::default()
            },
        )
        .unwrap();
        let spec = |body: &str, path: &str| FileSpec {
            url: format!("{url}/{path}"),
            path: root.path().join(path),
            hash: hex::encode(ring::digest::digest(&ring::digest::SHA256, body.as_bytes())),
            sha256: true,
            size: body.len() as u64,
        };
        let installer = spec("installer-fixture", "cache/java/fixture.zip");
        let library = spec("library-fixture", "libraries/keep.jar");
        let cancel = CancellationToken::new();
        let sink: EventSink = Arc::new(|_| {});
        for file in [&installer, &library] {
            downloader.file(file, &cancel, &sink).await.unwrap();
        }
        server.await.unwrap();
        let cache = downloader.cache_path(&installer).unwrap();
        assert!(cache.is_file());
        let partial = cache.with_extension("part");
        std::fs::write(&partial, b"resume").unwrap();
        downloader.discard_installer(&installer).await.unwrap();
        assert!(!installer.path.exists());
        assert!(!cache.exists());
        assert!(partial.is_file());
        assert!(library.path.is_file());
        assert!(downloader.cache_path(&library).unwrap().is_file());
        let index: serde_json::Value =
            crate::storage::read_json(&root.path().join("cache/verified-files.json")).unwrap();
        for path in [&installer.path, &cache] {
            assert!(index["entries"].get(path.to_str().unwrap()).is_none());
        }
        assert!(
            index["entries"]
                .get(library.path.to_str().unwrap())
                .is_some()
        );
        downloader.discard_installer(&installer).await.unwrap();
    }
    use sha1::{Digest, Sha1};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn failed_file_does_not_abort_neighbors_and_retry_only_fetches_that_file() {
        use sha1::{Digest, Sha1};
        let root = tempfile::tempdir().unwrap();
        let (url, server) = crate::test_http::serve(vec![
            (500, "failed".into()),
            (200, "good".into()),
            (200, "fixed".into()),
        ])
        .await;
        let d = Downloader::new(
            root.path(),
            Settings {
                proxy_mode: "none".into(),
                concurrency: 1,
                adaptive_concurrency: false,
                retries: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let failed = FileSpec {
            url: format!("{url}/failed"),
            path: root.path().join("a.jar"),
            hash: hex::encode(Sha1::digest(b"fixed")),
            sha256: false,
            size: 5,
        };
        let good = FileSpec {
            url: format!("{url}/good"),
            path: root.path().join("b.jar"),
            hash: hex::encode(Sha1::digest(b"good")),
            sha256: false,
            size: 4,
        };
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = events.clone();
        let sink: EventSink = Arc::new(move |e| output.lock().unwrap().push(e));
        let error = d
            .batch(
                vec![failed.clone(), good.clone()],
                &CancellationToken::new(),
                &sink,
            )
            .await
            .unwrap_err();
        assert_eq!(std::fs::read(&good.path).unwrap(), b"good");
        let failure = error.downcast_ref::<DownloadFailures>().unwrap();
        assert_eq!(failure.files.len(), 1);
        assert_eq!(failure.files[0].id(), failed.path.to_string_lossy());
        failure.files[0]
            .run(&CancellationToken::new(), &sink)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&failed.path).unwrap(), b"fixed");
        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("GET /failed "));
        assert!(requests[1].starts_with("GET /good "));
        assert!(requests[2].starts_with("GET /failed "));
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .flat_map(|e| &e.files)
                .any(|f| f.status == "error" && f.retryable && f.error.is_some())
        );
        assert!(
            events
                .iter()
                .flat_map(|e| &e.files)
                .any(|f| f.status == "installing")
        );
    }

    #[tokio::test]
    async fn curseforge_download_auth_is_not_sent_to_unrelated_hosts_and_keeps_ranges() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings {
            proxy_mode: "none".into(),
            ..Settings::default()
        };
        let downloader = Downloader::new(temp.path(), settings.clone())
            .unwrap()
            .with_curseforge_key(temp.path(), &settings, "private-test-key")
            .unwrap();
        let (url, server) = crate::test_http::serve(vec![(200, "bytes".into())]).await;
        let response = downloader
            .file_response(&format!("{url}/payload"), 123)
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "bytes");
        let requests = server.await.unwrap();
        assert!(!requests[0].contains("private-test-key"));
        assert!(!requests[0].to_ascii_lowercase().contains("x-api-key"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("range: bytes=123-")
        );
    }

    #[tokio::test]
    async fn skewed_clock_reports_tls_failure_once_and_recovers_after_clock_sync() {
        use rustls::{
            ClientConfig, RootCertStore, ServerConfig,
            pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime},
            time_provider::TimeProvider,
        };
        #[derive(Debug)]
        struct Clock(u64);
        impl TimeProvider for Clock {
            fn current_time(&self) -> Option<UnixTime> {
                Some(UnixTime::since_unix_epoch(Duration::from_secs(self.0)))
            }
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cert =
            CertificateDer::from(include_bytes!("../tests/fixtures/tls/localhost.der").to_vec());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            include_bytes!("../tests/fixtures/tls/localhost.key.der").to_vec(),
        ));
        let config = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let attempts = Arc::new(AtomicUsize::new(0));
        let counted = attempts.clone();
        let server = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                counted.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut stream) = acceptor.accept(socket).await {
                    let mut bytes = [0; 4096];
                    let _ = stream.read(&mut bytes).await;
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                        )
                        .await;
                }
            }
        });
        let client = |clock| {
            let mut roots = RootCertStore::empty();
            roots
                .add(CertificateDer::from(
                    include_bytes!("../tests/fixtures/tls/ca.der").to_vec(),
                ))
                .unwrap();
            let mut config = ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
            config.time_provider = Arc::new(Clock(clock));
            reqwest::Client::builder()
                .no_proxy()
                .tls_backend_preconfigured(config)
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
        };
        let temp = tempfile::tempdir().unwrap();
        let mut downloader = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                retries: 3,
                ..Settings::default()
            },
        )
        .unwrap();
        let url = format!("https://localhost:{port}/version.json");
        let cancel = CancellationToken::new();
        // June 2027: one year beyond the test's June 2026 reference clock.
        downloader.client = client(1_811_808_000);
        let error = downloader
            .json::<serde_json::Value>(&url, &cancel)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("系统日期"), "{error:#}");
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        // Signed partial download data must survive a TLS failure untouched.
        let file = FileSpec {
            url: url.clone(),
            path: temp.path().join("game.json"),
            hash: hex::encode(Sha1::digest(b"{}")),
            sha256: false,
            size: 2,
        };
        let part = downloader.cache_path(&file).unwrap().with_extension("part");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, b"{").unwrap();
        let sink: EventSink = Arc::new(|_| {});
        assert!(
            format!(
                "{:#}",
                downloader.file(&file, &cancel, &sink).await.unwrap_err()
            )
            .contains("系统日期")
        );
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
        assert_eq!(std::fs::read(&part).unwrap(), b"{");
        downloader.client = client(1_780_272_000); // June 2026: valid.
        assert_eq!(
            downloader
                .json::<serde_json::Value>(&url, &cancel)
                .await
                .unwrap(),
            serde_json::json!({})
        );
        downloader.file(&file, &cancel, &sink).await.unwrap();
        assert_eq!(std::fs::read(&file.path).unwrap(), b"{}");
        // January 2025: too early must be diagnosed as a clock issue too.
        downloader.client = client(1_735_689_600);
        assert!(
            format!(
                "{:#}",
                downloader
                    .json::<serde_json::Value>(&url, &cancel)
                    .await
                    .unwrap_err()
            )
            .contains("系统日期")
        );
        server.abort();
        let _ = server.await;
    }
    #[tokio::test]
    async fn batch_receipts_survive_cancel_and_restart_and_progress_scan_never_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                concurrency: 1,
                ..Settings::default()
            },
        )
        .unwrap();
        let path = temp.path().join("aaa.jar");
        std::fs::write(&path, b"cached").unwrap();
        let file = FileSpec {
            url: "http://127.0.0.1:9/never".into(),
            path,
            hash: hex::encode(Sha1::digest(b"cached")),
            sha256: false,
            size: 6,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let sink: EventSink = Arc::new(move |e| {
            if e.files.iter().any(|file| file.status == "done") {
                token.cancel();
            }
        });
        assert!(d.batch(vec![file.clone()], &cancel, &sink).await.is_err());
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 1);
        drop(d);
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        d.batch(vec![file.clone()], &CancellationToken::new(), &sink)
            .await
            .unwrap();
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 0);
        // A size-only inventory is a progress hint, not a verification. It must
        // not read contents, mark installed, or establish a trusted receipt.
        std::fs::write(&file.path, b"broken").unwrap();
        assert_eq!(
            local_progress(temp.path(), vec![file.clone()])
                .await
                .unwrap(),
            99.0
        );
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 0);
        assert!(
            !d.verify(&file.path, &file, &CancellationToken::new())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    #[ignore = "local performance sample, no network or user data"]
    async fn benchmark_verified_cache_reuse() {
        let temp = tempfile::tempdir().unwrap();
        let payload = vec![42u8; 64 * 1024];
        let hash = hex::encode(Sha1::digest(&payload));
        let files: Vec<_> = (0..2048)
            .map(|i| {
                let path = temp.path().join(format!("{i}.asset"));
                std::fs::write(&path, &payload).unwrap();
                FileSpec {
                    url: "http://127.0.0.1:9/never".into(),
                    path,
                    hash: hash.clone(),
                    sha256: false,
                    size: payload.len() as u64,
                }
            })
            .collect();
        let sink: EventSink = Arc::new(|_| {});
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let start = std::time::Instant::now();
        d.batch(files.clone(), &CancellationToken::new(), &sink)
            .await
            .unwrap();
        let cold = start.elapsed();
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 2048);
        drop(d);
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let start = std::time::Instant::now();
        d.batch(files, &CancellationToken::new(), &sink)
            .await
            .unwrap();
        let warm = start.elapsed();
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 0);
        eprintln!(
            "2048 files / 128 MiB: initial verification {cold:?}, persisted receipt reuse {warm:?}, payload hashes 2048 -> 0"
        );
    }
    #[tokio::test]
    async fn file_events_report_the_queue_stream_bytes_verified_completion_and_cache_hits() {
        let temp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let partial_received = Arc::new(tokio::sync::Notify::new());
        let partial_sent = partial_received.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nabc")
                .await
                .unwrap();
            // Hold the rest until a partial progress event arrives: independent
            // of scheduler timing or how TCP coalesces subsequent chunks.
            tokio::time::timeout(Duration::from_secs(3), partial_sent.notified())
                .await
                .unwrap();
            stream.write_all(b"defghi").await.unwrap();
        });
        let downloader = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let file = FileSpec {
            url,
            path: temp.path().join("client.jar"),
            hash: hex::encode(Sha1::digest(b"abcdefghi")),
            sha256: false,
            size: 9,
        };
        let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
        let captured = events.clone();
        let sink: EventSink = Arc::new(move |event| {
            if event
                .files
                .iter()
                .any(|file| file.status == "downloading" && file.downloaded > 0)
            {
                partial_received.notify_one();
            }
            captured.lock().unwrap().push(event);
        });
        let sink = Event::scoped(&sink, "instance-fixture", Some("files"));
        let cancel = CancellationToken::new();
        tokio::time::timeout(
            Duration::from_secs(4),
            downloader.batch(vec![file.clone(), file.clone()], &cancel, &sink),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        {
            let events = events.lock().unwrap();
            assert_eq!(events[0].kind, "file_plan");
            assert_eq!(events[0].files.len(), 1); // duplicate destinations use one transfer.
            assert_eq!(events[0].files[0].status, "queued");
            assert!(
                events
                    .iter()
                    .all(|e| e.instance_id.as_deref() == Some("instance-fixture")
                        && e.step.as_deref() == Some("files"))
            );
            let files: Vec<_> = events
                .iter()
                .filter(|e| e.kind == "file")
                .flat_map(|e| &e.files)
                .collect();
            assert!(
                files
                    .iter()
                    .any(|f| f.status == "downloading" && f.downloaded > 0 && f.downloaded < 9)
            );
            assert!(
                files
                    .iter()
                    .any(|f| f.status == "verifying" && f.downloaded == 9)
            );
            assert_eq!(files.last().unwrap().status, "done");
            assert_eq!(files.last().unwrap().downloaded, 9);
        }
        events.lock().unwrap().clear();
        downloader.file(&file, &cancel, &sink).await.unwrap();
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .flat_map(|e| &e.files)
                .all(|f| f.status != "downloading")
        );
        assert_eq!(events.last().unwrap().files[0].status, "done");
    }

    #[tokio::test]
    async fn batch_honors_manual_concurrency_and_adaptive_ignores_the_disabled_slider() {
        for (adaptive, configured) in [(false, 1), (false, 64), (true, 1)] {
            let temp = tempfile::tempdir().unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            let d = Downloader::new(
                temp.path(),
                Settings {
                    proxy_mode: "none".into(),
                    concurrency: configured,
                    adaptive_concurrency: adaptive,
                    ..Settings::default()
                },
            )
            .unwrap();
            let files = (0..if adaptive { 6 } else { configured + 2 })
                .map(|i| FileSpec {
                    url: format!("{address}/{i}"),
                    path: temp.path().join(format!("{i}.jar")),
                    hash: format!("{i:040x}"),
                    sha256: false,
                    size: 10,
                })
                .collect();
            let cancel = CancellationToken::new();
            let sink: EventSink = Arc::new(|e| {
                if e.kind == "progress" {
                    assert_eq!(e.completed, 0);
                }
            });
            let inspect = async {
                let mut connections = Vec::new();
                for _ in 0..if adaptive { 4 } else { configured } {
                    connections.push(listener.accept().await.unwrap().0);
                }
                assert!(
                    tokio::time::timeout(Duration::from_millis(50), listener.accept())
                        .await
                        .is_err()
                );
                cancel.cancel();
                connections
            };
            let (result, _connections) = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(d.batch(files, &cancel, &sink), inspect)
            })
            .await
            .unwrap();
            assert!(result.unwrap_err().to_string().contains("取消"));
            assert!(
                walkdir::WalkDir::new(temp.path())
                    .into_iter()
                    .all(|entry| !entry.unwrap().path().to_string_lossy().ends_with(".part"))
            );
        }
    }
    #[tokio::test]
    async fn automatic_source_races_headers_and_reuses_result_across_downloads() {
        let temp = tempfile::tempdir().unwrap();
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let slow_url = format!("http://{}", listener.local_addr().unwrap());
        // This source never responds; choosing a healthy source must not wait
        // for the slow source's 3 s timeout or a complete downloaded file.
        let slow = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
            drop(stream);
        });
        let (fast_url, fast) = crate::test_http::serve(vec![(200, String::new())]).await;
        let candidates = vec![slow_url.clone(), fast_url.clone()];
        let cancel = CancellationToken::new();
        let selected = tokio::time::timeout(
            Duration::from_secs(1),
            d.order_candidates("fixture", candidates.clone(), &cancel),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(selected, [fast_url, slow_url]);
        assert!(fast.await.unwrap()[0].starts_with("HEAD / HTTP/1.1"));
        slow.abort();
        // Both endpoints have closed, so this also catches accidental re-probes.
        assert_eq!(
            d.clone()
                .order_candidates("fixture", candidates, &cancel)
                .await
                .unwrap(),
            selected
        );
        cancel.cancel();
        assert!(
            d.order_candidates("fixture", selected, &cancel)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn automatic_source_ignores_failed_probe_and_can_be_cancelled_in_flight() {
        let temp = tempfile::tempdir().unwrap();
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let (failed_url, failed) = crate::test_http::serve(vec![(503, String::new())]).await;
        let (good_url, good) = crate::test_http::serve(vec![(200, String::new())]).await;
        let result = d
            .order_candidates(
                "health",
                vec![failed_url, good_url.clone()],
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result[0], good_url);
        good.await.unwrap();
        failed.abort(); // A successful probe may win before the failed one connects.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let operation = d.order_candidates("cancel", vec![url.clone(), url], &cancel);
        tokio::pin!(operation);
        tokio::select! {
            result = &mut operation => panic!("unexpected completion: {result:?}"),
            accepted = listener.accept() => {
                let (_stream, _) = accepted.unwrap();
                cancel.cancel();
                assert!(operation.await.unwrap_err().to_string().contains("取消"));
            }
        }
    }
    #[tokio::test]
    async fn retries_http_failure_and_rejects_corrupt_response() {
        for valid in [true, false] {
            let t = tempfile::tempdir().unwrap();
            let settings = Settings {
                retries: if valid { 1 } else { 0 },
                adaptive_concurrency: true,
                ..Settings::default()
            };
            let d = Downloader::new(t.path(), settings).unwrap();
            let content = "verified";
            let replies = if valid {
                vec![(503, "retry".into()), (200, content.into())]
            } else {
                vec![(200, "tampered".into())]
            };
            let (url, server) = crate::test_http::serve(replies).await;
            let file = FileSpec {
                url,
                path: t.path().join("game/test.jar"),
                hash: hex::encode(Sha1::digest(content)),
                sha256: false,
                size: content.len() as u64,
            };
            let sink: EventSink = Arc::new(|_| {});
            let result = d.file(&file, &CancellationToken::new(), &sink).await;
            let requests = server.await.unwrap();
            assert_eq!(d.metrics.retries.load(Ordering::Relaxed), 1);
            assert!(d.metrics.bytes.load(Ordering::Relaxed) > 0);
            if valid {
                result.unwrap();
                assert_eq!(requests.len(), 2);
                assert_eq!(std::fs::read(&file.path).unwrap(), content.as_bytes());
            } else {
                assert!(result.unwrap_err().to_string().contains("校验失败"));
                assert!(!file.path.exists());
                assert!(
                    walkdir::WalkDir::new(t.path().join("cache"))
                        .into_iter()
                        .all(|e| !e.unwrap().file_type().is_file())
                );
            }
        }
    }

    #[tokio::test]
    async fn cancellation_retains_partial_and_a_new_downloader_resumes_it() {
        use tokio::net::TcpListener;
        let t = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabcdef")
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let d = Downloader::new(
            t.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let mut file = FileSpec {
            url,
            path: t.path().join("game/test.jar"),
            hash: hex::encode(Sha1::digest(b"abcdefghi")),
            sha256: false,
            size: 9,
        };
        let sink: EventSink = Arc::new(move |event| {
            if event
                .files
                .iter()
                .any(|file| file.status == "downloading" && file.downloaded == 6)
            {
                token.cancel();
            }
        });
        let result = tokio::time::timeout(Duration::from_secs(3), d.file(&file, &cancel, &sink))
            .await
            .unwrap();
        assert!(result.unwrap_err().to_string().contains("取消"));
        assert!(!file.path.exists());
        let part = d.cache_path(&file).unwrap().with_extension("part");
        assert_eq!(tokio::fs::read(&part).await.unwrap(), b"abcdef");
        server.abort();
        let (url, server) = crate::test_http::serve_with_headers(vec![(
            206,
            "Content-Range: bytes 6-8/9\r\n".into(),
            "ghi".into(),
        )])
        .await;
        file.url = url;
        let d = Downloader::new(
            t.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        d.file(&file, &CancellationToken::new(), &sink)
            .await
            .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].to_lowercase().contains("range: bytes=6-"));
        assert_eq!(tokio::fs::read(&file.path).await.unwrap(), b"abcdefghi");
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn range_fallback_and_rejection_never_append_an_invalid_response() {
        for mode in ["ignored", "invalid", "corrupt", "unsatisfiable"] {
            let temp = tempfile::tempdir().unwrap();
            let d = Downloader::new(
                temp.path(),
                Settings {
                    proxy_mode: "none".into(),
                    retries: 0,
                    ..Settings::default()
                },
            )
            .unwrap();
            let replies = match mode {
                "ignored" => vec![(200, "".into(), "abcdefghi".into())],
                "invalid" => vec![(206, "Content-Range: bytes 0-2/9\r\n".into(), "ghi".into())],
                "corrupt" => vec![(206, "Content-Range: bytes 6-8/9\r\n".into(), "xyz".into())],
                _ => vec![
                    (416, "".into(), "".into()),
                    (200, "".into(), "abcdefghi".into()),
                ],
            };
            let (url, server) = crate::test_http::serve_with_headers(replies).await;
            let file = FileSpec {
                url,
                path: temp.path().join("game/client.jar"),
                hash: hex::encode(Sha1::digest(b"abcdefghi")),
                sha256: false,
                size: 9,
            };
            let part = d.cache_path(&file).unwrap().with_extension("part");
            tokio::fs::create_dir_all(part.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&part, b"abcdef").await.unwrap();
            let sink: EventSink = Arc::new(|_| {});
            let result = d.file(&file, &CancellationToken::new(), &sink).await;
            let requests = server.await.unwrap();
            assert!(requests[0].to_lowercase().contains("range: bytes=6-"));
            if matches!(mode, "ignored" | "unsatisfiable") {
                result.unwrap();
                assert_eq!(tokio::fs::read(&file.path).await.unwrap(), b"abcdefghi");
                if mode == "unsatisfiable" {
                    assert!(!requests[1].to_lowercase().contains("range:"));
                }
            } else {
                assert!(result.is_err());
                assert!(!file.path.exists());
                assert!(!d.cache_path(&file).unwrap().exists());
            }
            assert!(!part.exists());
        }
    }

    #[tokio::test]
    async fn resume_counts_all_cached_files_before_requesting_missing_files() {
        let temp = tempfile::tempdir().unwrap();
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let (url, server) = crate::test_http::serve_with_headers(vec![(
            206,
            "Content-Range: bytes 1-2/3\r\n".into(),
            "ew".into(),
        )])
        .await;
        let new = FileSpec {
            url,
            path: temp.path().join("game/aaa.jar"),
            hash: hex::encode(Sha1::digest(b"new")),
            sha256: false,
            size: 3,
        };
        let cached = FileSpec {
            url: "http://127.0.0.1:9/must-not-fetch".into(),
            path: temp.path().join("game/zzz.jar"),
            hash: hex::encode(Sha1::digest(b"cached")),
            sha256: false,
            size: 6,
        };
        let cache = d.cache_path(&cached).unwrap();
        crate::storage::atomic_write(&cache, b"cached").unwrap();
        let part = d.cache_path(&new).unwrap().with_extension("part");
        crate::storage::atomic_write(&part, b"n").unwrap();
        let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
        let captured = events.clone();
        let sink: EventSink = Arc::new(move |event| captured.lock().unwrap().push(event));
        let files = vec![new, cached];
        d.batch(files.clone(), &CancellationToken::new(), &sink)
            .await
            .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].to_lowercase().contains("range: bytes=1-"));
        {
            let events = events.lock().unwrap();
            let planned = &events[0].files;
            assert_eq!(planned.len(), 2);
            assert_eq!((planned[0].downloaded, planned[0].total), (1, 3));
            assert_eq!((planned[1].downloaded, planned[1].total), (6, 6));
            assert!(planned.iter().all(|file| file.status == "queued"));
            assert!(
                events
                    .iter()
                    .flat_map(|event| &event.files)
                    .all(|file| { file.downloaded >= if file.name == "zzz.jar" { 6 } else { 1 } })
            );
            let initial = events
                .iter()
                .find(|event| event.kind == "progress")
                .unwrap();
            assert_eq!(
                (initial.bytes, initial.total_bytes, initial.completed),
                (7, 9, 0)
            );
            assert!(initial.reset_progress);
            let ready = events
                .iter()
                .position(|e| e.kind == "progress" && e.bytes == 7 && e.total_bytes == 9)
                .unwrap();
            let request = events
                .iter()
                .position(|e| e.files.iter().any(|file| file.status == "downloading"))
                .unwrap();
            assert!(ready < request);
            let progress = events.iter().rev().find(|e| e.kind == "progress").unwrap();
            assert_eq!(
                (
                    progress.completed,
                    progress.total,
                    progress.bytes,
                    progress.total_bytes
                ),
                (2, 2, 9, 9)
            );
            assert!(
                events
                    .iter()
                    .flat_map(|event| &event.files)
                    .any(|file| file.name == "zzz.jar" && file.cached)
            );
        }
        events.lock().unwrap().clear();
        // Both endpoints are now unavailable. Repeating the task must use the
        // complete target/cache files without any request, including probes.
        d.batch(files, &CancellationToken::new(), &sink)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        let initial = events
            .iter()
            .find(|event| event.kind == "progress")
            .unwrap();
        assert_eq!(
            (initial.bytes, initial.total_bytes, initial.completed),
            (9, 9, 2)
        );
        let files: Vec<_> = events.iter().flat_map(|event| &event.files).collect();
        assert!(files.iter().all(|file| file.downloaded == file.total));
        assert!(!files.iter().any(|file| file.status == "downloading"));
        assert_eq!(
            files
                .iter()
                .filter(|file| file.status == "done" && file.cached)
                .map(|file| &file.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn corrupt_local_bytes_are_retracted_before_redownloading() {
        let temp = tempfile::tempdir().unwrap();
        let d = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        let (url, server) = crate::test_http::serve(vec![(200, "good".into())]).await;
        let file = FileSpec {
            url,
            path: temp.path().join("game/client.jar"),
            hash: hex::encode(Sha1::digest(b"good")),
            sha256: false,
            size: 4,
        };
        crate::storage::atomic_write(&file.path, b"evil").unwrap();
        let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
        let captured = events.clone();
        let sink: EventSink = Arc::new(move |event| captured.lock().unwrap().push(event));
        d.batch(vec![file.clone()], &CancellationToken::new(), &sink)
            .await
            .unwrap();
        assert_eq!(server.await.unwrap().len(), 1);
        assert_eq!(tokio::fs::read(&file.path).await.unwrap(), b"good");
        let events = events.lock().unwrap();
        assert_eq!(events[0].files[0].downloaded, 4);
        assert_eq!(events[0].files[0].status, "queued");
        let correction = events
            .iter()
            .position(|event| event.kind == "progress" && event.reset_progress && event.bytes == 0)
            .unwrap();
        let request = events
            .iter()
            .position(|event| event.files.iter().any(|file| file.status == "downloading"))
            .unwrap();
        assert!(correction < request);
        assert!(
            !events
                .iter()
                .flat_map(|event| &event.files)
                .any(|file| file.cached)
        );
        // The second check at transfer time must not resurrect the corrupt size.
        assert!(
            events[correction..request]
                .iter()
                .flat_map(|event| &event.files)
                .all(|file| file.downloaded == 0)
        );
    }

    #[tokio::test]
    async fn detects_corruption_and_uses_cache_without_network() {
        let t = tempfile::tempdir().unwrap();
        let d = Downloader::new(t.path(), Settings::default()).unwrap();
        let bytes = b"verified content";
        let hash = hex::encode(Sha1::digest(bytes));
        let cached = t
            .path()
            .join("cache/objects/sha1")
            .join(&hash[..2])
            .join(&hash);
        crate::storage::atomic_write(&cached, bytes).unwrap();
        let f = FileSpec {
            url: "http://127.0.0.1:1/no-network".into(),
            path: t.path().join("game/library.jar"),
            hash,
            sha256: false,
            size: bytes.len() as u64,
        };
        let sink: EventSink = Arc::new(|_| {});
        d.file(&f, &CancellationToken::new(), &sink).await.unwrap();
        assert_eq!(d.verification.hashes.load(Ordering::Relaxed), 1);
        drop(d);
        let d = Downloader::new(t.path(), Settings::default()).unwrap();
        tokio::fs::write(&f.path, b"broken").await.unwrap();
        d.file(&f, &CancellationToken::new(), &sink).await.unwrap();
        assert_eq!(tokio::fs::read(&f.path).await.unwrap(), bytes);
        assert_eq!(
            d.verification.hashes.load(Ordering::Relaxed),
            0,
            "verified cache copy survives restart without rehashing"
        );
        let d = d.with_full_verification();
        d.file(&f, &CancellationToken::new(), &sink).await.unwrap();
        assert_eq!(
            d.verification.hashes.load(Ordering::Relaxed),
            1,
            "repair bypasses the target receipt"
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(d.file(&f, &cancel, &sink).await.is_err());
    }
}
