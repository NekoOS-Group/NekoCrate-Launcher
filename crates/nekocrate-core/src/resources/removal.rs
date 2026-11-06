//! Recycle only listed resources, in their actual working directory.
use super::*;
use crate::version_management::ContentKind;
use std::collections::{BTreeSet, HashSet};

#[derive(Default, Serialize)]
pub struct ResourceRemoval {
    pub removed: Vec<ResourceCopySource>,
    pub failed: Vec<ResourceRemovalFailure>,
    pub confirmation_required: Vec<ResourceCopySource>,
}
#[derive(Serialize)]
pub struct ResourceRemovalFailure {
    pub source: ResourceCopySource,
    pub message: String,
}

impl ResourceKind {
    pub(super) fn content_kind(self) -> ContentKind {
        match self {
            Self::Mods => ContentKind::Mods,
            Self::Resourcepacks => ContentKind::Resourcepacks,
            Self::Shaderpacks => ContentKind::Shaderpacks,
            Self::Datapacks => ContentKind::Datapacks,
            Self::Saves => ContentKind::Saves,
        }
    }
}

impl Launcher {
    pub fn remove_game_resources(&self, sources: &[ResourceCopySource]) -> Result<ResourceRemoval> {
        self.remove_game_resources_with(sources, &mut crate::recycle::move_to_trash)
    }
    pub fn remove_game_resources_confirmed(
        &self,
        sources: &[ResourceCopySource],
        confirmed: bool,
    ) -> Result<ResourceRemoval> {
        self.remove_game_resources_checked(sources, confirmed, &mut crate::recycle::move_to_trash)
    }

    pub(super) fn remove_game_resources_with(
        &self,
        sources: &[ResourceCopySource],
        recycle: &mut dyn FnMut(&Path) -> Result<()>,
    ) -> Result<ResourceRemoval> {
        self.remove_game_resources_checked(sources, false, recycle)
    }
    fn remove_game_resources_checked(
        &self,
        sources: &[ResourceCopySource],
        confirmed: bool,
        recycle: &mut dyn FnMut(&Path) -> Result<()>,
    ) -> Result<ResourceRemoval> {
        anyhow::ensure!(
            !sources.is_empty() && sources.len() <= 256,
            "每次可删除 1–256 项资源"
        );
        let _operation = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let mut guards = Vec::new();
        let mut checked = HashSet::new();
        let mut entries = Vec::new();
        let mut origins = BTreeMap::new();
        let mut declared = BTreeMap::new();
        let mut confirmation_required = Vec::new();
        // Validate the whole request before touching any files. Keep directory
        // locks until completion, including aliases shared by different versions.
        for source in sources {
            let kind = source.kind.content_kind();
            let root = if source.global {
                self.resource_library_root()?
            } else {
                if checked.insert((&source.instance, &source.version)) {
                    guards.push(self.resource_directory_idle(&source.instance, &source.version)?);
                }
                self.content_location(&source.instance, &source.version)?.0
            };
            let directory = if source.global || kind == ContentKind::Datapacks {
                root.clone()
            } else {
                self.version_content_directory(&source.instance, &source.version, kind)?
            };
            let path = if source.global {
                self.cached_resource_path(kind, &source.file, None)?
            } else {
                let relative = directory.join(storage::safe_relative(&source.file)?);
                let relative = relative
                    .strip_prefix(&root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                // Unlike the read-only path resolver, deletion must not follow
                // a link to another resource in the same content directory.
                safe_join(&root, &relative)?;
                self.version_content_path(
                    &source.instance,
                    &source.version,
                    kind,
                    &source.file,
                    None,
                )?
            };
            let root = root.canonicalize()?;
            let path = path.canonicalize()?;
            if !confirmed && !source.global && source.kind == ResourceKind::Mods {
                let key = (source.instance.clone(), source.version.clone());
                if !declared.contains_key(&key) {
                    let content = self.version_content_index(&source.instance, &source.version)?;
                    declared.insert(
                        key.clone(),
                        content
                            .content
                            .files
                            .into_iter()
                            .filter(|file| file.declaration.is_some())
                            .map(|file| file.file_name)
                            .collect::<HashSet<_>>(),
                    );
                }
                if declared[&key].contains(&source.file) {
                    confirmation_required.push(source.clone());
                }
            }
            anyhow::ensure!(
                path != root && path.starts_with(&root),
                "资源路径不能离开所选目录"
            );
            if !origins.contains_key(&root) {
                safe_join(&root, ".nekocrate/resources.json.bak")?;
                origins.insert(root.clone(), records(&root)?);
            }
            entries.push((source.clone(), root, path));
        }
        // Ask once for the entire batch before moving even unrelated files.
        if !confirmation_required.is_empty() {
            return Ok(ResourceRemoval {
                confirmation_required,
                ..ResourceRemoval::default()
            });
        }
        let paths: BTreeSet<_> = entries.iter().map(|(_, _, path)| path.clone()).collect();
        let paths: Vec<_> = paths
            .iter()
            .filter(|path| {
                !paths
                    .iter()
                    .any(|parent| *path != parent && path.starts_with(parent))
            })
            .collect();
        let mut result = ResourceRemoval::default();
        let mut changed_roots = BTreeMap::new();
        for path in paths {
            let affected: Vec<_> = entries
                .iter()
                .filter(|(_, _, file)| file.starts_with(path))
                .collect();
            if let Err(error) = recycle(path) {
                for (source, _, _) in affected {
                    result.failed.push(ResourceRemovalFailure {
                        source: source.clone(),
                        message: format!("{error:#}"),
                    });
                }
                continue;
            }
            for (source, root, file) in affected {
                result.removed.push(source.clone());
                let relative = file
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let records = origins.get_mut(root).unwrap();
                let previous = records.len();
                let prefix = format!("{relative}/");
                records.retain(|name, _| name != &relative && !name.starts_with(&prefix));
                if records.len() != previous {
                    changed_roots.insert(root.clone(), source.clone());
                }
            }
        }
        // Batch provenance writes, rather than fsync once for every selected Mod.
        for (root, source) in changed_roots {
            let saved = safe_join(&root, ".nekocrate/resources.json")
                .and_then(|path| storage::write_json(&path, &origins[&root]));
            if let Err(error) = saved {
                let _ = crate::diagnostics::record_error(
                    self.root(),
                    "remove_resource_records",
                    &error,
                );
                result.failed.push(ResourceRemovalFailure {
                    source,
                    message: format!("文件已移入回收站，但来源记录更新失败：{error:#}"),
                });
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{archive, copy_target, fixture, install};
    use super::*;
    use tokio_util::sync::CancellationToken;

    fn source(id: &str, kind: ResourceKind, file: &str) -> ResourceCopySource {
        ResourceCopySource {
            global: false,
            kind,
            instance: id.into(),
            version: "1.20.1".into(),
            file: file.into(),
        }
    }
    fn put(root: &Path, file: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, file.as_bytes()).unwrap();
    }
    fn discard(path: &Path) -> Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[test]
    fn declared_disabled_mod_requires_confirmation_before_any_batch_mutation() {
        let (_temp, launcher, id, working) = fixture(true);
        put(&working, "mods/cat.jar.disabled");
        put(&working, "mods/personal.jar");
        storage::write_json(
            &working.join("modrinth.index.json"),
            &serde_json::json!({
                "formatVersion": 1, "game": "minecraft", "versionId": "1", "name": "Cats",
                "files": [{"path": "mods/cat.jar", "downloads": [], "hashes": {}}]
            }),
        )
        .unwrap();
        let declared = source(&id, ResourceKind::Mods, "cat.jar.disabled");
        let personal = source(&id, ResourceKind::Mods, "personal.jar");
        let batch = [personal.clone(), declared.clone()];
        let result = launcher
            .remove_game_resources_checked(&batch, false, &mut |_| panic!("confirmation required"))
            .unwrap();
        assert_eq!(result.confirmation_required, vec![declared]);
        assert!(result.removed.is_empty());
        assert!(working.join("mods/personal.jar").is_file());
        assert!(working.join("mods/cat.jar.disabled").is_file());
        let result = launcher
            .remove_game_resources_checked(&batch, true, &mut discard)
            .unwrap();
        assert_eq!(result.removed.len(), 2);
        assert!(result.confirmation_required.is_empty());
        assert!(!working.join("mods/cat.jar.disabled").exists());
        assert!(!working.join("mods/personal.jar").exists());
    }

    #[test]
    fn removal_covers_all_kinds_disabled_mods_nested_datapacks_and_cached_copies() {
        let (temp, launcher, id, working) = fixture(true);
        let entries = [
            (
                ResourceKind::Mods,
                "cat.jar.disabled",
                "mods/cat.jar.disabled",
            ),
            (
                ResourceKind::Resourcepacks,
                "cat.zip",
                "resourcepacks/cat.zip",
            ),
            (ResourceKind::Shaderpacks, "cat.zip", "shaderpacks/cat.zip"),
            (
                ResourceKind::Datapacks,
                "saves/World/datapacks/cat.zip",
                "saves/World/datapacks/cat.zip",
            ),
            (ResourceKind::Saves, "Backup", "saves/Backup"),
        ];
        for (_, _, path) in &entries[..4] {
            put(&working, path);
        }
        put(&working, "saves/World/level.dat");
        put(&working, "saves/Backup/level.dat");
        put(&working, "saves/Backup/region/r.0.0.mca");
        put(&working, "config/cat.toml");
        let library = launcher.resource_library_root().unwrap();
        put(&library, "mods/cat.jar.disabled");
        let mut sources: Vec<_> = entries
            .iter()
            .map(|(kind, file, _)| source(&id, *kind, file))
            .collect();
        sources.push(ResourceCopySource {
            global: true,
            ..source("", ResourceKind::Mods, "cat.jar.disabled")
        });
        let mut count = 0;
        let recycled = temp.path().join("recycled");
        fs::create_dir(&recycled).unwrap();
        let result = launcher
            .remove_game_resources_with(&sources, &mut |path| {
                count += 1;
                fs::rename(path, recycled.join(count.to_string()))?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.removed.len(), sources.len());
        assert!(sources.iter().all(|source| result.removed.contains(source)));
        assert!(result.failed.is_empty());
        assert_eq!(count, 6);
        for (_, _, path) in entries {
            assert!(!working.join(path).exists());
        }
        assert!(working.join("saves/World/level.dat").is_file());
        assert!(working.join("config/cat.toml").is_file());
        assert!(!library.join("mods/cat.jar.disabled").exists());
        assert!(
            fs::read_dir(recycled)
                .unwrap()
                .flatten()
                .any(|entry| entry.path().join("region/r.0.0.mca").is_file())
        );
    }

    #[test]
    fn invalid_batch_is_rejected_before_any_resource_is_recycled() {
        let (_temp, launcher, id, working) = fixture(true);
        put(&working, "mods/cat.jar");
        let valid = source(&id, ResourceKind::Mods, "cat.jar");
        for invalid in ["../config/cat.toml", "/tmp/cat.jar", "missing.jar"] {
            assert!(
                launcher
                    .remove_game_resources_with(
                        &[valid.clone(), source(&id, ResourceKind::Mods, invalid)],
                        &mut |_| panic!("invalid batch must not mutate files")
                    )
                    .is_err()
            );
        }
        let _operation = launcher.0.operation.try_lock().unwrap();
        assert!(
            launcher
                .remove_game_resources_with(&[valid], &mut |_| panic!("installation is active"))
                .is_err()
        );
        assert!(working.join("mods/cat.jar").is_file());
    }

    #[test]
    fn shared_directory_is_recycled_once_and_running_alias_blocks_removal_and_world_export() {
        let (_temp, launcher, id, working) = fixture(false);
        put(&working, "mods/cat.jar");
        put(&working, "saves/World/level.dat");
        let other = launcher.create_instance("Alias", "1.20.1").unwrap();
        launcher
            .mutate(|db| {
                db.instances
                    .iter_mut()
                    .find(|i| i.id == other.id)
                    .unwrap()
                    .game_directory = Some(working.clone());
                Ok(())
            })
            .unwrap();
        let a = source(&id, ResourceKind::Mods, "cat.jar");
        let b = source(&other.id, ResourceKind::Mods, "cat.jar");
        launcher
            .0
            .games
            .lock()
            .unwrap()
            .insert(other.id.clone(), CancellationToken::new());
        assert!(
            launcher
                .remove_game_resources_with(std::slice::from_ref(&a), &mut |_| panic!(
                    "shared game running"
                ))
                .is_err()
        );
        assert!(
            launcher
                .resource_export_path(&source(&id, ResourceKind::Saves, "World"))
                .is_err()
        );
        launcher.0.games.lock().unwrap().clear();
        let mut calls = 0;
        let result = launcher
            .remove_game_resources_with(&[a, b], &mut |path| {
                calls += 1;
                discard(path)
            })
            .unwrap();
        assert_eq!(result.removed.len(), 2);
        assert_eq!(calls, 1);
    }

    #[test]
    fn recycle_failure_preserves_that_file_and_reports_partial_success() {
        let (_temp, launcher, id, working) = fixture(true);
        put(&working, "mods/a.jar");
        put(&working, "mods/b.jar");
        let a = source(&id, ResourceKind::Mods, "a.jar");
        let b = source(&id, ResourceKind::Mods, "b.jar");
        let result = launcher
            .remove_game_resources_with(&[a.clone(), b.clone()], &mut |path| {
                if path.ends_with("a.jar") {
                    bail!("recycle unavailable");
                }
                discard(path)
            })
            .unwrap();
        assert_eq!(result.removed, vec![b]);
        assert_eq!(result.failed[0].source, a);
        assert!(result.failed[0].message.contains("recycle unavailable"));
        assert!(working.join("mods/a.jar").exists());
        assert!(!working.join("mods/b.jar").exists());
    }

    #[cfg(unix)]
    #[test]
    fn deletion_rejects_links_even_when_the_target_is_inside_the_same_directory() {
        let (_temp, launcher, id, working) = fixture(true);
        put(&working, "mods/real.jar");
        std::os::unix::fs::symlink(working.join("mods/real.jar"), working.join("mods/link.jar"))
            .unwrap();
        assert!(
            launcher
                .remove_game_resources_with(
                    &[source(&id, ResourceKind::Mods, "link.jar")],
                    &mut |_| panic!("must not follow symlink")
                )
                .is_err()
        );
        assert!(working.join("mods/real.jar").is_file());
    }

    #[tokio::test]
    async fn world_drag_copies_entire_save_and_refuses_to_overwrite_a_different_backup() {
        let (_temp, launcher, id, working) = fixture(true);
        let target = copy_target(&launcher, &id, &working, true);
        for file in [
            "level.dat",
            "region/r.0.0.mca",
            "playerdata/player.dat",
            "datapacks/data/pack.mcmeta",
        ] {
            put(&working, &format!("saves/My World/{file}"));
        }
        let world_source = source(&id, ResourceKind::Saves, "My World");
        assert_eq!(
            launcher.resource_export_path(&world_source).unwrap(),
            working.join("saves/My World").canonicalize().unwrap()
        );
        let cancel = CancellationToken::new();
        let plan = launcher
            .prepare_resource_copy(&world_source, &id, "target", &cancel)
            .unwrap()
            .unwrap();
        install(&launcher, &plan).await.unwrap();
        for file in [
            "level.dat",
            "region/r.0.0.mca",
            "playerdata/player.dat",
            "datapacks/data/pack.mcmeta",
        ] {
            assert_eq!(
                fs::read(working.join("saves/My World").join(file)).unwrap(),
                fs::read(target.join("saves/My World").join(file)).unwrap()
            );
        }
        assert!(
            launcher
                .prepare_resource_copy(&world_source, &id, "target", &cancel)
                .unwrap()
                .is_none()
        );
        fs::write(target.join("saves/My World/level.dat"), b"newer world").unwrap();
        assert!(
            launcher
                .prepare_resource_copy(&world_source, &id, "target", &cancel)
                .is_err()
        );
        assert_eq!(
            fs::read(target.join("saves/My World/level.dat")).unwrap(),
            b"newer world"
        );
        assert!(working.join("saves/My World/level.dat").exists());
        // An installed Mod's provenance is removed only with the actual file.
        archive(
            &working.join("mods/cat.jar"),
            &[("fabric.mod.json", r#"{"id":"cat","version":"1"}"#)],
        );
        let mod_source = source(&id, ResourceKind::Mods, "cat.jar");
        let plan = launcher
            .prepare_resource_copy(&mod_source, &id, "target", &cancel)
            .unwrap()
            .unwrap();
        install(&launcher, &plan).await.unwrap();
        assert!(records(&target).unwrap().contains_key("mods/cat.jar"));
        let result = launcher
            .remove_game_resources_with(
                &[ResourceCopySource {
                    version: "target".into(),
                    ..mod_source
                }],
                &mut discard,
            )
            .unwrap();
        assert!(result.failed.is_empty());
        assert!(!records(&target).unwrap().contains_key("mods/cat.jar"));
        assert!(working.join("mods/cat.jar").is_file());
    }
}
