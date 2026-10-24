use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Scans are read-only and must never occupy or cancel the download job slot.
#[derive(Clone, Default)]
pub struct FolderScanManager(Arc<Mutex<Option<(String, CancellationToken)>>>);

pub struct ScanGuard {
    manager: FolderScanManager,
    id: String,
    pub cancel: CancellationToken,
}

impl FolderScanManager {
    pub fn start(&self, id: String) -> ScanGuard {
        let cancel = CancellationToken::new();
        let mut current = self.0.lock().unwrap();
        if let Some((_, previous)) = current.replace((id.clone(), cancel.clone())) {
            previous.cancel();
        }
        ScanGuard {
            manager: self.clone(),
            id,
            cancel,
        }
    }

    pub fn cancel(&self, id: Option<&str>) {
        let current = self.0.lock().unwrap();
        if let Some((current_id, cancel)) = current.as_ref()
            && id.is_none_or(|id| id == current_id)
        {
            cancel.cancel();
        }
    }
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
        let mut current = self.manager.0.lock().unwrap();
        if current.as_ref().is_some_and(|(id, _)| id == &self.id) {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_completion_and_cancel_cannot_interrupt_new_scan() {
        let manager = FolderScanManager::default();
        let old = manager.start("old".into());
        let new = manager.start("new".into());
        assert!(old.cancel.is_cancelled());
        drop(old);
        manager.cancel(Some("old"));
        assert!(!new.cancel.is_cancelled());
        manager.cancel(Some("new"));
        assert!(new.cancel.is_cancelled());
        drop(new);
        assert!(manager.0.lock().unwrap().is_none());
    }
}
