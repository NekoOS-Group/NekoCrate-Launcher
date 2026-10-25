//! In-memory display summaries, invalidated by the same file identity used for
//! download receipts. Cache hits inspect metadata, never read the archive again.
use super::{ModIdentity, mod_metadata};
use crate::verification::Fingerprint;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_ENTRIES: usize = 2048;
const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Entry {
    fingerprint: Fingerprint,
    value: ModIdentity,
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
pub(crate) struct SummaryCache {
    entries: Mutex<Entries>,
    #[cfg(test)]
    pub reads: std::sync::atomic::AtomicUsize,
}
fn fingerprint(path: &Path) -> Option<Fingerprint> {
    Fingerprint::read_at(path, &fs::metadata(path).ok()?)
}
impl SummaryCache {
    pub(super) fn get(&self, path: &Path) -> Option<ModIdentity> {
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
    pub(super) fn read(&self, path: &Path) -> ModIdentity {
        if let Some(value) = self.get(path) {
            return value;
        }
        let before = fingerprint(path);
        #[cfg(test)]
        self.reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let value = mod_metadata(path).unwrap_or_default();
        if let Some(before) = before
            && fingerprint(path).as_ref() == Some(&before)
        {
            let bytes = path.as_os_str().len()
                + value.name.as_ref().map_or(0, String::len)
                + value.version.as_ref().map_or(0, String::len)
                + value.ids.iter().map(String::len).sum::<usize>()
                + 128;
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
