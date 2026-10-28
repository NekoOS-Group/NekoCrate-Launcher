//! Read-only discovery followed by explicit, revalidated in-place associations.
use crate::{
    Launcher,
    download::check_cancel,
    model::{Instance, InstanceVersion},
    storage,
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct FolderCandidate {
    pub path: String,
    pub name: String,
    pub versions: Vec<InstanceVersion>,
    pub existing_id: Option<String>,
    pub blocked: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct FolderIssue {
    pub path: String,
    pub message: String,
}
#[derive(Debug, Default, Serialize)]
pub struct FolderScan {
    pub candidates: Vec<FolderCandidate>,
    pub issues: Vec<FolderIssue>,
    pub skipped: usize,
}
impl FolderScan {
    fn issue(&mut self, path: &Path, message: impl ToString) {
        self.skipped += 1;
        // Bound error text from large inaccessible trees, not discovery results.
        if self.issues.len() < 100 {
            self.issues.push(FolderIssue {
                path: path.to_string_lossy().into(),
                message: message.to_string(),
            });
        }
    }
}
#[derive(Debug, Default, Serialize)]
pub struct FolderImportResult {
    pub added: Vec<Instance>,
    pub existing: Vec<Instance>,
    pub failures: Vec<FolderIssue>,
}
pub(crate) fn is_game_directory(path: &Path) -> bool {
    ["saves", "mods", "versions"]
        .iter()
        .any(|entry| path.join(entry).is_dir())
        || path.join("options.txt").is_file()
}
pub(crate) fn resolve_game_directory(path: &Path) -> Result<PathBuf> {
    let source = storage::expand_user_path(path)?;
    let source = source
        .canonicalize()
        .with_context(|| format!("无法访问导入目录：{}", source.display()))?;
    if !source.is_dir() {
        bail!("导入来源必须是一个目录：{}", source.display());
    }
    // Explicitly selected standard game folders may be empty. Discovery by
    // recursive drag-and-drop still requires game content to avoid empty backups.
    let standard_name = |path: &Path| {
        path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.eq_ignore_ascii_case(".minecraft") || name.eq_ignore_ascii_case("minecraft")
        })
    };
    let source = if is_game_directory(&source) {
        source
    } else {
        [".minecraft", "minecraft", "game"]
            .into_iter()
            .map(|name| source.join(name))
            .find(|path| path.is_dir())
            .unwrap_or(source)
    };
    if !is_game_directory(&source) && !standard_name(&source) {
        bail!("请选择含 saves、mods、versions 或 options.txt 的游戏目录");
    }
    Ok(source.canonicalize()?)
}
pub(crate) fn default_name(path: &Path) -> String {
    default_name_with_official(
        path,
        crate::instance_discovery::official_directory().as_deref(),
    )
}

fn default_name_with_official(path: &Path, official: Option<&Path>) -> String {
    // Canonical paths also recognize an official folder reached through a symlink
    // or the native folder picker, not just the automatic suggestion button.
    if crate::instance_discovery::is_official_path(path, official) {
        return crate::instance_discovery::OFFICIAL_INSTANCE_NAME.into();
    }
    let name = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy());
    let name: String = name
        .as_deref()
        .unwrap_or("Minecraft")
        .trim()
        .chars()
        .take(80)
        .collect();
    if name.is_empty() {
        "Minecraft".into()
    } else {
        name
    }
}

impl Launcher {
    /// Run on a blocking worker. Does not create directories, write metadata or
    /// touch game files. Explicitly dropped symlink roots resolve once; descendants
    /// do not follow symlinks/junctions, so discovery cannot escape through loops.
    pub fn scan_folders(
        &self,
        paths: &[PathBuf],
        cancel: &CancellationToken,
    ) -> Result<FolderScan> {
        let mut result = FolderScan::default();
        let mut roots = Vec::new();
        for path in paths {
            check_cancel(cancel)?;
            match storage::expand_user_path(path).and_then(|p| {
                p.canonicalize()
                    .with_context(|| format!("无法访问目录：{}", path.display()))
            }) {
                Ok(root) if root.is_dir() => roots.push(root),
                Ok(_) => result.issue(path, "请拖入文件夹"),
                Err(error) => result.issue(path, error),
            }
        }
        roots.sort_by(|a, b| {
            a.components()
                .count()
                .cmp(&b.components().count())
                .then(a.cmp(b))
        });
        let mut top_roots: Vec<PathBuf> = Vec::new();
        for root in roots {
            if !top_roots.iter().any(|parent| root.starts_with(parent)) {
                top_roots.push(root);
            }
        }
        let existing: Vec<_> = self
            .snapshot()
            .database
            .instances
            .into_iter()
            .map(|i| {
                let path = i.game_dir(self.root());
                (path.canonicalize().unwrap_or(path), i)
            })
            .collect();
        let mut seen = HashSet::new();
        for root in top_roots {
            if root.starts_with(self.root()) {
                result.issue(&root, "已跳过启动器数据目录");
                continue;
            }
            let mut entries = WalkDir::new(&root)
                .follow_links(false)
                .max_open(16)
                .into_iter();
            while let Some(entry) = entries.next() {
                check_cancel(cancel)?;
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        result.issue(error.path().unwrap_or(&root), &error);
                        continue;
                    }
                };
                let path = entry.path();
                if !entry.file_type().is_dir() {
                    continue;
                }
                if path.starts_with(self.root()) || !seen.insert(path.to_owned()) {
                    entries.skip_current_dir();
                    continue;
                }
                let candidate = (entry.depth() == 0
                    || entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(".minecraft"))
                    && is_game_directory(path);
                if !candidate {
                    continue;
                }
                entries.skip_current_dir();
                let same = existing.iter().find(|(directory, _)| directory == path);
                let blocked = if same.is_none() {
                    self.linkable_directory(path, None)
                        .err()
                        .map(|e| e.to_string())
                } else {
                    None
                };
                let versions = match self.discover_versions_cancellable(path, "", Some(cancel)) {
                    Ok(versions) => versions,
                    Err(error) => {
                        check_cancel(cancel)?;
                        result.issue(path, error);
                        continue;
                    }
                };
                result.candidates.push(FolderCandidate {
                    path: path.to_string_lossy().into(),
                    name: same
                        .map(|(_, i)| i.name.clone())
                        .unwrap_or_else(|| default_name(path)),
                    versions,
                    existing_id: same.map(|(_, i)| i.id.clone()),
                    blocked,
                });
            }
        }
        result.candidates.sort_by(|a, b| a.path.cmp(&b.path));
        check_cancel(cancel)?;
        Ok(result)
    }

    /// Serialize commits with other directory operations, while discovery remains
    /// independent from downloads. Recheck every path because preview may be stale.
    pub async fn import_folders(&self, paths: Vec<PathBuf>) -> Result<FolderImportResult> {
        let launcher = self.clone();
        tokio::task::spawn_blocking(move || -> Result<FolderImportResult> {
            let _guard = launcher
                .0
                .operation
                .try_lock()
                .context("有任务正在运行，请稍后关联实例")?;
            let mut result = FolderImportResult::default();
            let mut seen = HashSet::new();
            for path in paths {
                let attempt = (|| -> Result<()> {
                    let source = resolve_game_directory(&path)?;
                    if !seen.insert(source.clone()) {
                        return Ok(());
                    }
                    if let Some(existing) =
                        launcher
                            .snapshot()
                            .database
                            .instances
                            .into_iter()
                            .find(|i| {
                                let directory = i.game_dir(launcher.root());
                                directory.canonicalize().unwrap_or(directory) == source
                            })
                    {
                        result.existing.push(existing);
                        return Ok(());
                    }
                    let linked =
                        launcher.import_unlocked(source.clone(), String::new(), String::new())?;
                    result.added.push(linked);
                    Ok(())
                })();
                if let Err(error) = attempt {
                    result.failures.push(FolderIssue {
                        path: path.to_string_lossy().into(),
                        message: error.to_string(),
                    });
                }
            }
            Ok(result)
        })
        .await
        .context("关联实例任务失败")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn official_directory_name_is_shared_by_discovery_and_import() {
        let temp = tempfile::tempdir().unwrap();
        let official = temp.path().join("Library/Application Support/minecraft");
        let pack = temp.path().join("RLCraft/.minecraft");
        fs::create_dir_all(&official).unwrap();
        fs::create_dir_all(&pack).unwrap();
        assert_eq!(
            default_name_with_official(&official, Some(&official)),
            "官方启动器"
        );
        assert_eq!(
            default_name_with_official(&pack, Some(&official)),
            "RLCraft"
        );
        #[cfg(unix)]
        {
            let alias = temp.path().join("linked-game");
            std::os::unix::fs::symlink(&official, &alias).unwrap();
            assert_eq!(
                default_name_with_official(&alias, Some(&official)),
                "官方启动器"
            );
        }
    }

    fn game(path: &Path, versions: &[&str]) {
        fs::create_dir_all(path.join("saves/world")).unwrap();
        fs::write(path.join("saves/world/level.dat"), b"original save").unwrap();
        for id in versions {
            storage::write_json(
                &path.join("versions").join(id).join(format!("{id}.json")),
                &crate::test_http::version_fixture(id),
            )
            .unwrap();
        }
    }
    fn tree(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files: Vec<_> = WalkDir::new(path)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            // Windows enforces the launcher's exclusive byte-range lock even
            // against another file handle in this same process. Snapshot game
            // and persisted metadata files, not the live synchronization file.
            .filter(|entry| entry.file_name() != "launcher.lock")
            .map(|entry| {
                (
                    entry.path().strip_prefix(path).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    #[test]
    fn scan_finds_hidden_directories_names_parents_and_stops_at_game_roots() {
        let temp = tempfile::tempdir().unwrap();
        let games = temp.path().join("games");
        let first = games.join("RLCraft/.minecraft");
        let second = games.join("nested/逆转未来/.minecraft");
        game(&first, &["1.20.1", "1.21.1"]);
        game(&first.join("backups/old/.minecraft"), &["1.19"]);
        game(&second, &[]);
        fs::create_dir_all(games.join("empty/.minecraft")).unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let before = tree(temp.path());
        // Scanning does not occupy the mutation/download lock.
        let _guard = launcher.0.operation.try_lock().unwrap();
        let result = launcher
            .scan_folders(&[games, first], &CancellationToken::new())
            .unwrap();
        assert_eq!(result.candidates.len(), 2);
        assert_eq!(result.candidates[0].name, "RLCraft");
        assert_eq!(
            result.candidates[0]
                .versions
                .iter()
                .map(|v| v.id.as_str())
                .collect::<Vec<_>>(),
            ["1.20.1", "1.21.1"]
        );
        assert_eq!(result.candidates[1].name, "逆转未来");
        assert_eq!(result.skipped, 0);
        assert_eq!(
            tree(temp.path()),
            before,
            "discovery must be entirely read-only"
        );
        assert!(launcher.snapshot().database.instances.is_empty());
    }

    #[tokio::test]
    async fn batch_links_original_paths_deduplicates_and_unlink_keeps_game_files() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("pack-one/.minecraft");
        let second = temp.path().join("nested/pack-two/.minecraft");
        game(&first, &["1.21.1"]);
        game(&second, &["1.20.1"]);
        let original = tree(&first);
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let result = launcher
            .import_folders(vec![
                first.clone(),
                first.join("."),
                temp.path().join("missing"),
                second.clone(),
            ])
            .await
            .unwrap();
        assert_eq!(result.added.len(), 2);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.added[0].name, "pack-one");
        assert_eq!(result.added[1].name, "pack-two");
        assert_eq!(result.added[0].memory_mode, crate::model::MemoryMode::Auto);
        assert_eq!(
            result.added[0].game_dir(launcher.root()),
            first.canonicalize().unwrap()
        );
        let repeat = launcher.import_folders(vec![first.clone()]).await.unwrap();
        assert!(repeat.added.is_empty());
        assert_eq!(repeat.existing[0].id, result.added[0].id);
        let scan = launcher
            .scan_folders(std::slice::from_ref(&first), &CancellationToken::new())
            .unwrap();
        assert_eq!(
            scan.candidates[0].existing_id.as_deref(),
            Some(result.added[0].id.as_str())
        );
        assert!(scan.candidates[0].blocked.is_none());
        launcher
            .delete_instance_with_recycle(&result.added[0].id, crate::recycle::discard_test_fixture)
            .unwrap();
        assert_eq!(
            tree(&first),
            original,
            "unlinking only removes the association"
        );
        assert!(
            !launcher
                .root()
                .join("instances")
                .join(&result.added[1].id)
                .join(".minecraft")
                .exists()
        );
    }

    #[tokio::test]
    async fn commits_revalidate_and_do_not_wait_behind_downloads() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pack/.minecraft");
        game(&source, &[]);
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let scan = launcher
            .scan_folders(std::slice::from_ref(&source), &CancellationToken::new())
            .unwrap();
        assert_eq!(scan.candidates.len(), 1);
        {
            let _guard = launcher.0.operation.lock().await;
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    launcher.import_folders(vec![source.clone()])
                )
                .await
                .unwrap()
                .is_err()
            );
        }
        fs::remove_dir_all(&source).unwrap();
        let result = launcher.import_folders(vec![source]).await.unwrap();
        assert_eq!(result.failures.len(), 1);
        assert!(launcher.snapshot().database.instances.is_empty());
    }

    #[test]
    fn cancellation_invalid_files_and_launcher_data_are_handled_without_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            launcher
                .scan_folders(&[temp.path().to_owned()], &cancel)
                .is_err()
        );
        let file = temp.path().join("file");
        fs::write(&file, "not a directory").unwrap();
        let scan = launcher
            .scan_folders(
                &[file, launcher.root().to_owned()],
                &CancellationToken::new(),
            )
            .unwrap();
        assert!(scan.candidates.is_empty());
        assert_eq!(scan.skipped, 2);
        assert!(launcher.snapshot().database.instances.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_aliases_are_deduplicated_and_descendant_links_are_not_followed() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pack/.minecraft");
        game(&source, &[]);
        let alias = temp.path().join("alias");
        symlink(&source, &alias).unwrap();
        let search = temp.path().join("search");
        fs::create_dir(&search).unwrap();
        symlink(&search, search.join("loop")).unwrap();
        symlink(&source, search.join(".minecraft")).unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let scan = launcher
            .scan_folders(
                &[source.clone(), alias.clone(), search],
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].name, "pack");
        assert_eq!(
            launcher
                .import_folders(vec![source, alias])
                .await
                .unwrap()
                .added
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn manual_import_without_name_uses_resolved_game_parent() {
        let temp = tempfile::tempdir().unwrap();
        let pack = temp.path().join("RLCraft");
        game(&pack.join(".minecraft"), &[]);
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(pack, String::new(), String::new())
            .await
            .unwrap();
        assert_eq!(instance.name, "RLCraft");
    }
}
