//! Reuse successful hashes only while the file's identity and metadata match.
//! Explicit repair bypasses receipts; new/unrecorded files always get hashed.
use anyhow::Result;
use ring::digest::{Context as HashContext, SHA256};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fingerprint {
    size: u64,
    modified: (u64, u32),
    created: Option<(u64, u32)>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
    #[cfg(windows)]
    identity: (u64, [u8; 16], i64),
}
impl Fingerprint {
    fn read(meta: &fs::Metadata, _file: Option<&fs::File>) -> Option<Self> {
        let time = |value: SystemTime| {
            value
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| (d.as_secs(), d.subsec_nanos()))
        };
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            size: meta.len(),
            modified: time(meta.modified().ok()?)?,
            created: meta.created().ok().and_then(time),
            #[cfg(unix)]
            identity: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
            #[cfg(windows)]
            identity: windows_identity(_file?)?,
        })
    }

    pub(crate) fn read_at(_path: &Path, meta: &fs::Metadata) -> Option<Self> {
        if !meta.is_file() {
            return None;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
            // Metadata-only access: warm cache hits never read file contents.
            let file = fs::OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .open(_path)
                .ok()?;
            Self::read(&file.metadata().ok()?, Some(&file))
        }
        #[cfg(not(windows))]
        Self::read(meta, None)
    }

    async fn snapshot(path: &Path) -> Result<Option<Self>> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let meta = fs::symlink_metadata(&path)?;
            Ok(Self::read_at(&path, &meta))
        })
        .await?
    }
}

#[cfg(windows)]
fn windows_identity(file: &fs::File) -> Option<(u64, [u8; 16], i64)> {
    use std::{mem::MaybeUninit, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FILE_ID_INFO, FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut id = MaybeUninit::<FILE_ID_INFO>::uninit();
    let mut basic = MaybeUninit::<FILE_BASIC_INFO>::uninit();
    // SAFETY: the handle remains open and each output buffer matches the requested
    // Win32 information class and size. Read the buffers only after both calls succeed.
    unsafe {
        if GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            id.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        ) == 0
            || GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileBasicInfo,
                basic.as_mut_ptr().cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            ) == 0
        {
            // Filesystems without reliable identity/change metadata always hash.
            return None;
        }
        let id = id.assume_init();
        Some((
            id.VolumeSerialNumber,
            id.FileId.Identifier,
            basic.assume_init().ChangeTime,
        ))
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    hash: String,
    sha256: bool,
    fingerprint: Fingerprint,
}
#[derive(Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    entries: HashMap<PathBuf, Receipt>,
}

#[derive(Default)]
struct Receipts {
    paths: HashMap<PathBuf, Receipt>,
    hashes: HashMap<(bool, String), HashSet<PathBuf>>,
}
impl Receipts {
    fn insert(&mut self, path: PathBuf, receipt: Receipt) {
        self.remove(&path);
        self.hashes
            .entry((receipt.sha256, receipt.hash.to_ascii_lowercase()))
            .or_default()
            .insert(path.clone());
        self.paths.insert(path, receipt);
    }
    fn remove(&mut self, path: &Path) -> Option<Receipt> {
        let old = self.paths.remove(path)?;
        let key = (old.sha256, old.hash.to_ascii_lowercase());
        if let Some(paths) = self.hashes.get_mut(&key) {
            paths.remove(path);
            if paths.is_empty() {
                self.hashes.remove(&key);
            }
        }
        Some(old)
    }
}
impl std::ops::Deref for Receipts {
    type Target = HashMap<PathBuf, Receipt>;
    fn deref(&self) -> &Self::Target {
        &self.paths
    }
}

/// Read only valid receipts for legacy archive cleanup; no payload rehash on a hit.
pub(crate) fn recorded_sha256(root: &Path, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    let index = fs::read(root.join("cache/verified-files.json"))
        .ok()
        .and_then(|bytes| crate::portable::from_slice::<Index>(root, &bytes).ok())
        .filter(|index| index.version == 1)
        .unwrap_or_default();
    paths
        .iter()
        .filter_map(|path| {
            let receipt = index.entries.get(path)?;
            if !receipt.sha256
                || receipt.hash.len() != 64
                || !receipt.hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
            let metadata = fs::symlink_metadata(path).ok()?;
            (Fingerprint::read_at(path, &metadata)? == receipt.fingerprint)
                .then(|| (path.clone(), receipt.hash.to_ascii_lowercase()))
        })
        .collect()
}

pub(crate) struct VerificationCache {
    path: PathBuf,
    loaded: OnceCell<()>,
    entries: Mutex<Receipts>,
    generation: AtomicU64,
    saved: AtomicU64,
    writer: AsyncMutex<()>,
    readers: Arc<Semaphore>,
    #[cfg(test)]
    pub hashes: AtomicU64,
}
impl VerificationCache {
    pub fn shared(root: &Path) -> Arc<Self> {
        static STORES: OnceLock<Mutex<HashMap<PathBuf, Weak<VerificationCache>>>> = OnceLock::new();
        let mut stores = STORES.get_or_init(Mutex::default).lock().unwrap();
        let path = root.join("cache/verified-files.json");
        if let Some(store) = stores.get(&path).and_then(Weak::upgrade) {
            return store;
        }
        stores.retain(|_, store| store.strong_count() > 0);
        let store = Arc::new(Self {
            path: path.clone(),
            loaded: OnceCell::new(),
            entries: Mutex::default(),
            generation: AtomicU64::new(0),
            saved: AtomicU64::new(0),
            writer: AsyncMutex::new(()),
            readers: Arc::new(Semaphore::new(8)),
            #[cfg(test)]
            hashes: AtomicU64::new(0),
        });
        stores.insert(path, Arc::downgrade(&store));
        store
    }
    async fn load(&self) {
        self.loaded
            .get_or_init(|| async {
                let root = self.path.parent().unwrap().parent().unwrap();
                let index = tokio::fs::read(&self.path)
                    .await
                    .ok()
                    .and_then(|bytes| crate::portable::from_slice::<Index>(root, &bytes).ok())
                    .filter(|index| index.version == 1)
                    .unwrap_or_default();
                let mut receipts = Receipts::default();
                for (path, receipt) in index.entries {
                    receipts.insert(path, receipt);
                }
                *self.entries.lock().unwrap() = receipts;
            })
            .await;
    }
    /// Metadata-only receipt lookup, used to seed resumed progress without rehashing.
    pub async fn matches_receipt(&self, path: &Path, hash: &str, sha256: bool, size: u64) -> bool {
        self.load().await;
        let receipt = self.entries.lock().unwrap().get(path).cloned();
        let Some(receipt) = receipt else {
            return false;
        };
        if !receipt.hash.eq_ignore_ascii_case(hash)
            || receipt.sha256 != sha256
            || (size > 0 && receipt.fingerprint.size != size)
        {
            return false;
        }
        Fingerprint::snapshot(path)
            .await
            .ok()
            .flatten()
            .is_some_and(|current| current == receipt.fingerprint)
    }
    /// Content lookup is indexed, never a scan of game trees or all receipts per file.
    /// Candidates are only hints: the downloader must verify before copying.
    pub async fn candidates(&self, hash: &str, sha256: bool, size: u64) -> Vec<PathBuf> {
        self.load().await;
        let entries = self.entries.lock().unwrap();
        let mut paths: Vec<_> = entries
            .hashes
            .get(&(sha256, hash.to_ascii_lowercase()))
            .into_iter()
            .flatten()
            .filter(|path| size == 0 || entries.paths[*path].fingerprint.size == size)
            .cloned()
            .collect();
        paths.sort();
        paths
    }

    pub async fn forget_tree(&self, directory: &Path) {
        self.load().await;
        let mut entries = self.entries.lock().unwrap();
        let paths: Vec<_> = entries
            .keys()
            .filter(|p| p.starts_with(directory))
            .cloned()
            .collect();
        for path in paths {
            entries.remove(&path);
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Index imported local files in the SHA-1 namespace used by game/CDN manifests.
    pub async fn index_local(self: &Arc<Self>, path: &Path) -> Result<()> {
        self.load().await;
        let store = self.clone();
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let meta = fs::symlink_metadata(&path)?;
            let Some(before) = Fingerprint::read_at(&path, &meta) else {
                return Ok(());
            };
            if store
                .entries
                .lock()
                .unwrap()
                .get(&path)
                .is_some_and(|r| !r.sha256 && r.fingerprint == before)
            {
                return Ok(());
            }
            let mut input = fs::File::open(&path)?;
            let mut digest = Sha1::new();
            let mut buffer = vec![0; 256 * 1024];
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                digest.update(&buffer[..n]);
            }
            if Fingerprint::read(&input.metadata()?, Some(&input)) == Some(before.clone())
                && Fingerprint::read_at(&path, &fs::symlink_metadata(&path)?)
                    == Some(before.clone())
            {
                store.entries.lock().unwrap().insert(
                    path,
                    Receipt {
                        hash: hex::encode(digest.finalize()),
                        sha256: false,
                        fingerprint: before,
                    },
                );
                store.generation.fetch_add(1, Ordering::Relaxed);
            }
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
    pub async fn verify(
        self: &Arc<Self>,
        path: &Path,
        hash: &str,
        sha256: bool,
        size: u64,
        force: bool,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        self.load().await;
        let permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("操作已取消"),
            permit = self.readers.clone().acquire_owned() => permit?,
        };
        let store = self.clone();
        let path = path.to_owned();
        let hash = hash.to_owned();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            inspect(&path, &hash, sha256, size, Some((&store, force)), &cancel)
        })
        .await?
    }
    // One atomic snapshot per standalone file/batch, including failure/cancel.
    // A crash before this checkpoint merely causes a conservative rehash.
    pub async fn flush(self: &Arc<Self>) -> Result<()> {
        let _writer = self.writer.lock().await;
        if self.generation.load(Ordering::Relaxed) == self.saved.load(Ordering::Relaxed) {
            return Ok(());
        }
        let store = self.clone();
        let revision = tokio::task::spawn_blocking(move || {
            let entries = store.entries.lock().unwrap();
            let revision = store.generation.load(Ordering::Relaxed);
            let index = Index {
                version: 1,
                entries: entries.paths.clone(),
            };
            drop(entries);
            let root = store.path.parent().unwrap().parent().unwrap();
            let bytes = crate::portable::to_vec(root, &index)?;
            crate::storage::atomic_write(&store.path, &bytes)?;
            Ok::<_, anyhow::Error>(revision)
        })
        .await??;
        self.saved.store(revision, Ordering::Relaxed);
        Ok(())
    }
    // Only for freshly verified objects just renamed/copied by the downloader.
    pub async fn remember(&self, path: &Path, hash: &str, sha256: bool) -> Result<()> {
        self.load().await;
        if let Some(fingerprint) = Fingerprint::snapshot(path).await? {
            let mut entries = self.entries.lock().unwrap();
            entries.insert(
                path.to_owned(),
                Receipt {
                    hash: hash.to_owned(),
                    sha256,
                    fingerprint,
                },
            );
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
    pub fn forget(&self, path: &Path) {
        let mut entries = self.entries.lock().unwrap();
        if entries.remove(path).is_some() {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub async fn copy_verified(
        self: &Arc<Self>,
        source: &Path,
        destination: &Path,
        hash: &str,
        sha256: bool,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.load().await;
        let before = Fingerprint::snapshot(source).await?;
        let trusted = before.as_ref().is_some_and(|stamp| {
            self.entries
                .lock()
                .unwrap()
                .get(source)
                .is_some_and(|receipt| {
                    receipt.hash.eq_ignore_ascii_case(hash)
                        && receipt.sha256 == sha256
                        && &receipt.fingerprint == stamp
                })
        });
        tokio::fs::copy(source, destination).await?;
        crate::download::check_cancel(cancel)?;
        let after = Fingerprint::snapshot(source).await?;
        if (!trusted || before != after)
            && !self
                .verify(destination, hash, sha256, 0, true, cancel)
                .await?
        {
            anyhow::bail!("下载文件校验失败");
        }
        Ok(())
    }
}

pub(crate) async fn verify_full(path: &Path, hash: &str, sha256: bool, size: u64) -> Result<bool> {
    let path = path.to_owned();
    let hash = hash.to_owned();
    tokio::task::spawn_blocking(move || {
        inspect(&path, &hash, sha256, size, None, &CancellationToken::new())
    })
    .await?
}

fn inspect(
    path: &Path,
    hash: &str,
    sha256: bool,
    size: u64,
    store: Option<(&VerificationCache, bool)>,
    cancel: &CancellationToken,
) -> Result<bool> {
    crate::download::check_cancel(cancel)?;
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || (size > 0 && meta.len() != size) {
        return Ok(false);
    }
    let before = Fingerprint::read_at(path, &meta);
    if let (Some((store, false)), Some(fingerprint)) = (store, &before)
        && store
            .entries
            .lock()
            .unwrap()
            .get(path)
            .is_some_and(|receipt| {
                receipt.sha256 == sha256
                    && receipt.hash.eq_ignore_ascii_case(hash)
                    && &receipt.fingerprint == fingerprint
            })
    {
        return Ok(true);
    }
    let mut file = fs::File::open(path)?;
    #[cfg(test)]
    if let Some((store, _)) = store {
        store.hashes.fetch_add(1, Ordering::Relaxed);
    }
    let mut a = Sha1::new();
    let mut b = HashContext::new(&SHA256);
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        crate::download::check_cancel(cancel)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if sha256 {
            b.update(&buffer[..n]);
        } else {
            a.update(&buffer[..n]);
        }
    }
    let matched = (if sha256 {
        hex::encode(b.finish())
    } else {
        hex::encode(a.finalize())
    })
    .eq_ignore_ascii_case(hash);
    let unchanged = before.is_some()
        && before == Fingerprint::read(&file.metadata()?, Some(&file))
        && fs::symlink_metadata(path)
            .ok()
            .is_some_and(|meta| Fingerprint::read_at(path, &meta) == before);
    let usable = matched && (unchanged || before.is_none());
    if let Some((store, _)) = store {
        let mut entries = store.entries.lock().unwrap();
        if matched && unchanged {
            entries.insert(
                path.to_owned(),
                Receipt {
                    hash: hash.to_owned(),
                    sha256,
                    fingerprint: before.unwrap(),
                },
            );
            store.generation.fetch_add(1, Ordering::Relaxed);
        } else if entries.remove(path).is_some() {
            store.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
    // If timestamps cannot be read, never memoize; the computed hash is usable.
    Ok(usable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::digest::digest;

    #[tokio::test]
    async fn sha256_file_hashes_match_known_vectors_across_read_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("payload");
        for (bytes, expected) in [
            (
                b"abc".to_vec(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                vec![b'a'; 1_000_000],
                "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            ),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(
                verify_full(&path, &expected.to_uppercase(), true, bytes.len() as u64)
                    .await
                    .unwrap()
            );
            fs::write(&path, vec![b'b'; bytes.len()]).unwrap();
            assert!(
                !verify_full(&path, expected, true, bytes.len() as u64)
                    .await
                    .unwrap()
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn legacy_windows_receipts_without_file_identity_are_rehashed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("asset");
        fs::write(&path, b"good").unwrap();
        let hash = hex::encode(Sha1::digest(b"good"));
        let cancel = CancellationToken::new();
        let store = VerificationCache::shared(temp.path());
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        store.flush().await.unwrap();
        let index_path = store.path.clone();
        drop(store);
        let mut index: serde_json::Value =
            serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
        for receipt in index["entries"].as_object_mut().unwrap().values_mut() {
            receipt["fingerprint"]
                .as_object_mut()
                .unwrap()
                .remove("identity");
        }
        fs::write(index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        let store = VerificationCache::shared(temp.path());
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 1);
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(
            store.hashes.load(Ordering::Relaxed),
            1,
            "new receipts still reuse unchanged files"
        );
    }

    #[tokio::test]
    async fn persisted_receipts_skip_reads_but_repair_and_content_changes_rehash() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("asset");
        fs::write(&path, b"good").unwrap();
        let hash = hex::encode(Sha1::digest(b"good"));
        let cancel = CancellationToken::new();
        let store = VerificationCache::shared(temp.path());
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 1);
        store.flush().await.unwrap();
        let weak = Arc::downgrade(&store);
        drop(store);
        assert!(weak.upgrade().is_none());
        let store = VerificationCache::shared(temp.path());
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(
            store.hashes.load(Ordering::Relaxed),
            0,
            "restart uses the persisted receipt"
        );
        assert!(
            store
                .verify(&path, &hash, false, 4, true, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(
            store.hashes.load(Ordering::Relaxed),
            1,
            "explicit repair always hashes"
        );
        let old = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"evil").unwrap();
        // Both Unix ctime and Windows ChangeTime detect restored modification times.
        #[cfg(any(unix, windows))]
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        #[cfg(not(any(unix, windows)))]
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old + std::time::Duration::from_secs(2))
            .unwrap();
        assert!(
            !store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 2);
        assert!(!store.entries.lock().unwrap().contains_key(&path));
        fs::write(&path, b"short").unwrap();
        assert!(
            !store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(
            store.hashes.load(Ordering::Relaxed),
            2,
            "size mismatch needs no payload read"
        );
    }

    #[tokio::test]
    async fn damaged_index_and_changed_hash_fail_closed_and_sha256_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("cache")).unwrap();
        fs::write(temp.path().join("cache/verified-files.json"), b"{broken").unwrap();
        let path = temp.path().join("runtime.zip");
        fs::write(&path, b"runtime").unwrap();
        let store = VerificationCache::shared(temp.path());
        let cancel = CancellationToken::new();
        let hash = hex::encode(digest(&SHA256, b"runtime"));
        assert!(
            store
                .verify(&path, &hash, true, 7, false, &cancel)
                .await
                .unwrap()
        );
        assert!(
            store
                .verify(&path, &hash, true, 7, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 1);
        assert!(
            !store
                .verify(&path, &"0".repeat(64), true, 7, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 2);
        assert!(store.entries.lock().unwrap().is_empty());
        cancel.cancel();
        assert!(
            store
                .verify(&path, &hash, true, 7, false, &cancel)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn replacing_file_with_same_size_and_mtime_invalidates_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("asset");
        fs::write(&path, b"good").unwrap();
        let store = VerificationCache::shared(temp.path());
        let cancel = CancellationToken::new();
        let hash = hex::encode(Sha1::digest(b"good"));
        assert!(
            store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        let old = fs::metadata(&path).unwrap().modified().unwrap();
        let replacement = temp.path().join("replacement");
        fs::write(&replacement, b"evil").unwrap();
        // Windows requires a writable handle to change file timestamps.
        fs::OpenOptions::new()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert!(
            !store
                .verify(&path, &hash, false, 4, false, &cancel)
                .await
                .unwrap()
        );
        assert_eq!(store.hashes.load(Ordering::Relaxed), 2);
    }
}
