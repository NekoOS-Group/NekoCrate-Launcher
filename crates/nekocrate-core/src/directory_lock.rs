//! OS locks coordinate installed and portable launchers using the same game tree.
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

#[derive(Default)]
pub(crate) struct Locks(Mutex<HashMap<PathBuf, Weak<DirectoryLock>>>);
pub(crate) struct DirectoryLock(fs::File);
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
impl Locks {
    pub fn acquire(&self, game: &Path) -> Result<Option<Arc<DirectoryLock>>> {
        // Unavailable linked directories must still be removable from the library.
        if !game.is_dir() {
            return Ok(None);
        }
        let game = game.canonicalize()?;
        let mut locks = self.0.lock().unwrap();
        if let Some(guard) = locks.get(&game).and_then(Weak::upgrade) {
            return Ok(Some(guard));
        }
        locks.retain(|_, guard| guard.strong_count() > 0);
        // Keep read-only imports/unlinks from leaving files in the game tree.
        // Both modes derive the same per-user temporary lock from the real path.
        let directory = std::env::temp_dir().join("NekoCrate-directory-locks");
        fs::create_dir_all(&directory)?;
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "游戏锁目录不能是符号链接"
        );
        let name = game.to_string_lossy();
        let name = if cfg!(windows) {
            name.to_lowercase()
        } else {
            name.into_owned()
        };
        let key = hex::encode(ring::digest::digest(&ring::digest::SHA256, name.as_bytes()));
        let path = directory.join(format!("{key}.lock"));
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "游戏目录锁不是普通文件"
            );
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .context("无法锁定游戏目录，请检查目录是否可写")?;
        file.try_lock()
            .context("此游戏目录正由另一个猫咪盒子启动器使用，请等待其任务结束或关闭游戏")?;
        let guard = Arc::new(DirectoryLock(file));
        locks.insert(game, Arc::downgrade(&guard));
        Ok(Some(guard))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_launchers_exclude_conflicting_operations_and_release_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let first = Locks::default();
        let second = Locks::default();
        let guard = first.acquire(temp.path()).unwrap();
        let nested = first.acquire(temp.path()).unwrap();
        assert!(second.acquire(temp.path()).is_err());
        drop(guard);
        assert!(second.acquire(temp.path()).is_err());
        drop(nested);
        assert!(second.acquire(temp.path()).is_ok());
    }
}
