use crate::{
    archive_support as archive,
    download::{Downloader, FileSpec},
    model::*,
    storage, version,
};
use crate::{fabric, java, minecraft};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use serde_json::json;
use std::{collections::HashMap, fs::File, io::Read};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub(crate) struct Install<'a> {
    pub root: &'a Path,
    pub game: &'a Path,
    pub minecraft: &'a str,
    pub loader: Option<&'a ModLoader>,
    pub id: &'a str,
    pub explicit_java: Option<&'a str>,
    pub downloader: &'a Downloader,
    pub cancel: &'a CancellationToken,
    pub sink: &'a EventSink,
    pub auto_java: bool,
}
impl Install<'_> {
    pub async fn run(&self) -> Result<Value> {
        let Self {
            root,
            game,
            minecraft,
            loader,
            id,
            downloader,
            cancel,
            sink,
            ..
        } = self;
        let base = minecraft::version(root, minecraft, downloader, cancel, sink).await?;
        // Shared vanilla metadata is immutable; never overwrite an unrelated local profile.
        let base_value = serde_json::to_value(&base)?;
        let base_path = version::metadata_path(game, minecraft)?;
        if !base_path.exists() {
            storage::write_json(&base_path, &base_value)?;
        }
        sink(Event::workflow("loader", "安装加载器"));
        let mut profile = match loader {
            None => json!({"id":id,"inheritsFrom":minecraft}),
            Some(loader) if loader.kind == "fabric" => {
                fabric::profile(root, minecraft, &loader.version, downloader, cancel).await?
            }
            Some(loader) => {
                // Processors need the original client JAR, not a merged/patched one.
                let client = base
                    .downloads
                    .get("client")
                    .context("元数据缺少客户端文件")?;
                downloader
                    .file(
                        &FileSpec {
                            url: client.url.clone(),
                            path: version::client_path(game, &base)?,
                            hash: client.sha1.clone(),
                            sha256: false,
                            size: client.size,
                        },
                        cancel,
                        &self.step("loader"),
                    )
                    .await?;
                self.forge(loader, &base).await?
            }
        };
        profile["id"] = json!(id);
        profile["inheritsFrom"] = json!(minecraft);
        profile["jar"] = json!(minecraft);
        let resolved = version::resolve_chain(&[base_value, profile.clone()])?;
        if resolved.loader.as_ref() != *loader {
            bail!("安装器生成的加载器与所选版本不一致");
        }
        sink(Event::workflow("files", "下载与校验游戏文件"));
        minecraft::install_files(
            game,
            &resolved.version,
            downloader,
            cancel,
            &self.files_sink(),
        )
        .await?;
        sink(Event::workflow("java", "准备 Java"));
        let runtime = java::ensure(
            root,
            java::JavaRequirement::for_instance(&resolved.version, game, self.explicit_java)
                .await?,
            self.explicit_java,
            self.auto_java,
            downloader,
            cancel,
            &self.step("java"),
        )
        .await?;
        storage::write_json(
            &game.join("versions").join(id).join("runtime.json"),
            &runtime,
        )?;
        let mut result = serde_json::to_value(resolved.version)?;
        result["patches"] = json!([{"id":"game", "version":minecraft}]);
        // Keep the archive throughout retries/cancellation of later game-file steps.
        if let Some(loader) =
            loader.filter(|loader| matches!(loader.kind.as_str(), "forge" | "neoforge"))
            && let Err(error) = self.discard_forge_installer(loader).await
        {
            sink(Event::new(
                "warning",
                format!("清理加载器安装包失败：{error:#}"),
            ));
        }
        Ok(result)
    }
    async fn discard_forge_installer(&self, loader: &ModLoader) -> Result<()> {
        let cache = self
            .root
            .join("metadata/loader-installers")
            .join(&loader.kind)
            .join(format!("{}-{}", self.minecraft, loader.version));
        let hash = storage::read_json::<String>(&cache.join("installer.sha1.json"))?;
        self.downloader
            .discard_installer(&FileSpec {
                url: String::new(),
                path: cache.join("installer.jar"),
                hash,
                sha256: false,
                size: 0,
            })
            .await
    }
    fn files_sink(&self) -> EventSink {
        let forward = self.sink.clone();
        Arc::new(move |mut event| {
            if event.step.is_none() {
                event.step = Some("files".into());
            }
            forward(event);
        })
    }
    fn step(&self, step: &str) -> EventSink {
        let forward = self.sink.clone();
        let step = step.to_owned();
        Arc::new(move |mut event| {
            event.step = Some(step.clone());
            forward(event);
        })
    }
    async fn forge(&self, loader: &ModLoader, base: &Version) -> Result<Value> {
        let (repository, coordinate) = installer_coordinate(self.minecraft, loader)?;
        let url = format!("{repository}/{}", version::maven_path(&coordinate, None)?);
        let cache = self
            .root
            .join("metadata/loader-installers")
            .join(&loader.kind)
            .join(format!("{}-{}", self.minecraft, loader.version));
        let installer = cache.join("installer.jar");
        let digest_path = cache.join("installer.sha1.json");
        let hash: String = match storage::read_json::<String>(&digest_path)
            .ok()
            .filter(|hash| hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            Some(hash) => hash,
            None => {
                let hash = self.downloader.maven_sha1(&url, self.cancel).await?;
                storage::write_json(&digest_path, &hash)?;
                hash
            }
        };
        let installer_spec = FileSpec {
            url,
            path: installer.clone(),
            hash,
            sha256: false,
            size: 0,
        };
        self.downloader
            .file(&installer_spec, self.cancel, &self.step("loader"))
            .await?;
        let mut zip = archive::open(&installer)?;
        let install = archive::json(&mut zip, "install_profile.json")?;
        let modern = install["spec"].as_u64().is_some();
        let mut profile = if modern {
            if install["minecraft"].as_str() != Some(self.minecraft) {
                bail!("加载器安装器与游戏版本不匹配");
            }
            archive::json(
                &mut zip,
                install["json"]
                    .as_str()
                    .unwrap_or("version.json")
                    .trim_start_matches('/'),
            )?
        } else {
            install["versionInfo"]
                .as_object()
                .context("不支持此加载器安装器格式")?;
            install["versionInfo"].clone()
        };
        let libraries = self.game.join("libraries");
        std::fs::create_dir_all(&libraries)?;
        // Embedded Maven artifacts come only from the official, hash-verified installer.
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let Some(relative) = entry.name().strip_prefix("maven/") else {
                continue;
            };
            if relative.is_empty() || entry.is_dir() {
                continue;
            }
            let out = safe_output(&libraries, relative)?;
            std::fs::create_dir_all(out.parent().unwrap())?;
            std::io::copy(&mut entry, &mut File::create(out)?)?;
        }
        if !modern {
            let coordinate = install["install"]["path"]
                .as_str()
                .context("旧版 Forge 安装器缺少依赖路径")?;
            let relative = version::maven_path(coordinate, None)?;
            let out = safe_output(&libraries, &relative)?;
            let entry = install["install"]["filePath"]
                .as_str()
                .context("旧版 Forge 安装器缺少通用 JAR")?;
            let mut source = zip.by_name(entry)?;
            std::fs::create_dir_all(out.parent().unwrap())?;
            std::io::copy(&mut source, &mut File::create(&out)?)?;
            let sha1 = sha1_file(&out)?;
            for library in profile["libraries"]
                .as_array_mut()
                .context("加载器依赖缺失")?
            {
                if library["name"] == coordinate {
                    library["downloads"] = json!({"artifact":{"path":relative,"url":"","sha1":sha1,"size":std::fs::metadata(&out)?.len()}});
                }
            }
        }
        let mut install_libraries = install["libraries"].as_array().cloned().unwrap_or_default();
        let profile_libraries = profile["libraries"]
            .as_array_mut()
            .context("加载器依赖缺失")?;
        // Hydrate legacy SHA-1 sidecars while retaining blank URLs for generated files.
        for library in profile_libraries
            .iter_mut()
            .chain(install_libraries.iter_mut())
        {
            version::normalize_library(library)?;
            let artifact = &mut library["downloads"]["artifact"];
            if let Some(url) = artifact["url"]
                .as_str()
                .and_then(|u| u.strip_prefix("http://"))
            {
                artifact["url"] = json!(format!("https://{url}"));
            }
            if !artifact.is_object() {
                continue;
            }
            let path = safe_output(
                &libraries,
                artifact["path"].as_str().context("依赖路径缺失")?,
            )?;
            if artifact["sha1"].as_str().unwrap_or("").is_empty() {
                let url = artifact["url"].as_str().unwrap_or("");
                let hash = if !url.is_empty() {
                    self.downloader.maven_sha1(url, self.cancel).await?
                } else if path.is_file() {
                    sha1_file(&path)?
                } else {
                    continue;
                };
                artifact["sha1"] = json!(hash);
            }
        }
        let mut pending = vec![];
        for library in install_libraries.iter().chain(profile_libraries.iter()) {
            if library["clientreq"] == false {
                continue;
            }
            let a = &library["downloads"]["artifact"];
            let Some(url) = a["url"].as_str().filter(|u| !u.is_empty()) else {
                continue;
            };
            // Old Forge metadata still contains HTTP Maven URLs. Upgrade transport.
            let url = if let Some(rest) = url.strip_prefix("http://") {
                format!("https://{rest}")
            } else {
                url.into()
            };
            pending.push(FileSpec {
                url,
                path: safe_output(&libraries, a["path"].as_str().context("依赖路径缺失")?)?,
                hash: a["sha1"].as_str().unwrap_or("").into(),
                sha256: false,
                size: a["size"].as_u64().unwrap_or(0),
            });
        }
        self.downloader
            .batch(pending, self.cancel, &self.step("loader"))
            .await?;
        if modern {
            let mut runtime_profile = profile.clone();
            runtime_profile["id"] = json!(self.id);
            let resolved = version::resolve_chain(&[serde_json::to_value(base)?, runtime_profile])?;
            let requirement = java::JavaRequirement::for_instance(
                &resolved.version,
                self.game,
                self.explicit_java,
            )
            .await?;
            let runtime = java::ensure(
                self.root,
                requirement,
                self.explicit_java,
                self.auto_java,
                self.downloader,
                self.cancel,
                &self.step("loader"),
            )
            .await?;
            let mut data = HashMap::from([
                ("SIDE".into(), "client".into()),
                ("MINECRAFT_VERSION".into(), self.minecraft.to_owned()),
                ("ROOT".into(), java::argument_path(self.game)),
                ("LIBRARY_DIR".into(), java::argument_path(&libraries)),
                ("INSTALLER".into(), java::argument_path(&installer)),
                (
                    "MINECRAFT_JAR".into(),
                    java::argument_path(&version::client_path(self.game, base)?),
                ),
            ]);
            if let Some(entries) = install["data"].as_object() {
                for (key, value) in entries {
                    let Some(value) = value["client"].as_str() else {
                        continue;
                    };
                    let expanded = if let Some(text) =
                        value.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))
                    {
                        text.into()
                    } else if value.starts_with('[') {
                        expand(value, &data, &libraries)?
                    } else {
                        let relative = value.trim_start_matches('/');
                        let out = safe_output(&cache.join("data"), relative)?;
                        let mut entry = zip.by_name(relative)?;
                        std::fs::create_dir_all(out.parent().unwrap())?;
                        std::io::copy(&mut entry, &mut File::create(&out)?)?;
                        java::argument_path(&out)
                    };
                    data.insert(key.clone(), expanded);
                }
            }
            for (index, processor) in install["processors"]
                .as_array()
                .context("加载器处理步骤缺失")?
                .iter()
                .enumerate()
            {
                if processor["sides"]
                    .as_array()
                    .is_some_and(|s| !s.iter().any(|v| v == "client"))
                {
                    continue;
                }
                crate::download::check_cancel(self.cancel)?;
                let mut outputs = vec![];
                if let Some(values) = processor["outputs"].as_object() {
                    for (path, hash) in values {
                        outputs.push((
                            PathBuf::from(expand(path, &data, &libraries)?),
                            expand(
                                hash.as_str().context("处理器输出校验值无效")?,
                                &data,
                                &libraries,
                            )?,
                        ));
                    }
                }
                let mut complete = !outputs.is_empty();
                for (path, hash) in &outputs {
                    complete &= Downloader::verified(path, hash, false, 0).await?;
                }
                if complete {
                    continue;
                }
                self.sink.as_ref()(Event::new(
                    "stage",
                    format!("运行加载器安装步骤 {}", index + 1),
                ));
                let jar = libraries.join(version::maven_path(
                    processor["jar"].as_str().context("安装处理器缺少 JAR")?,
                    None,
                )?);
                let main = main_class(&jar)?;
                let mut classpath = vec![jar];
                for value in processor["classpath"]
                    .as_array()
                    .context("安装处理器缺少 classpath")?
                {
                    classpath.push(libraries.join(version::maven_path(
                        value.as_str().context("安装处理器依赖无效")?,
                        None,
                    )?));
                }
                let classpath = classpath
                    .iter()
                    .map(|p| java::argument_path(p))
                    .collect::<Vec<_>>()
                    .join(if cfg!(windows) { ";" } else { ":" });
                let args: Vec<_> = processor["args"]
                    .as_array()
                    .context("安装处理器缺少参数")?
                    .iter()
                    .map(|arg| expand(arg.as_str().context("处理器参数无效")?, &data, &libraries))
                    .collect::<Result<_>>()?;
                let log_path = cache.join(format!("processor-{index}.log"));
                let stdout = File::create(&log_path)?;
                let mut command = java::background_command(Path::new(&runtime.path));
                command
                    .arg("-Djava.awt.headless=true")
                    .arg("-cp")
                    .arg(classpath)
                    .arg(main)
                    .args(args)
                    .current_dir(self.game)
                    .stdout(stdout.try_clone()?)
                    .stderr(stdout)
                    .kill_on_drop(true);
                let status = run_processor(command, self.cancel).await?;
                if !status.success() {
                    let text = std::fs::read(&log_path).unwrap_or_default();
                    let tail = &text[text.len().saturating_sub(4096)..];
                    bail!("加载器安装步骤失败：{}", java::decode_output(tail));
                }
                for (path, hash) in outputs {
                    if !Downloader::verified(&path, &hash, false, 0).await? {
                        bail!("加载器生成文件校验失败：{}", path.display());
                    }
                }
            }
        }
        // Hash generated artifacts when their installer profile omitted a digest.
        for library in profile["libraries"].as_array_mut().unwrap() {
            let a = &mut library["downloads"]["artifact"];
            if a.is_object() && a["sha1"].as_str().unwrap_or("").is_empty() {
                let path = safe_output(&libraries, a["path"].as_str().context("依赖路径缺失")?)?;
                a["sha1"] = json!(sha1_file(&path)?);
                a["size"] = json!(std::fs::metadata(path)?.len());
            }
        }
        Ok(profile)
    }
}
async fn run_processor(
    mut command: tokio::process::Command,
    cancel: &CancellationToken,
) -> Result<std::process::ExitStatus> {
    let mut pause = super::INSTALL_PAUSE.try_with(Clone::clone).ok();
    let token = cancel.child_token();
    let _cleanup = token.clone().drop_guard();
    // This task keeps receiving pause/cancel even when the UI suspends polling
    // the surrounding installation. Resume reruns only the interrupted processor.
    tokio::spawn(async move {
        loop {
            if let Some(receiver) = &mut pause {
                while *receiver.borrow_and_update() {
                    tokio::select! {
                        _ = token.cancelled() => bail!("操作已取消"),
                        changed = receiver.changed() => if changed.is_err() { break; },
                    }
                }
            }
            crate::download::check_cancel(&token)?;
            let mut child = command.spawn()?;
            loop {
                tokio::select! {
                    biased;
                    _ = token.cancelled() => { child.kill().await?; bail!("操作已取消"); },
                    changed = async {
                        match &mut pause {
                            Some(receiver) => receiver.changed().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        if changed.is_err() { pause = None; }
                        else if pause.as_ref().is_some_and(|p| *p.borrow()) {
                            child.kill().await?;
                            break;
                        }
                    },
                    status = child.wait() => return Ok(status?),
                }
            }
        }
    })
    .await?
}
fn installer_coordinate(game: &str, loader: &ModLoader) -> Result<(&'static str, String)> {
    archive::component(game)?;
    archive::component(&loader.version)?;
    match loader.kind.as_str() {
        "forge" => Ok((
            "https://maven.minecraftforge.net",
            format!(
                "net.minecraftforge:forge:{game}-{}:installer",
                loader.version
            ),
        )),
        "neoforge" if game == "1.20.1" && loader.version.starts_with("47.") => Ok((
            "https://maven.neoforged.net/releases",
            format!("net.neoforged:forge:{game}-{}:installer", loader.version),
        )),
        "neoforge" => Ok((
            "https://maven.neoforged.net/releases",
            format!("net.neoforged:neoforge:{}:installer", loader.version),
        )),
        _ => bail!("不支持此加载器的自动安装"),
    }
}
fn safe_output(root: &Path, path: &str) -> Result<PathBuf> {
    let mut output = root.to_path_buf();
    for component in archive::relative(path)?.components() {
        output.push(component);
        if std::fs::symlink_metadata(&output).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("加载器安装路径包含符号链接");
        }
    }
    Ok(output)
}
fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    let mut file = File::open(path)?;
    let mut hash = Sha1::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
fn main_class(path: &Path) -> Result<String> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut text = String::new();
    zip.by_name("META-INF/MANIFEST.MF")?
        .take(1024 * 1024)
        .read_to_string(&mut text)?;
    let text = text.replace("\r\n ", "").replace("\n ", "");
    Ok(text
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class: "))
        .context("安装处理器没有 Main-Class")?
        .trim()
        .into())
}
fn expand(text: &str, data: &HashMap<String, String>, libraries: &Path) -> Result<String> {
    if let Some(coordinate) = text.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Ok(java::argument_path(
            &libraries.join(version::maven_path(coordinate, None)?),
        ));
    }
    let mut result = String::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('{') {
        result.push_str(before);
        let (key, tail) = after.split_once('}').context("处理器变量缺少结束括号")?;
        result.push_str(
            data.get(key)
                .with_context(|| format!("处理器变量未定义：{key}"))?,
        );
        rest = tail;
    }
    result.push_str(rest);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn legacy_installer_extracts_its_bundled_forge_jar_without_running_processors() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("游戏目录/.minecraft");
        let loader = ModLoader {
            kind: "forge".into(),
            version: "14.23.5.2860".into(),
        };
        let coordinate = "net.minecraftforge:forge:1.12.2-14.23.5.2860";
        let profile = json!({"id":"forge","mainClass":"net.minecraft.launchwrapper.Launch","minecraftArguments":"--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker","libraries":[{"name":coordinate}]});
        let manifest = json!({"install":{"path":coordinate,"filePath":"forge-universal.jar"},"versionInfo":profile});
        let cache = temp
            .path()
            .join("metadata/loader-installers/forge/1.12.2-14.23.5.2860");
        std::fs::create_dir_all(&cache).unwrap();
        let path = cache.join("installer.jar");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        zip.start_file(
            "install_profile.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file(
            "forge-universal.jar",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"legacy bundled jar").unwrap();
        zip.finish().unwrap();
        storage::write_json(
            &cache.join("installer.sha1.json"),
            &sha1_file(&path).unwrap(),
        )
        .unwrap();
        let downloader = Downloader::new(
            temp.path(),
            Settings {
                proxy_mode: "none".into(),
                download_source: "official".into(),
                retries: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        let cancel = CancellationToken::new();
        let result = Install {
            root: temp.path(),
            game: &game,
            minecraft: "1.12.2",
            loader: Some(&loader),
            id: "1.12.2-forge-14.23.5.2860",
            explicit_java: None,
            downloader: &downloader,
            cancel: &cancel,
            sink: &sink,
            auto_java: false,
        }
        .forge(&loader, &crate::test_http::version_fixture("1.12.2"))
        .await
        .unwrap();
        assert_eq!(version::detect_loader(&result), Some(loader));
        let artifact = &result["libraries"][0]["downloads"]["artifact"];
        assert_eq!(artifact["url"], "");
        assert_eq!(
            std::fs::read(
                game.join("libraries")
                    .join(artifact["path"].as_str().unwrap())
            )
            .unwrap(),
            b"legacy bundled jar"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pausing_stops_the_child_and_resuming_restarts_only_that_processor() {
        use std::time::Duration;
        let temp = tempfile::tempdir().unwrap();
        let pid_path = temp.path().join("pid");
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args(["-c", "echo $$ > \"$1\"; exec /bin/sleep 30", "processor"])
            .arg(&pid_path)
            .kill_on_drop(true);
        let (pause, receiver) = tokio::sync::watch::channel(false);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            super::super::with_pause(receiver, run_processor(command, &token)).await
        });
        let first = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_path)
                    && !pid.trim().is_empty()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        pause.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !tokio::process::Command::new("/bin/kill")
                    .args(["-0", first.trim()])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
                    .unwrap()
                    .success()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(!task.is_finished());
        pause.send(false).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if std::fs::read_to_string(&pid_path)
                    .is_ok_and(|pid| !pid.trim().is_empty() && pid != first)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        pause.send(true).unwrap();
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn variables_are_individual_arguments_and_maven_paths_are_bounded() {
        let map = HashMap::from([("ROOT".into(), "C:/游戏/Some Pack".into())]);
        assert_eq!(
            expand("--target={ROOT}/client.jar", &map, Path::new("libraries")).unwrap(),
            "--target=C:/游戏/Some Pack/client.jar"
        );
        assert!(expand("{UNKNOWN}", &map, Path::new("libraries")).is_err());
        assert!(expand("[a:b:../../oops]", &map, Path::new("libraries")).is_err());
    }
}
