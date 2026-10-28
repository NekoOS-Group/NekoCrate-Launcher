//! Download one declared Mod without invoking the rest of a pack installation.
use super::*;
use std::{collections::HashMap, fs};

impl Launcher {
    pub async fn download_declared_mod(
        &self,
        id: &str,
        version_id: &str,
        key: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<String> {
        let _guard = self.0.operation.lock().await;
        self.idle(id)?;
        let game = self.available_game_dir(id)?;
        let (working, _) = self.content_location(id, version_id)?;
        let sink = Event::scoped(sink, id, None);
        sink(Event::workflow("mod_resolve", "解析 Mod 文件"));
        let declared = crate::pack_content::load(&game, version_id, &working)?
            .into_iter()
            .find(|file| file.key == key && file.kind == "mods")
            .context("未找到对应的 Mod 下载声明，请刷新列表")?;
        let mut file = PackFile {
            key: declared.key,
            display_name: Some(declared.name),
            path: declared.path,
            urls: declared.urls,
            sha1: declared.sha1,
            sha512: declared.sha512,
            size: declared.size,
            optional: declared.optional,
            project: declared.project,
            file: declared.file,
            manual_url: None,
        };
        let settings = self.settings();
        let mut downloader = Downloader::new(self.root(), settings.clone())?.for_installation(
            self.root(),
            id,
            &game,
            version_id,
            &format!("mod:{key}"),
        )?;
        if declared.source == "CurseForge" {
            let api_key = self.curseforge_key()?;
            if let Some(key) = &api_key {
                downloader = downloader.with_curseforge_key(self.root(), &settings, key)?;
            }
            if file.urls.is_empty() && !declared.manual {
                let original_path = file.path.take();
                let mut subset = Pack {
                    format: "curseforge".into(),
                    name: String::new(),
                    version: String::new(),
                    minecraft: String::new(),
                    loader: None,
                    overrides: vec![],
                    files: vec![file],
                    manifest: Value::Null,
                };
                let api = crate::network::client_builder(self.root(), &settings)?
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?;
                provider::resolve(&mut subset, api_key, &api, cancel, |_| Ok(())).await?;
                file = subset.files.remove(0);
                if crate::pack_content::kind(file.path.as_deref()) != "mods" {
                    bail!("此声明对应的文件不是 Mod");
                }
                // Existing manifests may deliberately use an alternate filename.
                file.path = original_path.or(file.path);
            }
        }
        if file.urls.is_empty() {
            bail!("此 Mod 没有可自动下载的地址，请从作者页面下载后拖入此版本");
        }
        for url in &file.urls {
            archive::https(url)?;
        }
        if file.sha1.len() != 40
            || !file.sha1.bytes().all(|c| c.is_ascii_hexdigit())
            || file.size > archive::MAX_ENTRY
            || (!file.sha512.is_empty()
                && (file.sha512.len() != 128
                    || !file.sha512.bytes().all(|c| c.is_ascii_hexdigit())))
        {
            bail!("Mod 下载声明缺少有效校验值或文件大小超限");
        }
        file.sha1.make_ascii_lowercase();
        file.sha512.make_ascii_lowercase();
        let relative = file.path.as_deref().context("Mod 文件路径缺失")?;
        if !relative.starts_with("mods/") || !relative.to_ascii_lowercase().ends_with(".jar") {
            bail!("Mod 下载目标必须是 mods 目录内的 JAR 文件");
        }
        let destination = archive::installed_destination(&working, relative)?;
        let name = destination
            .strip_prefix(working.join("mods"))?
            .to_string_lossy()
            .replace('\\', "/");
        if destination.try_exists()? {
            if Downloader::verified(&destination, &file.sha1, false, file.size).await? {
                if !file.sha512.is_empty() {
                    verify_sha512(&destination, &file.sha512, cancel).await?;
                }
                downloader
                    .index_installed(&FileSpec {
                        path: destination,
                        url: String::new(),
                        hash: file.sha1.clone(),
                        sha256: false,
                        size: file.size,
                    })
                    .await?;
                downloader.finish_installation(&sink).await;
                return Ok(name);
            }
            bail!("同名 Mod 文件已存在且内容不同，未覆盖");
        }
        crate::download::check_cancel(cancel)?;
        let stage = archive::managed_destination(
            &working,
            &format!(
                ".nekocrate/mod-downloads/{}/{}",
                file.sha1,
                destination.file_name().unwrap().to_string_lossy()
            ),
        )?;
        let cache = archive::managed_destination(&working, ".nekocrate/downloads")?;
        let spec = FileSpec {
            path: stage.clone(),
            url: file.urls[0].clone(),
            hash: file.sha1.clone(),
            sha256: false,
            size: file.size,
        };
        downloader = downloader
            .with_mirrors(HashMap::from([(stage.clone(), file.urls.clone())]))
            .with_cache(cache);
        downloader
            .retain_until_installed(vec![stage.clone()])
            .await?;
        sink(Event::workflow("mod_files", "下载与校验 Mod"));
        let files_sink = Event::scoped(&sink, id, Some("mod_files"));
        downloader
            .import_cached_files(self.root(), std::slice::from_ref(&spec), cancel)
            .await?;
        downloader
            .batch(vec![spec.clone()], cancel, &files_sink)
            .await?;
        if !file.sha512.is_empty() {
            verify_sha512(&stage, &file.sha512, cancel).await?;
        }
        crate::download::check_cancel(cancel)?;
        sink(Event::workflow("mod_commit", "安装 Mod"));
        // Validate again after network work, before creating any final directories.
        let destination = archive::installed_destination(&working, relative)?;
        if destination.try_exists()? {
            bail!("同名 Mod 文件已存在且内容不同，未覆盖");
        }
        if game
            .join("versions")
            .join(version_id)
            .join(".nekocrate/modpack.json")
            .is_file()
        {
            let mut job = record::read_job(&game, version_id)?;
            if let Some(existing) = job.pack.files.iter_mut().find(|item| item.key == file.key) {
                *existing = file;
            } else {
                job.pack.files.push(file);
            }
            record::write_job(&game, version_id, &job)?;
        }
        fs::create_dir_all(destination.parent().unwrap())?;
        // Same-volume staging: publish exclusively, never overwrite a concurrently created file.
        fs::hard_link(&stage, &destination).context("无法写入 Mod 文件，已有文件未被覆盖")?;
        let _ = fs::remove_file(stage);
        downloader
            .index_installed(&FileSpec {
                path: destination,
                ..spec
            })
            .await?;
        downloader.finish_installation(&sink).await;
        Ok(name)
    }
}
