//! Small, read-only suggestions for the add-instance picker. Never walk a tree.
use crate::{Launcher, folder_import};
use directories::BaseDirs;
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DirectorySource {
    Official,
    Portable,
}

#[derive(Debug, Serialize)]
pub struct SuggestedInstance {
    pub path: PathBuf,
    pub name: String,
    pub source: DirectorySource,
}

pub(crate) const OFFICIAL_INSTANCE_NAME: &str = "官方启动器";

fn official_location(
    platform: &str,
    home: Option<&Path>,
    app_data: Option<&Path>,
) -> Option<PathBuf> {
    match platform {
        "windows" => app_data.map(|path| path.join(".minecraft")),
        "macos" => home.map(|path| path.join("Library/Application Support/minecraft")),
        _ => home.map(|path| path.join(".minecraft")),
    }
}

pub(crate) fn official_directory() -> Option<PathBuf> {
    let base = BaseDirs::new()?;
    official_location(
        std::env::consts::OS,
        Some(base.home_dir()),
        Some(base.data_dir()),
    )
}

pub(crate) fn is_official_path(path: &Path, official: Option<&Path>) -> bool {
    official
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|official| path.canonicalize().is_ok_and(|path| path == official))
}

fn locations(
    platform: &str,
    home: Option<&Path>,
    app_data: Option<&Path>,
    executable: &Path,
) -> Vec<(PathBuf, DirectorySource)> {
    let mut paths = Vec::new();
    if let Some(path) = official_location(platform, home, app_data) {
        paths.push((path, DirectorySource::Official));
    }
    if platform == "macos" {
        // An installed app's executable lives inside .app/Contents/MacOS.
        // Portable game folders normally sit beside the .app bundle itself.
        if let Some(bundle) = executable.ancestors().find(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        }) && let Some(parent) = bundle.parent()
        {
            paths.push((parent.join(".minecraft"), DirectorySource::Portable));
        }
    }
    if let Some(parent) = executable.parent() {
        paths.push((parent.join(".minecraft"), DirectorySource::Portable));
    }
    paths
}

impl Launcher {
    pub fn suggested_instances(&self, executable: &Path) -> Vec<SuggestedInstance> {
        if self.is_portable() {
            return self.suggestions_at(vec![(
                self.root().parent().unwrap().join(".minecraft"),
                DirectorySource::Portable,
            )]);
        }
        let base = BaseDirs::new();
        self.suggestions_at(locations(
            std::env::consts::OS,
            base.as_ref().map(|dirs| dirs.home_dir()),
            base.as_ref().map(|dirs| dirs.data_dir()),
            executable,
        ))
    }

    fn suggestions_at(&self, paths: Vec<(PathBuf, DirectorySource)>) -> Vec<SuggestedInstance> {
        let mut seen = HashSet::new();
        paths
            .into_iter()
            .filter_map(|(path, source)| {
                // Also excludes already linked/overlapping folders and launcher data.
                // Missing/inaccessible optional locations are normal, not UI errors.
                let path = self.linkable_directory(&path, None).ok()?;
                if !seen.insert(path.clone()) {
                    return None;
                }
                Some(SuggestedInstance {
                    name: match source {
                        DirectorySource::Official => OFFICIAL_INSTANCE_NAME.into(),
                        DirectorySource::Portable => folder_import::default_name(&path),
                    },
                    path,
                    source,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_locations_use_roaming_data_and_real_bundle_parent() {
        let home = Path::new("/home/player");
        let roaming = Path::new("/roaming/player");
        let exe = Path::new("/portable/launcher");
        for (platform, expected) in [
            ("windows", roaming.join(".minecraft")),
            ("linux", home.join(".minecraft")),
            ("macos", home.join("Library/Application Support/minecraft")),
        ] {
            let paths = locations(platform, Some(home), Some(roaming), exe);
            assert_eq!(paths[0], (expected, DirectorySource::Official));
            assert_eq!(paths[1].0, Path::new("/portable/.minecraft"));
        }
        let paths = locations(
            "macos",
            None,
            None,
            Path::new("/Games/Neko.app/Contents/MacOS/launcher"),
        );
        assert_eq!(paths[0].0, Path::new("/Games/.minecraft"));
        assert_eq!(
            paths[1].0,
            Path::new("/Games/Neko.app/Contents/MacOS/.minecraft")
        );
    }

    #[tokio::test]
    async fn suggestions_are_read_only_exact_deduplicated_and_link_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let game = temp.path().join("Pack/.minecraft");
        fs::create_dir_all(game.join("saves/world")).unwrap();
        fs::write(game.join("saves/world/level.dat"), b"original").unwrap();
        let empty = temp.path().join("Empty/.minecraft");
        fs::create_dir_all(&empty).unwrap();
        let nested = temp.path().join("Elsewhere/Pack/.minecraft/versions");
        fs::create_dir_all(&nested).unwrap();
        let paths = vec![
            (game.clone(), DirectorySource::Official),
            (game.join("."), DirectorySource::Portable),
            (empty.clone(), DirectorySource::Portable),
            (
                temp.path().join("Elsewhere/.minecraft"),
                DirectorySource::Portable,
            ),
            (launcher.root().to_owned(), DirectorySource::Portable),
        ];
        {
            let _busy = launcher.0.operation.try_lock().unwrap();
            let found = launcher.suggestions_at(paths.clone());
            assert_eq!(found.len(), 2);
            assert_eq!(found[0].name, "官方启动器");
            assert_eq!(found[0].source, DirectorySource::Official);
            assert!(launcher.snapshot().database.instances.is_empty());
            assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
        }
        let linked = launcher
            .import_folders(vec![game.clone(), empty.clone()])
            .await
            .unwrap();
        assert!(linked.failures.is_empty());
        assert_eq!(linked.added.len(), 2);
        assert!(linked.added.iter().all(|instance| {
            instance
                .icon
                .is_some_and(|icon| icon < crate::model::INSTANCE_ICON_COUNT)
        }));
        assert_eq!(
            linked.added[0].game_dir(launcher.root()),
            game.canonicalize().unwrap()
        );
        assert!(linked.added[1].versions.is_empty());
        assert!(launcher.suggestions_at(paths).is_empty());
        // Choosing the containing pack again resolves before duplicate detection.
        let repeat = launcher
            .import_folders(vec![game.parent().unwrap().to_owned()])
            .await
            .unwrap();
        assert!(repeat.added.is_empty());
        assert_eq!(repeat.existing[0].id, linked.added[0].id);
        assert_eq!(repeat.existing[0].icon, linked.added[0].icon);
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
        assert!(
            !launcher
                .instance_dir(&linked.added[0].id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
        assert!(!temp.path().join("Elsewhere/.minecraft").exists());
    }

    #[cfg(unix)]
    #[test]
    fn aliases_and_overlapping_directories_are_not_offered_twice() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let game = temp.path().join("Pack/.minecraft");
        fs::create_dir_all(game.join("versions")).unwrap();
        let alias = temp.path().join("Alias/.minecraft");
        fs::create_dir_all(alias.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&game, &alias).unwrap();
        let paths = vec![
            (game.clone(), DirectorySource::Official),
            (alias, DirectorySource::Portable),
        ];
        assert_eq!(launcher.suggestions_at(paths.clone()).len(), 1);
        launcher
            .import_unlocked(game.clone(), "".into(), "".into())
            .unwrap();
        fs::create_dir_all(game.join("backups/.minecraft")).unwrap();
        let mut paths = paths;
        paths.push((game.join("backups/.minecraft"), DirectorySource::Portable));
        assert!(launcher.suggestions_at(paths).is_empty());
    }
}
