//! Reuse successful hashes only while the file's identity and metadata match.
//! Explicit repair bypasses receipts; new/unrecorded files always get hashed.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::{
    collections::HashMap,
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
struct Fingerprint {
    size: u64,
    modified: (u64, u32),
    created: Option<(u64, u32)>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
impl Fingerprint {
    fn read(meta: &fs::Metadata) -> Option<Self> {
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
        })
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

pub(crate) struct VerificationCache {
    path: PathBuf,
    loaded: OnceCell<()>,
    entries: Mutex<HashMap<PathBuf, Receipt>>,
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
                let index = tokio::fs::read(&self.path)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
                    .filter(|index| index.version == 1)
                    .unwrap_or_default();
                *self.entries.lock().unwrap() = index.entries;
            })
            .await;
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
                entries: entries.clone(),
            };
            drop(entries);
            let bytes = serde_json::to_vec(&index)?;
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
        let meta = tokio::fs::symlink_metadata(path).await?;
        if meta.is_file()
            && let Some(fingerprint) = Fingerprint::read(&meta)
        {
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
        let before = Fingerprint::read(&tokio::fs::symlink_metadata(source).await?);
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
        let after = Fingerprint::read(&tokio::fs::symlink_metadata(source).await?);
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
    let before = Fingerprint::read(&meta);
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
    let mut b = Sha256::new();
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
        hex::encode(b.finalize())
    } else {
        hex::encode(a.finalize())
    })
    .eq_ignore_ascii_case(hash);
    let unchanged = before.is_some()
        && before == Fingerprint::read(&file.metadata()?)
        && fs::symlink_metadata(path)
            .ok()
            .is_some_and(|meta| meta.is_file() && Fingerprint::read(&meta) == before);
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
        // On Unix even a same-size modification with restored mtime changes ctime.
        // Other platforms invalidate through the changed modification timestamp.
        #[cfg(unix)]
        fs::File::open(&path).unwrap().set_modified(old).unwrap();
        #[cfg(not(unix))]
        fs::File::open(&path)
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
        let hash = hex::encode(Sha256::digest(b"runtime"));
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
        fs::File::open(&replacement)
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
