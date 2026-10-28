//! System recycle bin operations. No private trash directory or permanent-delete fallback.
use crate::{Launcher, model::Database, storage};
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub(crate) fn move_to_trash(source: &Path) -> Result<()> {
    // The library canonicalizes the parent only, so a symlink is recycled as a
    // link, never as its target. Native shell APIs also handle non-ASCII paths.
    let context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    let context = {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = context;
        // Use the native API without requesting Finder automation permission.
        context.set_delete_method(DeleteMethod::NsFileManager);
        context
    };
    context
        .delete(source)
        .context("无法移入系统回收站，请检查权限或关闭占用文件的程序")
}

impl Launcher {
    pub(crate) fn mutate_recycling<T>(
        &self,
        source: &Path,
        update: impl FnOnce(&mut Database) -> Result<T>,
        recycle: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<T> {
        let exists = match fs::symlink_metadata(source) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        // Persist the new record before touching files. Keep the lock through
        // the operation so rollback cannot discard concurrent settings edits.
        // Both desktop entry points execute this on a blocking worker.
        let mut db = self.0.db.lock().unwrap();
        let mut candidate = db.clone();
        let result = update(&mut candidate)?;
        let database = self.root().join("database.json");
        storage::write_json(&database, &candidate)?;
        if exists && let Err(error) = recycle(source) {
            if let Err(rollback) = storage::write_json(&database, &*db) {
                return Err(error.context(format!(
                    "回收失败且无法恢复列表记录，原文件位于：{}；{rollback:#}",
                    source.display()
                )));
            }
            return Err(error);
        }
        *db = candidate;
        Ok(result)
    }
}

/// Keep ordinary unit tests out of the user's real recycle bin.
#[cfg(test)]
pub(crate) fn discard_test_fixture(source: &Path) -> Result<()> {
    fs::remove_dir_all(source)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn failed_recycle_restores_version_and_instance_records_without_deleting_files() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("猫咪", "pack").unwrap();
        let version = launcher
            .game_dir(&instance.id)
            .unwrap()
            .join("versions/pack");
        fs::create_dir_all(version.join("saves")).unwrap();
        fs::write(version.join("saves/keep"), b"world").unwrap();
        let before = serde_json::to_value(launcher.snapshot().database).unwrap();
        let fail = |_: &Path| -> Result<()> { bail!("system recycle bin unavailable") };
        assert!(
            launcher
                .delete_version_with_recycle(&instance.id, "pack", fail)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(launcher.snapshot().database).unwrap(),
            before
        );
        assert!(
            launcher
                .delete_instance_with_recycle(&instance.id, fail)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(launcher.snapshot().database).unwrap(),
            before
        );
        assert_eq!(fs::read(version.join("saves/keep")).unwrap(), b"world");
        let persisted: serde_json::Value =
            storage::read_json(&temp.path().join("database.json")).unwrap();
        assert_eq!(persisted, before);
        assert!(!temp.path().join("trash").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Uses the macOS Trash for a uniquely named temporary fixture"]
    fn native_macos_recycles_directory_and_preserves_symlink_target() {
        let temp = tempfile::tempdir().unwrap();
        let name = format!("nekocrate-回收测试-{}", uuid::Uuid::new_v4());
        let source = temp.path().join(&name);
        let target = temp.path().join("keep-outside");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("marker"), b"test-only").unwrap();
        fs::write(&target, b"outside").unwrap();
        std::os::unix::fs::symlink(&target, source.join("link")).unwrap();
        move_to_trash(&source).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"outside");
        let recycled = std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".Trash")
            .join(name);
        assert_eq!(fs::read(recycled.join("marker")).unwrap(), b"test-only");
        // Clean only the unique fixture created by this test, never other trash.
        fs::remove_dir_all(recycled).unwrap();
        assert_eq!(fs::read(target).unwrap(), b"outside");
    }
}
