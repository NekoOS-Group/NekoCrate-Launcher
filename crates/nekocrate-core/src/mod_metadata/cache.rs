//! Shared parsed descriptors, invalidated by the same file identity used for
//! download receipts. Cache hits inspect metadata, never read the archive again.
use super::{Metadata, load};
use crate::verification::Fingerprint;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 2048;
const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Entry {
    fingerprint: Fingerprint,
    value: Arc<Metadata>,
    used: u64,
    bytes: usize,
}
#[derive(Default)]
struct Entries {
    files: HashMap<PathBuf, Entry>,
    clock: u64,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    entries: Mutex<Entries>,
}
fn fingerprint(path: &Path) -> Option<Fingerprint> {
    Fingerprint::read_at(path, &fs::metadata(path).ok()?)
}
impl Cache {
    fn get(&self, path: &Path) -> Option<Arc<Metadata>> {
        let stamp = fingerprint(path)?;
        let mut entries = self.entries.lock().unwrap();
        entries.clock += 1;
        let used = entries.clock;
        let entry = entries.files.get_mut(path)?;
        if entry.fingerprint == stamp {
            entry.used = used;
            return Some(entry.value.clone());
        }
        let old = entries.files.remove(path).unwrap();
        entries.bytes -= old.bytes;
        None
    }
    fn read(&self, path: &Path) -> Arc<Metadata> {
        if let Some(value) = self.get(path) {
            return value;
        }
        let before = fingerprint(path);
        let value = Arc::new(load(path));
        if let Some(before) = before
            && fingerprint(path).as_ref() == Some(&before)
        {
            let bytes = path.as_os_str().len() + value.bytes + 128;
            if bytes <= MAX_BYTES {
                let mut entries = self.entries.lock().unwrap();
                if let Some(old) = entries.files.remove(path) {
                    entries.bytes -= old.bytes;
                }
                while entries.files.len() >= MAX_ENTRIES || entries.bytes + bytes > MAX_BYTES {
                    let oldest = entries
                        .files
                        .iter()
                        .min_by_key(|(_, e)| e.used)
                        .map(|(p, _)| p.clone());
                    let Some(oldest) = oldest else {
                        break;
                    };
                    entries.bytes -= entries.files.remove(&oldest).unwrap().bytes;
                }
                entries.clock += 1;
                let used = entries.clock;
                entries.files.insert(
                    path.to_owned(),
                    Entry {
                        fingerprint: before,
                        value: value.clone(),
                        used,
                        bytes,
                    },
                );
                entries.bytes += bytes;
            }
        }
        value
    }
}

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Cache::default)
}
pub(crate) fn get(path: &Path) -> Option<Arc<Metadata>> {
    cache().get(path)
}
pub(crate) fn read(path: &Path) -> Arc<Metadata> {
    cache().read(path)
}
