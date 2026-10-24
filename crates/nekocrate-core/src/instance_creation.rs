use crate::{Launcher, model::Instance, storage};
use anyhow::{Context, Result, bail};
use std::{fs, io::ErrorKind, path::Path};

pub(crate) fn validate(name: &str, version: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        bail!("实例名称应为 1–80 个字符");
    }
    if !version.is_empty() {
        storage::safe_version(version)?;
    }
    Ok(())
}

impl Launcher {
    /// An external location holds a fresh .minecraft folder. Existing directories
    /// belong to the import flow and must never be overwritten by creation.
    pub fn create_instance_at(
        &self,
        name: &str,
        version: &str,
        directory: Option<&Path>,
    ) -> Result<Instance> {
        let Some(directory) = directory else {
            return self.create_instance(name, version);
        };
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后创建实例")?;
        validate(name, version)?;
        if directory.as_os_str().is_empty() {
            bail!("请选择外部文件夹");
        }
        let expanded = storage::expand_user_path(directory)?;
        let selected_game_folder = expanded
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(".minecraft"));
        let parent = expanded
            .canonicalize()
            .context("无法访问外部文件夹，请重新选择")?;
        if !parent.is_dir() {
            bail!("请选择外部文件夹");
        }
        let game = if selected_game_folder
            || parent
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(".minecraft"))
        {
            parent
        } else {
            parent.join(".minecraft")
        };
        // The existing parent has been canonicalized; validate even before the
        // child exists so nested instances cannot create files in another game.
        self.check_external_directory(&game, None)?;
        if let Err(error) = fs::create_dir(&game) {
            if error.kind() == ErrorKind::AlreadyExists {
                bail!("该位置已有 .minecraft，请导入已有目录或选择其他位置");
            }
            return Err(error).context("无法创建游戏目录，请检查文件夹权限");
        }
        match self.create_unlocked(name, version, Some(game.clone())) {
            Ok(instance) => Ok(instance),
            Err(error) => {
                // Only undo the empty directory we just created. If another
                // process added files, leave them intact rather than recursing.
                let _ = fs::remove_dir(&game);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_creation_persists_original_path_and_unlink_preserves_game_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let parent = temp.path().join("Games/My 新实例");
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join("keep.txt"), b"existing").unwrap();
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher
            .create_instance_at("外部", "", Some(&parent))
            .unwrap();
        let game = parent.join(".minecraft").canonicalize().unwrap();
        assert_eq!(instance.game_directory.as_ref(), Some(&game));
        assert_eq!(instance.icon, Some(0));
        assert!(instance.versions.is_empty());
        assert!(
            !launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
        let managed = launcher.create_instance_at("内部", "", None).unwrap();
        assert!(managed.game_directory.is_none());
        assert!(managed.game_dir(launcher.root()).is_dir());
        drop(launcher);
        let launcher = Launcher::open(Some(root)).unwrap();
        assert_eq!(launcher.game_dir(&instance.id).unwrap(), game);
        fs::write(game.join("options.txt"), b"new game settings").unwrap();
        launcher.delete_instance(&instance.id).unwrap();
        assert_eq!(
            fs::read(game.join("options.txt")).unwrap(),
            b"new game settings"
        );
        assert_eq!(fs::read(parent.join("keep.txt")).unwrap(), b"existing");
    }

    #[test]
    fn rejects_collisions_invalid_input_and_overlapping_directories_without_writes() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let parent = temp.path().join("Games");
        fs::create_dir_all(&parent).unwrap();
        for (name, version) in [("", ""), ("valid", "../bad")] {
            assert!(
                launcher
                    .create_instance_at(name, version, Some(&parent))
                    .is_err()
            );
            assert!(!parent.join(".minecraft").exists());
        }
        assert!(
            launcher
                .create_instance_at("bad", "", Some(Path::new("")))
                .is_err()
        );
        assert!(
            launcher
                .create_instance_at("bad", "", Some(&parent.join("missing")))
                .is_err()
        );
        assert!(!parent.join("missing").exists());
        fs::create_dir(parent.join(".minecraft")).unwrap();
        fs::write(parent.join(".minecraft/keep"), b"original").unwrap();
        assert!(
            launcher
                .create_instance_at("bad", "", Some(&parent))
                .unwrap_err()
                .to_string()
                .contains("已有 .minecraft")
        );
        assert!(
            launcher
                .create_instance_at("bad", "", Some(&parent.join(".minecraft")))
                .is_err()
        );
        assert!(!parent.join(".minecraft/.minecraft").exists());
        assert_eq!(
            fs::read(parent.join(".minecraft/keep")).unwrap(),
            b"original"
        );
        assert!(launcher.snapshot().database.instances.is_empty());
        let linked = launcher
            .import_unlocked(parent.clone(), "existing".into(), "".into())
            .unwrap();
        let nested = linked.game_dir(launcher.root()).join("nested");
        fs::create_dir(&nested).unwrap();
        assert!(
            launcher
                .create_instance_at("nested", "", Some(&nested))
                .is_err()
        );
        assert!(!nested.join(".minecraft").exists());
        assert!(
            launcher
                .create_instance_at("internal", "", Some(launcher.root()))
                .is_err()
        );
        assert!(!launcher.root().join(".minecraft").exists());
        #[cfg(unix)]
        {
            let alias = temp.path().join("Alias");
            std::os::unix::fs::symlink(&nested, &alias).unwrap();
            assert!(
                launcher
                    .create_instance_at("alias", "", Some(&alias))
                    .is_err()
            );
            assert!(!nested.join(".minecraft").exists());
            let outside = temp.path().join("Other");
            fs::create_dir(&outside).unwrap();
            let named_alias = temp.path().join(".minecraft");
            std::os::unix::fs::symlink(&outside, &named_alias).unwrap();
            assert!(
                launcher
                    .create_instance_at("alias", "", Some(&named_alias))
                    .is_err()
            );
            assert!(fs::read_dir(outside).unwrap().next().is_none());
        }
    }

    #[test]
    fn failed_metadata_save_removes_only_the_new_empty_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let parent = temp.path().join("Games");
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join("keep"), b"original").unwrap();
        let database = launcher.root().join("database.json");
        if database.is_file() {
            fs::remove_file(&database).unwrap();
        }
        fs::create_dir_all(database.join("block-replacement")).unwrap();
        assert!(
            launcher
                .create_instance_at("external", "", Some(&parent))
                .is_err()
        );
        assert!(!parent.join(".minecraft").exists());
        assert_eq!(fs::read(parent.join("keep")).unwrap(), b"original");
        assert!(launcher.snapshot().database.instances.is_empty());
    }
}
