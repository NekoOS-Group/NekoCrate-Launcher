//! Explicit directory choices share one resolver across launch and resource management.
use crate::{Launcher, launch_options::IsolationMode, version};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct LaunchDirectory {
    pub path: PathBuf,
    pub mode: IsolationMode,
    pub changed: bool,
    pub protected_pack: bool,
}
impl Launcher {
    /// Read-only preview: changing a choice never creates or moves game files.
    pub fn launch_directory(
        &self,
        id: &str,
        isolation: Option<IsolationMode>,
    ) -> Result<LaunchDirectory> {
        let mut instance = self.instance(id)?;
        let game = self.available_game_dir(id)?;
        let selected = instance.selected().map(|v| (v.id.clone(), v.modpack));
        let current = selected
            .as_ref()
            .map(|(id, _)| version::instance_working_directory(&game, &instance, id))
            .transpose()?
            .unwrap_or_else(|| game.clone());
        instance.launch.isolation = isolation;
        let path = selected
            .as_ref()
            .map(|(id, _)| version::instance_working_directory(&game, &instance, id))
            .transpose()?
            .unwrap_or_else(|| game.clone());
        Ok(LaunchDirectory {
            mode: isolation.unwrap_or(if path == game {
                IsolationMode::Shared
            } else {
                IsolationMode::Version
            }),
            changed: path != current,
            protected_pack: selected.is_some_and(|(_, pack)| pack),
            path,
        })
    }
    pub(crate) fn validate_isolation_change(
        &self,
        id: &str,
        isolation: Option<IsolationMode>,
    ) -> Result<()> {
        let instance = self.instance(id)?;
        if instance.launch.isolation == isolation {
            return Ok(());
        }
        let game = self.available_game_dir(id)?;
        let mut changed = instance.clone();
        changed.launch.isolation = isolation;
        for entry in &instance.versions {
            let current = version::instance_working_directory(&game, &instance, &entry.id)?;
            if current != version::instance_working_directory(&game, &changed, &entry.id)? {
                ensure!(
                    crate::resources::pending_plans(&current, id, &entry.id)?.is_empty(),
                    "此版本有未完成的资源安装，请完成或删除任务后再更改隔离方式"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        memory::MemoryConfig,
        model::{InstanceVersion, MemoryMode},
        resources::{ResourcePlan, ResourceTarget},
        storage,
    };
    use std::fs;
    fn save(launcher: &Launcher, id: &str, isolation: Option<IsolationMode>) -> Result<()> {
        let mut options = launcher.instance(id)?.launch;
        options.isolation = isolation;
        launcher.update_instance_options(
            id,
            "Renamed",
            MemoryConfig {
                mode: MemoryMode::Auto,
                custom_mb: 2048,
            },
            None,
            None,
            Some(options),
        )
    }
    #[test]
    fn old_layout_is_preserved_and_explicit_settings_align_all_directory_consumers() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Fixture", "1.20.1").unwrap();
        let id = &instance.id;
        let game = launcher.game_dir(id).unwrap();
        let folder = game.join("versions/1.20.1");
        fs::create_dir_all(folder.join("mods")).unwrap();
        fs::write(folder.join("mods/keep.jar"), b"unchanged").unwrap();
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/shared.jar"), b"shared").unwrap();
        storage::write_json(
            &folder.join("1.20.1.json"),
            &crate::test_http::version_fixture("1.20.1"),
        )
        .unwrap();
        assert_eq!(
            launcher.launch_directory(id, None).unwrap().mode,
            IsolationMode::Version
        );
        save(&launcher, id, None).unwrap();
        assert_eq!(launcher.instance(id).unwrap().launch.isolation, None);
        let preview = launcher
            .launch_directory(id, Some(IsolationMode::Shared))
            .unwrap();
        assert!(preview.changed);
        assert_eq!(preview.path, game);
        for (mode, expected) in [
            (IsolationMode::Shared, &game),
            (IsolationMode::Version, &folder),
        ] {
            save(&launcher, id, Some(mode)).unwrap();
            assert_eq!(
                &launcher.resource_target(id, "1.20.1").unwrap().directory,
                expected
            );
            assert_eq!(
                &launcher
                    .version_metadata(id, "1.20.1")
                    .unwrap()
                    .working_directory
                    .unwrap(),
                expected
            );
            assert_eq!(
                &launcher
                    .version_content(id, "1.20.1", crate::version_management::ContentKind::Mods)
                    .unwrap()
                    .directory,
                &expected.join("mods")
            );
        }
        assert_eq!(
            fs::read(folder.join("mods/keep.jar")).unwrap(),
            b"unchanged"
        );
        assert_eq!(fs::read(game.join("mods/shared.jar")).unwrap(), b"shared");
        let mut isolated = launcher.instance(id).unwrap();
        isolated.versions.push(InstanceVersion::pending("other"));
        assert_ne!(
            version::instance_working_directory(&game, &isolated, "other").unwrap(),
            folder
        );
    }
    #[test]
    fn isolation_can_be_configured_before_a_versions_metadata_is_downloaded() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pending", "1.21.1").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        save(&launcher, &instance.id, Some(IsolationMode::Version)).unwrap();
        assert_eq!(
            launcher
                .launch_directory(&instance.id, Some(IsolationMode::Version))
                .unwrap()
                .path,
            game.join("versions/1.21.1")
        );
        assert!(!game.join("versions/1.21.1/mods").exists());
        save(&launcher, &instance.id, Some(IsolationMode::Shared)).unwrap();
    }
    #[test]
    fn managed_modpacks_keep_their_directory_even_under_shared_policy() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pack", "pending").unwrap();
        launcher
            .mutate(|db| {
                db.instances[0].versions[0].modpack = true;
                Ok(())
            })
            .unwrap();
        save(&launcher, &instance.id, Some(IsolationMode::Shared)).unwrap();
        let preview = launcher
            .launch_directory(&instance.id, Some(IsolationMode::Shared))
            .unwrap();
        assert!(preview.protected_pack);
        assert_eq!(
            preview.path,
            launcher
                .game_dir(&instance.id)
                .unwrap()
                .join("versions/pending")
        );
    }
    #[test]
    fn pending_resource_installation_blocks_only_a_changed_target() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Fixture", "1.20.1").unwrap();
        let id = &instance.id;
        let game = launcher.game_dir(id).unwrap();
        storage::write_json(
            &version::metadata_path(&game, "1.20.1").unwrap(),
            &crate::test_http::version_fixture("1.20.1"),
        )
        .unwrap();
        let task = game.join(".nekocrate/resource-tasks/fixture/plan.json");
        let mut plan = ResourcePlan {
            id: "fixture".into(),
            target: ResourceTarget {
                instance: id.clone(),
                version: "1.20.1".into(),
                directory: game,
                minecraft: "1.20.1".into(),
                loader: None,
                shared: true,
                worlds: vec![],
            },
            entries: vec![],
            conflicts: vec![],
            warnings: vec![],
            created_at: 0,
            complete: false,
            started: true,
        };
        storage::write_json(&task, &plan).unwrap();
        assert!(
            save(&launcher, id, Some(IsolationMode::Version))
                .unwrap_err()
                .to_string()
                .contains("未完成")
        );
        save(&launcher, id, Some(IsolationMode::Shared)).unwrap();
        let other = launcher.create_instance("Other", "1.20.1").unwrap();
        save(&launcher, &other.id, Some(IsolationMode::Version)).unwrap();
        plan.complete = true;
        storage::write_json(&task, &plan).unwrap();
        save(&launcher, id, Some(IsolationMode::Version)).unwrap();
    }
}
