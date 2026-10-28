//! Download copies belong to an installation, not to an unlimited global cache.
//! A durable claim is written before downloading; only committing the installation
//! releases it. Shared objects survive while another interrupted task needs them.
use crate::{
    download::{FileSpec, object_path},
    storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    instance: String,
    game: PathBuf,
    version: String,
    task: String,
    #[serde(default)]
    complete: bool,
    files: BTreeSet<PathBuf>,
}

pub(crate) struct DownloadTask {
    root: PathBuf,
    path: PathBuf,
    record: Mutex<Record>,
}

fn record_path(root: &Path, instance: &str, version: &str, task: &str) -> PathBuf {
    let key = serde_json::to_vec(&(instance, version, task)).unwrap();
    let hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, &key));
    root.join("cache/download-tasks")
        .join(format!("{hash}.json"))
}

// Refuse linked parents, even when the final file itself is not a link.
pub(crate) fn checked_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let relative = path.strip_prefix(root)?;
    let mut current = root.to_owned();
    for part in relative.components() {
        ensure!(
            matches!(part, std::path::Component::Normal(_)),
            "无效下载暂存路径"
        );
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "下载暂存路径不能包含符号链接"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(current)
}

fn allowed_file(root: &Path, record: &Record, path: &Path) -> Result<()> {
    let roots = [
        root.join("cache/objects"),
        root.join("cache/java"),
        root.join("cache/modpacks"),
        root.join("metadata/loader-installers"),
        record
            .game
            .join("versions")
            .join(&record.version)
            .join(".nekocrate/downloads"),
        record.game.join(".nekocrate/downloads"),
        record
            .game
            .join("versions")
            .join(&record.version)
            .join(".nekocrate/mod-downloads"),
        record.game.join(".nekocrate/mod-downloads"),
        record
            .game
            .join("versions")
            .join(&record.version)
            .join(".nekocrate/source.zip"),
    ];
    let base = roots
        .iter()
        .find(|base| path.starts_with(base))
        .context("下载暂存文件不在受管目录内")?;
    let anchor = if base.starts_with(root) {
        root
    } else {
        &record.game
    };
    checked_path(anchor, path)?;
    // Loader metadata and processor outputs remain useful; only the archive is disposable.
    if base == &root.join("metadata/loader-installers") {
        ensure!(
            path.file_name().is_some_and(|n| n == "installer.jar"),
            "不能清理加载器元数据"
        );
    }
    Ok(())
}

impl DownloadTask {
    pub fn new(
        root: &Path,
        instance: &str,
        game: &Path,
        version: &str,
        task: &str,
    ) -> Result<Arc<Self>> {
        storage::safe_id(instance)?;
        storage::safe_version(version)?;
        let version_directory = checked_path(game, &game.join("versions").join(version))?;
        fs::create_dir_all(version_directory)?;
        let path = record_path(root, instance, version, task);
        checked_path(root, &path)?;
        let record = if path.try_exists()? {
            let record: Record = storage::read_json(&path)?;
            ensure!(
                record.instance == instance
                    && record.game == game
                    && record.version == version
                    && record.task == task,
                "下载任务归属不匹配"
            );
            record
        } else {
            Record {
                instance: instance.into(),
                game: game.into(),
                version: version.into(),
                task: task.into(),
                complete: false,
                files: BTreeSet::new(),
            }
        };
        Ok(Arc::new(Self {
            root: root.into(),
            path,
            record: Mutex::new(record),
        }))
    }

    pub async fn track(self: &Arc<Self>, cache: &Path, files: &[FileSpec]) -> Result<()> {
        let mut paths = BTreeSet::new();
        for file in files {
            let object = object_path(cache, file)?;
            paths.insert(object.with_extension("part"));
            paths.insert(object);
            // Successful consumption also releases installer archives themselves.
            if file.path.starts_with(self.root.join("cache/java"))
                || (file
                    .path
                    .starts_with(self.root.join("metadata/loader-installers"))
                    && file.path.file_name().is_some_and(|n| n == "installer.jar"))
            {
                paths.insert(file.path.clone());
            }
            // A legacy global copy may be reused by a task with a version-local cache.
            let old = object_path(&self.root.join("cache/objects"), file)?;
            if old != object_path(cache, file)? && old.is_file() {
                paths.insert(old);
            }
        }
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut record = this.record.lock().unwrap();
            for path in &paths {
                allowed_file(&this.root, &record, path)?;
            }
            if paths.is_subset(&record.files) && !record.complete && this.path.is_file() {
                return Ok(());
            }
            record.files.extend(paths);
            record.complete = false;
            storage::write_json(&this.path, &*record)
        })
        .await?
    }

    pub async fn complete(self: &Arc<Self>) -> Result<Vec<PathBuf>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut record = this.record.lock().unwrap();
            record.complete = true;
            storage::write_json(&this.path, &*record)?;
            release(&this.root, |path, _| path == this.path)
        })
        .await?
    }

    pub async fn track_extra(self: &Arc<Self>, paths: Vec<PathBuf>) -> Result<()> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut record = this.record.lock().unwrap();
            for path in &paths {
                allowed_file(&this.root, &record, path)?;
            }
            record.files.extend(paths);
            record.complete = false;
            storage::write_json(&this.path, &*record)
        })
        .await?
    }
}

fn records(root: &Path) -> Result<Vec<(PathBuf, Record)>> {
    let directory = checked_path(root, &root.join("cache/download-tasks"))?;
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let record: Record = storage::read_json(&entry.path())?;
        storage::safe_id(&record.instance)?;
        storage::safe_version(&record.version)?;
        ensure!(
            record_path(root, &record.instance, &record.version, &record.task) == entry.path(),
            "下载任务记录不匹配"
        );
        records.push((entry.path(), record));
    }
    Ok(records)
}

fn release(root: &Path, selected: impl Fn(&Path, &Record) -> bool) -> Result<Vec<PathBuf>> {
    let records = records(root)?;
    let protected: BTreeSet<_> = records
        .iter()
        .filter(|(p, r)| !selected(p, r) && !r.complete)
        .flat_map(|(_, r)| r.files.iter().cloned())
        .collect();
    let mut removed = Vec::new();
    for (path, record) in records.iter().filter(|(p, r)| selected(p, r)) {
        for file in record.files.difference(&protected) {
            allowed_file(root, record, file)?;
            match fs::remove_file(file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            removed.push(file.clone());
        }
        fs::remove_file(path)?;
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }
    Ok(removed)
}

pub(crate) fn remove_owner(root: &Path, instance: &str, version: Option<&str>) -> Result<()> {
    release(root, |_, r| {
        r.instance == instance && version.is_none_or(|v| r.version == v)
    })?;
    Ok(())
}

pub(crate) fn local_objects(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for (_, record) in records(root)? {
        for file in &record.files {
            if file.starts_with(record.game.join(".nekocrate/downloads"))
                || file.starts_with(
                    record
                        .game
                        .join("versions")
                        .join(&record.version)
                        .join(".nekocrate/downloads"),
                )
            {
                allowed_file(root, &record, file)?;
                if file.is_file() {
                    files.insert(file.clone());
                }
            }
        }
    }
    Ok(files)
}

pub(crate) fn remove_local_objects(root: &Path, mut files: BTreeSet<PathBuf>) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    for (_, record) in records(root)? {
        let matched: Vec<_> = record.files.intersection(&files).cloned().collect();
        for file in matched {
            allowed_file(root, &record, &file)?;
            fs::remove_file(&file)?;
            files.remove(&file);
        }
    }
    ensure!(files.is_empty(), "下载暂存文件不属于当前任务");
    Ok(())
}

pub(crate) fn pending_files(root: &Path) -> Result<BTreeSet<PathBuf>> {
    Ok(records(root)?
        .into_iter()
        .filter(|(_, record)| !record.complete)
        .flat_map(|(_, record)| record.files)
        .collect())
}

/// Finish interrupted cleanup. Missing external disks do not mean deleted versions.
pub(crate) fn reconcile(root: &Path, instances: &[crate::model::Instance]) -> Result<()> {
    let orphaned = |r: &Record| {
        !instances
            .iter()
            .any(|i| i.id == r.instance && i.versions.iter().any(|v| v.id == r.version))
            || (r.game.is_dir()
                && fs::symlink_metadata(r.game.join("versions").join(&r.version))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound))
    };
    let removed: Vec<_> = records(root)?
        .into_iter()
        .filter(|(_, r)| orphaned(r))
        .map(|(_, r)| r)
        .collect();
    release(root, |_, r| r.complete || orphaned(r))?;
    for record in removed {
        crate::resources::remove_version_tasks(
            &record.game,
            &record.instance,
            Some(&record.version),
        )?;
    }
    Ok(())
}

/// Drop disposable bytes instead of sending them to the Trash with saves/mods.
/// Rename on the same volume, then roll back if the OS rejects recycling.
pub(crate) fn recycle_without_staging(
    root: &Path,
    source: &Path,
    recycle: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        return recycle(source);
    }
    let mut paths = Vec::new();
    for name in ["downloads", "mod-downloads", "resource-tasks", "source.zip"] {
        let path = checked_path(source, &source.join(".nekocrate").join(name))?;
        if path.try_exists()? {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return recycle(source);
    }
    let staging = source
        .parent()
        .context("无效版本目录")?
        .join(format!(".nekocrate-discard-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let mut moved = Vec::new();
    let result = (|| {
        for (index, path) in paths.iter().enumerate() {
            let target = staging.join(index.to_string());
            fs::rename(path, &target)?;
            moved.push((path, target));
        }
        recycle(source)
    })();
    if let Err(error) = result {
        for (path, target) in moved.iter().rev() {
            fs::rename(target, path)
                .with_context(|| format!("恢复暂存文件失败，文件仍在 {}", staging.display()))?;
        }
        let _ = fs::remove_dir(&staging);
        return Err(error);
    }
    // Recycling has committed. A cleanup failure must not roll back the database
    // to a version that is now in the OS Trash.
    if let Err(error) = fs::remove_dir_all(&staging) {
        let error = anyhow::Error::from(error)
            .context(format!("清理下载暂存目录失败 {}", staging.display()));
        let _ = crate::diagnostics::record_error(root, "download_cleanup", &error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Launcher, download::Downloader, model::Settings};
    use sha1::{Digest, Sha1};
    use tokio_util::sync::CancellationToken;

    fn fixture(root: &Path) -> (Launcher, crate::model::Instance, FileSpec) {
        let launcher = Launcher::open(Some(root.into())).unwrap();
        let instance = launcher.create_instance("test", "one").unwrap();
        launcher.add_version(&instance.id, "two").unwrap();
        let spec = FileSpec {
            path: instance.game_dir(root).join("libraries/file.jar"),
            url: "http://127.0.0.1:1/must-not-fetch".into(),
            hash: hex::encode(Sha1::digest(b"verified")),
            sha256: false,
            size: 8,
        };
        (launcher, instance, spec)
    }
    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[tokio::test]
    async fn success_releases_copies_only_after_all_pending_owners_finish() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let (_launcher, instance, spec) = fixture(root);
        let cache = root.join("cache/objects");
        let object = object_path(&cache, &spec).unwrap();
        let part = object.with_extension("part");
        put(&object, b"verified");
        put(&part, b"partial");
        put(&spec.path, b"verified");
        let first =
            DownloadTask::new(root, &instance.id, &instance.game_dir(root), "one", "game").unwrap();
        let second =
            DownloadTask::new(root, &instance.id, &instance.game_dir(root), "two", "game").unwrap();
        first
            .track(&cache, std::slice::from_ref(&spec))
            .await
            .unwrap();
        second
            .track(&cache, std::slice::from_ref(&spec))
            .await
            .unwrap();
        first.complete().await.unwrap();
        assert!(
            object.is_file() && part.is_file(),
            "paused owner still needs both copies"
        );
        drop(second);
        let second =
            DownloadTask::new(root, &instance.id, &instance.game_dir(root), "two", "game").unwrap();
        second.complete().await.unwrap();
        assert!(!object.exists() && !part.exists());
        assert_eq!(fs::read(&spec.path).unwrap(), b"verified");
    }

    #[tokio::test]
    async fn removing_a_version_discards_its_partial_but_failed_recycling_preserves_it() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let (launcher, instance, spec) = fixture(root);
        let game = instance.game_dir(root);
        let task = DownloadTask::new(root, &instance.id, &game, "one", "game").unwrap();
        task.track(&root.join("cache/objects"), std::slice::from_ref(&spec))
            .await
            .unwrap();
        let part = object_path(&root.join("cache/objects"), &spec)
            .unwrap()
            .with_extension("part");
        put(&part, b"unfinished");
        let local = game.join("versions/one/.nekocrate/downloads/keep.part");
        put(&local, b"pack partial");
        put(&game.join("versions/one/saves/world"), b"save");
        assert!(
            launcher
                .delete_version_with_recycle(&instance.id, "one", |_| anyhow::bail!(
                    "cannot recycle"
                ))
                .is_err()
        );
        assert!(part.is_file() && local.is_file());
        let recycled = root.join("fake-trash");
        launcher
            .delete_version_with_recycle(&instance.id, "one", |p| {
                fs::rename(p, &recycled)?;
                Ok(())
            })
            .unwrap();
        assert!(!part.exists());
        assert!(!recycled.join(".nekocrate/downloads").exists());
        assert_eq!(fs::read(recycled.join("saves/world")).unwrap(), b"save");
        assert!(records(root).unwrap().is_empty());
    }

    #[tokio::test]
    async fn downloads_reuse_indexed_installed_files_after_cache_cleanup_and_restart() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let (_launcher, instance, spec) = fixture(root);
        put(&spec.path, b"verified");
        let d = Downloader::new(root, Settings::default())
            .unwrap()
            .for_installation(root, &instance.id, &instance.game_dir(root), "one", "game")
            .unwrap();
        let sink: crate::model::EventSink = Arc::new(|_| {});
        let cancel = CancellationToken::new();
        d.file(&spec, &cancel, &sink).await.unwrap();
        d.finish_installation(&sink).await;
        drop(d);
        let d = Downloader::new(root, Settings::default()).unwrap();
        let second = FileSpec {
            path: root.join("other-game/mods/renamed.jar"),
            ..spec.clone()
        };
        d.file(&second, &cancel, &sink).await.unwrap();
        assert_eq!(fs::read(&second.path).unwrap(), b"verified");
        assert!(
            !object_path(&root.join("cache/objects"), &spec)
                .unwrap()
                .exists()
        );
        // An indexed filename is not proof: changed bytes must never be reused.
        fs::write(&spec.path, b"corrupt!").unwrap();
        fs::remove_file(&second.path).unwrap();
        let third = FileSpec {
            path: root.join("third-game/mods/mod.jar"),
            ..spec
        };
        assert!(d.file(&third, &cancel, &sink).await.is_err());
        assert!(!third.path.exists());
    }

    #[tokio::test]
    async fn removal_outside_the_launcher_cleans_claims_but_offline_disks_do_not() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let (launcher, instance, spec) = fixture(root);
        let game = instance.game_dir(root);
        let task = DownloadTask::new(root, &instance.id, &game, "one", "game").unwrap();
        task.track(&root.join("cache/objects"), std::slice::from_ref(&spec))
            .await
            .unwrap();
        let object = object_path(&root.join("cache/objects"), &spec).unwrap();
        put(&object, b"verified");
        let offline = root.join("offline");
        fs::rename(&game, &offline).unwrap();
        reconcile(root, &launcher.snapshot().database.instances).unwrap();
        assert!(object.is_file());
        fs::rename(offline, &game).unwrap();
        fs::remove_dir_all(game.join("versions/one")).unwrap();
        reconcile(root, &launcher.snapshot().database.instances).unwrap();
        assert!(!object.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linked_staging_is_never_followed_or_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let (_launcher, instance, spec) = fixture(root);
        let task =
            DownloadTask::new(root, &instance.id, &instance.game_dir(root), "one", "game").unwrap();
        let outside = root.join("keep");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("file"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("cache/objects")).unwrap();
        assert!(
            task.track(&root.join("cache/objects"), &[spec])
                .await
                .is_err()
        );
        assert_eq!(fs::read(outside.join("file")).unwrap(), b"keep");
    }
}
