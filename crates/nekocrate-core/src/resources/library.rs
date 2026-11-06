//! Explicitly collected resources survive independently of any game directory.
use super::*;
use crate::version_management::{self as content, ContentKind, VersionContent};
use tokio_util::sync::CancellationToken;

struct Staging(PathBuf);
impl Staging {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn kind_name(kind: ContentKind) -> Result<&'static str> {
    Ok(match kind {
        ContentKind::Mods => "mods",
        ContentKind::Resourcepacks => "resourcepacks",
        ContentKind::Shaderpacks => "shaderpacks",
        ContentKind::Datapacks => "datapacks",
        _ => bail!("全局资源库支持 Mods、资源包、光影和数据包"),
    })
}

impl Launcher {
    pub(crate) fn resource_library_root(&self) -> Result<PathBuf> {
        crate::resource_manager::owned(self.root(), Path::new("resource-library"))
    }

    pub fn cached_resources(&self, kind: ContentKind) -> Result<VersionContent> {
        kind_name(kind)?;
        let root = self.resource_library_root()?;
        let mut listing = content::read_content(&root, None, kind, false)?;
        for file in &mut listing.files {
            // Check every listed path, including intermediate directories.
            let path = listing.directory.join(&file.file_name);
            crate::resource_manager::owned(&root, path.strip_prefix(&root)?)?;
            if kind == ContentKind::Mods {
                content::apply_identity(file, self.0.mod_summaries.read(&path));
            }
        }
        crate::sorting::sort_by_cached_key(&mut listing.files, |f| {
            (f.name.to_lowercase(), f.file_name.to_lowercase())
        });
        Ok(listing)
    }

    pub fn cached_resource_path(
        &self,
        kind: ContentKind,
        file: &str,
        source: Option<&str>,
    ) -> Result<PathBuf> {
        kind_name(kind)?;
        let relative = storage::safe_relative(file)?;
        let root = self.resource_library_root()?;
        let listing = content::read_content(&root, None, kind, false)?;
        anyhow::ensure!(
            listing.files.iter().any(|f| f.file_name == file),
            "此文件不在全局资源库中"
        );
        let path = listing.directory.join(relative);
        let path = crate::resource_manager::owned(&root, path.strip_prefix(&root)?)?;
        if let Some(source) = source {
            content::details::check_source(kind, source)?;
            anyhow::ensure!(path.is_dir(), "元数据文件位于压缩包内");
            return crate::resource_manager::owned(&path, &storage::safe_relative(source)?);
        }
        Ok(path)
    }

    pub fn cached_resource_metadata(
        &self,
        kind: ContentKind,
        file: &str,
    ) -> Result<content::details::ContentMetadata> {
        content::details::content_metadata(
            self.cached_resource_path(kind, file, None)?,
            &self.resource_library_root()?,
            kind,
        )
    }
    pub fn cached_resource_source(
        &self,
        kind: ContentKind,
        file: &str,
        source: &str,
    ) -> Result<String> {
        content::details::content_source(self.cached_resource_path(kind, file, None)?, kind, source)
    }
    pub fn cached_resource_presentation(
        &self,
        kind: ContentKind,
        file: &str,
    ) -> Result<content::presentation::ContentPresentation> {
        content::presentation::content_presentation(
            &self.cached_resource_path(kind, file, None)?,
            &self.resource_library_root()?,
            kind,
        )
    }

    pub fn remove_cached_resource(&self, kind: ContentKind, file: &str) -> Result<()> {
        self.remove_cached_resource_with(kind, file, crate::recycle::move_to_trash)
    }
    fn remove_cached_resource_with(
        &self,
        kind: ContentKind,
        file: &str,
        recycle: fn(&Path) -> Result<()>,
    ) -> Result<()> {
        let kind = match kind {
            ContentKind::Mods => ResourceKind::Mods,
            ContentKind::Resourcepacks => ResourceKind::Resourcepacks,
            ContentKind::Shaderpacks => ResourceKind::Shaderpacks,
            ContentKind::Datapacks => ResourceKind::Datapacks,
            _ => bail!("全局资源库支持 Mods、资源包、光影和数据包"),
        };
        let result = self.remove_game_resources_with(
            &[ResourceCopySource {
                global: true,
                kind,
                instance: String::new(),
                version: String::new(),
                file: file.into(),
            }],
            &mut |path| recycle(path),
        )?;
        if let Some(failure) = result.failed.first() {
            bail!("{}", failure.message);
        }
        Ok(())
    }

    pub async fn cache_game_resources(
        &self,
        inputs: Vec<LocalImport>,
        sources: Vec<ResourceCopySource>,
        cancel: &CancellationToken,
    ) -> Result<usize> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let launcher = self.clone();
        let token = cancel.clone();
        let files = tokio::task::spawn_blocking(move || {
            launcher.collect_resources(inputs, sources, &token)
        })
        .await??;
        // Feed the existing local-file index: future downloads can reuse these
        // bytes directly, without a second object-cache copy.
        self.index_collected(&files).await;
        Ok(files.len())
    }

    async fn index_collected(&self, files: &[PathBuf]) {
        let downloader = match crate::download::Downloader::new(self.root(), self.settings()) {
            Ok(downloader) => downloader,
            Err(error) => {
                let _ =
                    crate::diagnostics::record_error(self.root(), "index_cached_resource", &error);
                return;
            }
        };
        for file in files {
            if file.is_file()
                && let Err(error) = downloader.index_local(file).await
            {
                let _ =
                    crate::diagnostics::record_error(self.root(), "index_cached_resource", &error);
            }
        }
    }

    /// Collect a single explicitly chosen artifact. Dependencies are recorded, not
    /// silently installed: this library has no game or loader of its own.
    pub async fn download_cached_resource(
        &self,
        scope: &ResourceCatalog,
        project: &str,
        version: &str,
        cancel: &CancellationToken,
        sink: &crate::model::EventSink,
    ) -> Result<usize> {
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        anyhow::ensure!(
            scope.kind != ResourceKind::Saves,
            "全局资源库支持 Mods、资源包、光影和数据包"
        );
        let target = scope.target()?;
        let api = provider::Provider::new(self, scope.platform)?;
        let selected = api.version(Some(project), version, cancel).await?;
        anyhow::ensure!(selected.project == project, "项目与资源版本不一致");
        anyhow::ensure!(
            provider::compatible(&selected, &target, scope.kind),
            "此资源没有兼容版本"
        );
        let detail = api.project(project, cancel).await?;
        let file = provider::primary_file(&selected)?;
        // A private per-artifact download cache avoids deleting an interrupted
        // game's shared objects when the library copy is committed.
        let key = hex::encode(ring::digest::digest(
            &ring::digest::SHA256,
            &serde_json::to_vec(&(scope.platform, project, version))?,
        ));
        let root = self.resource_library_root()?;
        let staging = safe_join(&root, &format!(".imports/downloads/{key}"))?;
        fs::create_dir_all(&staging)?;
        let settings = self.settings();
        let mut downloader = crate::download::Downloader::new(self.root(), settings.clone())?
            .with_cache(safe_join(&staging, "objects")?);
        if scope.platform == Platform::Curseforge {
            downloader = downloader.with_curseforge_key(
                self.root(),
                &settings,
                crate::curseforge::key()?,
            )?;
        }
        let spec = crate::download::FileSpec {
            url: file.url.clone(),
            path: safe_join(&staging, &format!("files/{}", file.name))?,
            hash: file.sha1.clone(),
            sha256: false,
            size: file.size,
        };
        downloader.batch(vec![spec.clone()], cancel, sink).await?;
        let launcher = self.clone();
        let token = cancel.clone();
        let input = LocalImport {
            path: spec.path.clone(),
            kind: Some(scope.kind),
        };
        let hash = file.sha1.clone();
        let origin = ResourceOrigin {
            platform: scope.platform,
            project: Some(project.to_owned()),
            version: Some(version.to_owned()),
            name: detail.name,
            dependencies: selected.dependencies,
        };
        let kind = scope.kind;
        let files = tokio::task::spawn_blocking(move || {
            let sha256 = imports::fingerprint(&input.path, &token)?.0;
            launcher.collect_entries(
                vec![(
                    input,
                    Some(ResourceRecord {
                        kind,
                        origin,
                        sha256,
                        sha1: Some(hash),
                        installed_at: storage::now(),
                    }),
                    true,
                )],
                &token,
            )
        })
        .await??;
        self.index_collected(&files).await;
        // Failed or cancelled transfers retain their bytes; only successful
        // publication removes the download copy.
        if let Err(error) = downloader.discard_installer(&spec).await {
            let _ = crate::diagnostics::record_error(
                self.root(),
                "clean_cached_resource_staging",
                &error,
            );
        } else {
            let _ = fs::remove_dir_all(&staging);
        }
        Ok(files.len())
    }

    fn collect_resources(
        &self,
        inputs: Vec<LocalImport>,
        sources: Vec<ResourceCopySource>,
        cancel: &CancellationToken,
    ) -> Result<Vec<PathBuf>> {
        anyhow::ensure!(
            (1..=256).contains(&(inputs.len() + sources.len())),
            "每次可导入 1–256 个文件或目录"
        );
        let root = self.resource_library_root()?;
        let mut inputs: Vec<_> = inputs
            .into_iter()
            .map(|input| (input, None, false))
            .collect();
        for source in sources {
            let path = self.resource_export_path(&source)?.canonicalize()?;
            let origin = if source.global {
                root.clone()
            } else {
                self.resource_target(&source.instance, &source.version)?
                    .directory
            };
            let provenance = path
                .strip_prefix(origin.canonicalize()?)
                .ok()
                .and_then(|relative| {
                    records(&origin)
                        .ok()?
                        .remove(&relative.to_string_lossy().replace('\\', "/"))
                });
            inputs.push((
                LocalImport {
                    path,
                    kind: Some(source.kind),
                },
                provenance,
                true,
            ));
        }
        self.collect_entries(inputs, cancel)
    }

    fn collect_entries(
        &self,
        inputs: Vec<(LocalImport, Option<ResourceRecord>, bool)>,
        cancel: &CancellationToken,
    ) -> Result<Vec<PathBuf>> {
        let root = self.resource_library_root()?;
        fs::create_dir_all(&root)?;
        let staging_root = safe_join(&root, ".imports")?;
        fs::create_dir_all(&staging_root)?;
        let staging = Staging(staging_root.join(uuid::Uuid::new_v4().to_string()));
        fs::create_dir(staging.path())?;
        let mut pending = BTreeMap::<String, (PathBuf, String, bool)>::new();
        let mut provenance = records(&root)?;
        for (index, (input, record, listed)) in inputs.into_iter().enumerate() {
            crate::download::check_cancel(cancel)?;
            let mut detected = imports::inspect(&input.path)?;
            // Existing legacy Mods may be valid JARs without a loader descriptor.
            if listed && input.kind == Some(ResourceKind::Mods) && detected.format == "unknown" {
                crate::archive_support::open(&input.path)?;
                detected.kinds.push(ResourceKind::Mods);
            }
            let kind = input
                .kind
                .or_else(|| (detected.kinds.len() == 1).then(|| detected.kinds[0]))
                .context("无法判断资源类型，请选择资源包或数据包")?;
            anyhow::ensure!(
                kind != ResourceKind::Saves,
                "全局资源库支持 Mods、资源包、光影和数据包"
            );
            anyhow::ensure!(
                detected.kinds.contains(&kind),
                "文件内容与所选资源类型不符：{}",
                detected.name
            );
            let name = if detected.prefix.is_some() {
                input
                    .path
                    .file_stem()
                    .context("文件名为空")?
                    .to_string_lossy()
                    .into_owned()
            } else {
                detected.name.clone()
            };
            component(&name)?;
            let destination = format!("{}/{name}", kind.directory());
            let output = safe_join(&root, &destination)?;
            let (digest, size) = imports::fingerprint(&input.path, cancel)?;
            let entry = PlanEntry {
                kind,
                destination: destination.clone(),
                size,
                origin: record
                    .as_ref()
                    .map(|r| r.origin.clone())
                    .unwrap_or(ResourceOrigin {
                        platform: Platform::Local,
                        project: None,
                        version: None,
                        name: detected.name,
                        dependencies: vec![],
                    }),
                input: Input::Local {
                    path: input.path,
                    digest: digest.clone(),
                    archive_prefix: detected.prefix,
                },
                reuse: false,
                content_sha256: None,
            };
            let staged = staging.path().join(index.to_string());
            imports::stage(&entry, &staged, cancel, &|_, _| {})?;
            let actual = imports::fingerprint(&staged, cancel)?.0;
            if let Input::Local {
                archive_prefix: None,
                ..
            } = &entry.input
            {
                anyhow::ensure!(actual == digest, "资源在导入时发生变化，请重新导入");
            }
            if let Some((_, previous, _)) = pending.get(&destination) {
                anyhow::ensure!(
                    *previous == actual,
                    "全局资源库存在内容不同的同名文件：{name}"
                );
                continue;
            }
            let reuse = output.exists();
            if reuse {
                anyhow::ensure!(
                    imports::fingerprint(&output, cancel)?.0 == actual,
                    "全局资源库存在内容不同的同名文件：{name}"
                );
            }
            // Same-name conflicts never overwrite user files; all inputs are
            // validated before publishing the batch.
            pending.insert(destination.clone(), (staged, actual.clone(), reuse));
            if reuse
                && record.is_none()
                && provenance
                    .get(&destination)
                    .is_some_and(|r| r.sha256 == actual)
            {
                continue;
            }
            provenance.insert(
                destination,
                ResourceRecord {
                    kind,
                    origin: entry.origin,
                    sha256: actual,
                    sha1: record.filter(|r| r.sha256 == digest).and_then(|r| r.sha1),
                    installed_at: storage::now(),
                },
            );
        }
        let mut created = Vec::new();
        let result = (|| -> Result<()> {
            for (relative, (staged, hash, reuse)) in &pending {
                crate::download::check_cancel(cancel)?;
                let output = safe_join(&root, relative)?;
                if *reuse {
                    continue;
                }
                fs::create_dir_all(output.parent().context("无效资源目录")?)?;
                anyhow::ensure!(!output.exists(), "目标在导入期间发生变化");
                if staged.is_dir() {
                    fs::rename(staged, &output)?;
                } else if let Err(error) = fs::hard_link(staged, &output) {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        return Err(error.into());
                    }
                    install::copy_new(staged, &output, hash, cancel)?;
                }
                created.push((output, hash.clone()));
            }
            storage::write_json(&safe_join(&root, ".nekocrate/resources.json")?, &provenance)
        })();
        if let Err(error) = result {
            for (path, hash) in created.iter().rev() {
                if imports::fingerprint(path, &CancellationToken::new())
                    .is_ok_and(|(actual, _)| actual == *hash)
                {
                    let _ = if path.is_dir() {
                        fs::remove_dir_all(path)
                    } else {
                        fs::remove_file(path)
                    };
                }
            }
            return Err(error);
        }
        pending
            .keys()
            .map(|relative| safe_join(&root, relative))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        archive, cache, cache_project, copy_target, fixture, install, local, remote, sink,
    };
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn collected_resources_survive_source_removal_and_copy_to_another_version() {
        let (_temp, launcher, id, working) = fixture(true);
        let target = copy_target(&launcher, &id, &working, true);
        let path = working.join("mods/cat.jar.disabled");
        archive(
            &path,
            &[(
                "fabric.mod.json",
                r#"{"id":"cat","name":"Cat","version":"1","depends":{"minecraft":"1.20.1"}}"#,
            )],
        );
        let source = ResourceCopySource {
            global: false,
            kind: ResourceKind::Mods,
            instance: id.clone(),
            version: "1.20.1".into(),
            file: "cat.jar.disabled".into(),
        };
        let cancel = CancellationToken::new();
        assert_eq!(
            launcher
                .cache_game_resources(vec![], vec![source], &cancel)
                .await
                .unwrap(),
            1
        );
        assert!(path.is_file());
        fs::remove_file(path).unwrap();
        let cached = launcher.cached_resources(ContentKind::Mods).unwrap();
        assert_eq!(cached.files.len(), 1);
        assert_eq!(cached.files[0].name, "Cat");
        assert!(!cached.files[0].enabled);
        assert!(
            launcher
                .cached_resource_source(ContentKind::Mods, "cat.jar.disabled", "fabric.mod.json")
                .unwrap()
                .contains("Cat")
        );
        let source = ResourceCopySource {
            global: true,
            kind: ResourceKind::Mods,
            instance: String::new(),
            version: String::new(),
            file: "cat.jar.disabled".into(),
        };
        let plan = launcher
            .prepare_resource_copies(&[source], &id, "target", &cancel)
            .unwrap()
            .unwrap();
        install(&launcher, &plan).await.unwrap();
        let installed = target.join("mods/cat.jar.disabled");
        assert!(installed.is_file());
        launcher
            .remove_cached_resource_with(ContentKind::Mods, "cat.jar.disabled", |path| {
                Ok(fs::remove_file(path)?)
            })
            .unwrap();
        assert!(installed.is_file());
        assert!(
            launcher
                .cached_resources(ContentKind::Mods)
                .unwrap()
                .files
                .is_empty()
        );
    }

    #[tokio::test]
    async fn imports_are_idempotent_conflicts_are_atomic_and_types_are_not_redirected() {
        let (temp, launcher, _, _) = fixture(true);
        let one = temp.path().join("one/cat.jar");
        archive(
            &one,
            &[("fabric.mod.json", r#"{"id":"cat","version":"1"}"#)],
        );
        let cancel = CancellationToken::new();
        launcher
            .cache_game_resources(vec![local(&one)], vec![], &cancel)
            .await
            .unwrap();
        launcher
            .cache_game_resources(vec![local(&one)], vec![], &cancel)
            .await
            .unwrap();
        let conflict = temp.path().join("two/cat.jar");
        archive(
            &conflict,
            &[("fabric.mod.json", r#"{"id":"cat","version":"2"}"#)],
        );
        let another = temp.path().join("two/new.jar");
        archive(
            &another,
            &[("fabric.mod.json", r#"{"id":"new","version":"1"}"#)],
        );
        let error = launcher
            .cache_game_resources(vec![local(&another), local(&conflict)], vec![], &cancel)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("同名"));
        assert_eq!(
            launcher
                .cached_resources(ContentKind::Mods)
                .unwrap()
                .files
                .len(),
            1
        );
        let shader = temp.path().join("shaders.zip");
        archive(&shader, &[("shaders/main.fsh", "void main() {}")]);
        assert!(
            launcher
                .cache_game_resources(
                    vec![LocalImport {
                        path: shader.clone(),
                        kind: Some(ResourceKind::Mods)
                    }],
                    vec![],
                    &cancel
                )
                .await
                .is_err()
        );
        assert!(
            launcher
                .cached_resources(ContentKind::Shaderpacks)
                .unwrap()
                .files
                .is_empty()
        );
        launcher
            .cache_game_resources(vec![local(&shader)], vec![], &cancel)
            .await
            .unwrap();
        let resource = temp.path().join("resource.zip");
        archive(
            &resource,
            &[
                (
                    "pack.mcmeta",
                    r#"{"pack":{"pack_format":15,"description":"Cat"}}"#,
                ),
                ("assets/cat/lang/en_us.json", "{}"),
            ],
        );
        let data = temp.path().join("data.zip");
        archive(
            &data,
            &[
                (
                    "pack.mcmeta",
                    r#"{"pack":{"pack_format":15,"description":"Data"}}"#,
                ),
                ("data/cat/functions/cat.mcfunction", "say cat"),
            ],
        );
        launcher
            .cache_game_resources(vec![local(&resource), local(&data)], vec![], &cancel)
            .await
            .unwrap();
        assert_eq!(
            launcher
                .cached_resources(ContentKind::Resourcepacks)
                .unwrap()
                .files
                .len(),
            1
        );
        let listing = launcher.cached_resources(ContentKind::Datapacks).unwrap();
        assert_eq!(listing.files.len(), 1);
        assert!(
            launcher
                .cached_resource_path(ContentKind::Datapacks, &listing.files[0].file_name, None)
                .unwrap()
                .is_file()
        );
        let cache = launcher.resource_cache().unwrap();
        let parts = &cache
            .iter()
            .find(|entry| matches!(entry.kind, crate::resource_manager::CacheKind::Resources))
            .unwrap()
            .parts;
        for kind in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
            assert!(parts.iter().any(|part| part.kind == kind && part.size > 0));
        }
        assert!(
            launcher
                .cached_resource_path(ContentKind::Mods, "../database.json", None)
                .is_err()
        );
        cancel.cancel();
        assert!(
            launcher
                .cache_game_resources(vec![local(&another)], vec![], &cancel)
                .await
                .is_err()
        );
        assert_eq!(
            launcher
                .cached_resources(ContentKind::Mods)
                .unwrap()
                .files
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn online_collection_reuses_local_bytes_records_origin_and_cleans_completed_staging() {
        let (temp, launcher, id, working) = fixture(true);
        let original = temp.path().join("original/cat.jar");
        archive(
            &original,
            &[("fabric.mod.json", r#"{"id":"cat","version":"1"}"#)],
        );
        let bytes = fs::read(&original).unwrap();
        let hash = hex::encode(ring::digest::digest(
            &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
            &bytes,
        ));
        let downloader =
            crate::download::Downloader::new(launcher.root(), launcher.settings()).unwrap();
        downloader.index_local(&original).await.unwrap();
        let mut metadata = remote("one", "cat", json!([]));
        metadata["files"][0]["size"] = json!(bytes.len());
        metadata["files"][0]["hashes"]["sha1"] = json!(hash);
        metadata["files"][0]["url"] = json!("https://unreachable.invalid/cat.jar");
        cache(&launcher, "version/one", &[], metadata);
        cache_project(&launcher, "cat");
        let scope = ResourceCatalog {
            minecraft: "1.20.1".into(),
            loader: Some(ModLoader {
                kind: "fabric".into(),
                version: "0.16.10".into(),
            }),
            kind: ResourceKind::Mods,
            platform: Platform::Modrinth,
        };
        let cancel = CancellationToken::new();
        launcher
            .download_cached_resource(&scope, "cat", "one", &cancel, &sink())
            .await
            .unwrap();
        let cached = launcher
            .cached_resource_path(ContentKind::Mods, "cat.jar", None)
            .unwrap();
        assert_eq!(fs::read(&cached).unwrap(), bytes);
        let root = launcher.resource_library_root().unwrap();
        assert_eq!(
            records(&root).unwrap()["mods/cat.jar"]
                .origin
                .project
                .as_deref(),
            Some("cat")
        );
        assert!(
            !launcher
                .resource_cache()
                .unwrap()
                .iter()
                .any(|entry| matches!(entry.kind, crate::resource_manager::CacheKind::Partials))
        );
        launcher
            .cache_game_resources(vec![local(&original)], vec![], &cancel)
            .await
            .unwrap();
        assert_eq!(
            records(&root).unwrap()["mods/cat.jar"]
                .origin
                .project
                .as_deref(),
            Some("cat")
        );
        copy_target(&launcher, &id, &working, true);
        let plan = launcher
            .prepare_resource_copies(
                &[ResourceCopySource {
                    global: true,
                    kind: ResourceKind::Mods,
                    instance: String::new(),
                    version: String::new(),
                    file: "cat.jar".into(),
                }],
                &id,
                "target",
                &cancel,
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.entries[0].origin.project.as_deref(), Some("cat"));
        install(&launcher, &plan).await.unwrap();
        // Remove the original. The library alone can satisfy another download,
        // even with an unreachable URL and without an object-cache copy.
        fs::remove_file(original).unwrap();
        let spec = crate::download::FileSpec {
            path: temp.path().join("reuse/cat.jar"),
            url: "https://unreachable.invalid/cat.jar".into(),
            hash,
            sha256: false,
            size: bytes.len() as u64,
        };
        downloader
            .batch(vec![spec.clone()], &cancel, &sink())
            .await
            .unwrap();
        assert_eq!(fs::read(spec.path).unwrap(), bytes);
        assert!(
            launcher
                .download_cached_resource(&scope, "other", "one", &cancel, &sink())
                .await
                .unwrap_err()
                .to_string()
                .contains("不一致")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn library_rejects_symlinked_storage_and_sources() {
        let (temp, launcher, _, _) = fixture(true);
        let original = temp.path().join("source/cat.jar");
        archive(
            &original,
            &[("fabric.mod.json", r#"{"id":"cat","version":"1"}"#)],
        );
        let linked = temp.path().join("linked.jar");
        std::os::unix::fs::symlink(&original, &linked).unwrap();
        assert!(
            launcher
                .cache_game_resources(vec![local(&linked)], vec![], &CancellationToken::new())
                .await
                .is_err()
        );
        let root = launcher.resource_library_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("mods")).unwrap();
        assert!(
            launcher
                .cache_game_resources(vec![local(&original)], vec![], &CancellationToken::new())
                .await
                .is_err()
        );
        assert!(fs::read_dir(outside).unwrap().next().is_none());
    }
}
