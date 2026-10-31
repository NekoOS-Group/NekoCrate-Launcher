use super::*;
use crate::{
    download::{Downloader, FileSpec},
    model::{Event, EventSink},
    version_management,
};
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    io::{Read, Write},
};
use tokio_util::sync::CancellationToken;

fn sha1(path: &Path, cancel: &CancellationToken) -> Result<String> {
    let mut input = fs::File::open(path)?;
    let mut hash = Sha1::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
fn valid_destination(entry: &PlanEntry, target: &ResourceTarget) -> Result<PathBuf> {
    let parts: Vec<_> = entry.destination.split('/').collect();
    let valid = if entry.kind == ResourceKind::Datapacks {
        parts.len() == 4
            && parts[0] == "saves"
            && parts[2] == "datapacks"
            && target.worlds.iter().any(|w| w == parts[1])
    } else {
        parts.len() == 2 && parts[0] == entry.kind.directory()
    };
    if !valid {
        bail!("资源目标路径与类型不符");
    }
    safe_join(&target.directory, &entry.destination)
}
fn staged_path(directory: &Path, index: usize, entry: &PlanEntry) -> Result<PathBuf> {
    let name = entry
        .destination
        .rsplit('/')
        .next()
        .context("无效资源文件名")?;
    safe_join(directory, &format!("stage/{index}/{name}"))
}
pub(super) fn preflight(plan: &mut ResourcePlan, cancel: &CancellationToken) -> Result<()> {
    let installed = records(&plan.target.directory)?;
    let mut names = HashSet::new();
    for entry in &mut plan.entries {
        entry.reuse = false;
        let path = valid_destination(entry, &plan.target)?;
        if !names.insert(entry.destination.to_lowercase()) {
            plan.conflicts
                .push(format!("多个资源使用相同文件名：{}", entry.destination));
        }
        // Reject case-only collisions consistently on Windows, macOS and Linux.
        if let Some(parent) = path.parent()
            && parent.is_dir()
        {
            let name = path.file_name().unwrap().to_string_lossy();
            for existing in fs::read_dir(parent)? {
                let existing = existing?.file_name().to_string_lossy().into_owned();
                if existing.to_lowercase() == name.to_lowercase() && existing != name {
                    plan.conflicts.push(format!("文件名大小写冲突：{existing}"));
                }
            }
        }
        if path.exists() {
            entry.reuse = if let Some(expected) = &entry.content_sha256 {
                imports::fingerprint(&path, cancel)?.0 == *expected
            } else if let Input::Remote { file } = &entry.input {
                path.is_file() && sha1(&path, cancel)?.eq_ignore_ascii_case(&file.sha1)
            } else {
                false
            };
            if !entry.reuse {
                plan.conflicts
                    .push(format!("目标已存在且内容不同：{}", entry.destination));
            }
        }
        if entry.origin.project.is_some() {
            for (other, record) in &installed {
                if other != &entry.destination
                    && record.origin.platform == entry.origin.platform
                    && record.origin.project == entry.origin.project
                    && safe_join(&plan.target.directory, other)?.exists()
                {
                    plan.conflicts
                        .push(format!("已安装此项目的其他文件：{other}"));
                }
            }
        }
    }
    if let Err(error) = check_mods(plan, None) {
        plan.conflicts.push(error.to_string());
    }
    plan.conflicts.sort();
    plan.conflicts.dedup();
    Ok(())
}
fn remove_staged(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => bail!("暂存路径包含符号链接"),
        Ok(m) if m.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
// exFAT/removable disks may not implement hard links. Still create exclusively,
// verify while copying, and remove only the new partial file on error.
pub(super) fn copy_new(
    source: &Path,
    output: &Path,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = (|| -> Result<()> {
        let mut source = fs::File::open(source)?;
        let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
        let mut bytes = [0; 64 * 1024];
        loop {
            if cancel.is_cancelled() {
                bail!("操作已取消");
            }
            let count = source.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            digest.update(&bytes[..count]);
            target.write_all(&bytes[..count])?;
        }
        if hex::encode(digest.finish()) != expected {
            bail!("资源在安装期间发生变化：{}", output.display());
        }
        target.sync_all()?;
        Ok(())
    })();
    drop(target);
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}
fn check_mods(plan: &ResourcePlan, directory: Option<&Path>) -> Result<()> {
    let mut mods = BTreeMap::<String, String>::new();
    // Read enabled local identities once. Include legacy mods/<Minecraft version> too.
    for folder in [
        plan.target.directory.join("mods"),
        plan.target
            .directory
            .join("mods")
            .join(&plan.target.minecraft),
    ] {
        if !folder.is_dir() {
            continue;
        }
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() || entry.path().extension().is_none_or(|e| e != "jar")
            {
                continue;
            }
            if let Some(identity) = version_management::mod_metadata(&entry.path()) {
                let relative = entry
                    .path()
                    .strip_prefix(&plan.target.directory)?
                    .to_string_lossy()
                    .replace('\\', "/");
                for id in identity.ids {
                    mods.insert(id, relative.clone());
                }
            }
        }
    }
    for (index, entry) in plan.entries.iter().enumerate().filter(|(_, e)| {
        e.kind == ResourceKind::Mods && e.destination.to_ascii_lowercase().ends_with(".jar")
    }) {
        let path = if entry.reuse {
            valid_destination(entry, &plan.target)?
        } else if let Some(directory) = directory {
            staged_path(directory, index, entry)?
        } else if let Input::Local { path, .. } = &entry.input {
            path.clone()
        } else {
            continue;
        };
        if let Some(identity) = version_management::mod_metadata(&path) {
            for id in identity.ids {
                if let Some(other) = mods.insert(id.clone(), entry.destination.clone())
                    && other != entry.destination
                {
                    bail!("Mod ID 冲突：{id} ({other} / {})", entry.destination);
                }
            }
        }
    }
    Ok(())
}

impl Launcher {
    pub async fn install_resources(
        &self,
        id: &str,
        version: &str,
        task: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let _operation = self.0.operation.lock().await;
        let _directory_guard = self.idle(id)?;
        let (mut plan, directory) = {
            // Serialize the target snapshot/checkpoint with isolation edits. No network work
            // or await holds this short lock, and unrelated settings remain editable.
            let _layout = self.0.layout.lock().unwrap();
            let target = self.resource_target(id, version)?;
            let directory = task_dir(&target, task)?;
            let mut plan = read_plan(&safe_join(&directory, "plan.json")?, &target.directory)?;
            if plan.target.instance != id
                || plan.target.version != version
                || plan.target.directory != target.directory
                || plan.target.minecraft != target.minecraft
                || plan.target.loader != target.loader
            {
                bail!("目标版本已变化，请重新生成资源安装计划");
            }
            if !plan.complete {
                if !plan.conflicts.is_empty() {
                    bail!("请先解决安装预览中的冲突并重新生成计划");
                }
                plan.target = target;
                plan.started = true;
                save_plan(&plan)?;
            }
            (plan, directory)
        };
        let game = self.available_game_dir(id)?;
        let mut downloader = Downloader::new(self.root(), self.settings())?.for_installation(
            self.root(),
            id,
            &game,
            version,
            &format!("resource:{task}"),
        )?;
        if plan.complete {
            index_plan(&downloader, &plan).await?;
            downloader.finish_installation(sink).await;
            return Ok(());
        }
        let sink = Event::scoped(sink, id, None);
        sink(Event::workflow("resource_check", "检查资源与目标目录"));
        let token = cancel.clone();
        plan = tokio::task::spawn_blocking(move || {
            preflight(&mut plan, &token)?;
            Ok::<_, anyhow::Error>(plan)
        })
        .await??;
        if !plan.conflicts.is_empty() {
            bail!("{}", plan.conflicts.join("\n"));
        }
        if plan
            .entries
            .iter()
            .any(|e| e.origin.platform == Platform::Curseforge)
        {
            let key = crate::curseforge::key()?;
            downloader = downloader.with_curseforge_key(self.root(), &self.settings(), key)?;
        }
        sink(Event::workflow("resource_files", "下载与暂存资源"));
        let stage = safe_join(&directory, "stage")?;
        fs::create_dir_all(&stage)?;
        let mut remote = Vec::new();
        for (index, entry) in plan.entries.iter().enumerate() {
            if entry.reuse {
                continue;
            }
            let output = staged_path(&directory, index, entry)?;
            match &entry.input {
                Input::Remote { file } => {
                    let url = reqwest::Url::parse(&file.url)?;
                    if url.scheme() != "https"
                        || !url.username().is_empty()
                        || url.password().is_some()
                        || file.sha1.len() != 40
                        || !file.sha1.bytes().all(|c| c.is_ascii_hexdigit())
                    {
                        bail!("无效资源下载地址或校验信息");
                    }
                    remote.push(FileSpec {
                        url: file.url.clone(),
                        path: output,
                        hash: file.sha1.clone(),
                        sha256: false,
                        size: file.size,
                    });
                }
                Input::Local { .. } => {
                    let entry = entry.clone();
                    let token = cancel.clone();
                    let files = Event::scoped(&sink, id, Some("resource_files"));
                    let file_id = entry.destination.clone();
                    let name = entry.origin.name.clone();
                    let emit_file = move |status: &str, bytes: u64, total: u64| {
                        files(Event {
                            files: vec![crate::model::FileProgress {
                                error: None,
                                retryable: false,
                                id: file_id.clone(),
                                name: name.clone(),
                                downloaded: bytes,
                                total,
                                status: status.into(),
                                cached: false,
                            }],
                            ..Event::new("file", "")
                        })
                    };
                    emit_file("queued", 0, entry.size);
                    tokio::task::spawn_blocking(move || {
                        remove_staged(&output)?;
                        let result = imports::stage(&entry, &output, &token, &|bytes, total| {
                            emit_file("installing", bytes, total)
                        });
                        match result {
                            Ok(total) => {
                                emit_file("done", total, total);
                                Ok(())
                            }
                            Err(error) => {
                                emit_file("error", 0, entry.size);
                                Err(error)
                            }
                        }
                    })
                    .await??;
                }
            }
        }
        downloader
            .batch(
                remote,
                cancel,
                &Event::scoped(&sink, id, Some("resource_files")),
            )
            .await?;
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        sink(Event::workflow("resource_commit", "校验并安装资源"));
        // Once publication begins, finish this short transaction even if cancellation arrives.
        // Staging and plans remain on failure so a restart can reuse verified downloads.
        let token = cancel.clone();
        let plan = tokio::task::spawn_blocking(move || {
            let working = plan.target.directory.clone();
            commit(plan, &directory, token)?;
            read_plan(&safe_join(&directory, "plan.json")?, &working)
        })
        .await??;
        index_plan(&downloader, &plan).await?;
        downloader.finish_installation(&sink).await;
        sink(Event::workflow("finalize", "资源安装完成"));
        Ok(())
    }
}
async fn index_plan(downloader: &Downloader, plan: &ResourcePlan) -> Result<()> {
    for entry in &plan.entries {
        let path = valid_destination(entry, &plan.target)?;
        if !path.is_file() {
            continue;
        }
        let (hash, sha256) = match &entry.input {
            Input::Remote { file } => (file.sha1.clone(), false),
            Input::Local { .. } => {
                downloader.index_local(&path).await?;
                continue;
            }
        };
        downloader
            .index_installed(&FileSpec {
                path,
                hash,
                sha256,
                size: 0,
                url: String::new(),
            })
            .await?;
    }
    downloader
        .forget_staging(&task_dir(&plan.target, &plan.id)?.join("stage"))
        .await;
    Ok(())
}
fn commit(mut plan: ResourcePlan, directory: &Path, cancel: CancellationToken) -> Result<()> {
    check_mods(&plan, Some(directory))?;
    for (index, entry) in plan.entries.iter_mut().enumerate() {
        let path = if entry.reuse {
            valid_destination(entry, &plan.target)?
        } else {
            staged_path(directory, index, entry)?
        };
        let actual = imports::fingerprint(&path, &cancel)?.0;
        if entry
            .content_sha256
            .as_ref()
            .is_some_and(|expected| expected != &actual)
        {
            bail!("资源在安装期间发生变化：{}", entry.destination);
        }
        if let Input::Remote { file } = &entry.input
            && !sha1(&path, &cancel)?.eq_ignore_ascii_case(&file.sha1)
        {
            bail!("资源校验失败：{}", entry.destination);
        }
        entry.content_sha256 = Some(actual);
    }
    // Checkpoint verified payload hashes before publishing any files. This also recovers
    // from process termination between a rename and writing provenance.
    save_plan(&plan)?;
    preflight(&mut plan, &cancel)?;
    if !plan.conflicts.is_empty() {
        bail!("{}", plan.conflicts.join("\n"));
    }
    if cancel.is_cancelled() {
        bail!("操作已取消");
    }
    let mut provenance = records(&plan.target.directory)?;
    let mut created: Vec<(PathBuf, String)> = Vec::new();
    let result = (|| -> Result<()> {
        for (index, entry) in plan.entries.iter().enumerate() {
            let output = valid_destination(entry, &plan.target)?;
            if !entry.reuse {
                fs::create_dir_all(output.parent().context("无效目标目录")?)?;
                let source = staged_path(directory, index, entry)?;
                if output.exists() {
                    bail!("目标在安装期间发生变化：{}", entry.destination);
                }
                if source.is_dir() {
                    // rename fails for an existing nonempty directory; user data is never merged.
                    fs::rename(&source, &output)?;
                } else {
                    // Atomic create-without-replace, on the same volume as the staging directory.
                    if let Err(error) = fs::hard_link(&source, &output) {
                        if error.kind() == std::io::ErrorKind::AlreadyExists {
                            return Err(error.into());
                        }
                        copy_new(
                            &source,
                            &output,
                            entry.content_sha256.as_deref().unwrap(),
                            &cancel,
                        )?;
                    }
                }
                created.push((output.clone(), entry.content_sha256.clone().unwrap()));
            }
            provenance.insert(
                entry.destination.clone(),
                ResourceRecord {
                    kind: entry.kind,
                    origin: entry.origin.clone(),
                    sha256: entry.content_sha256.clone().unwrap(),
                    sha1: match &entry.input {
                        Input::Remote { file } => Some(file.sha1.clone()),
                        _ => None,
                    },
                    installed_at: storage::now(),
                },
            );
        }
        storage::write_json(
            &safe_join(&plan.target.directory, ".nekocrate/resources.json")?,
            &provenance,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        for (path, hash) in created.iter().rev() {
            // Never remove a file somebody else modified after publication.
            if imports::fingerprint(path, &CancellationToken::new())
                .is_ok_and(|(actual, _)| actual == *hash)
            {
                let _ = remove_staged(path);
            }
        }
        return Err(error);
    }
    plan.complete = true;
    save_plan(&plan)?;
    let _ = remove_staged(&safe_join(directory, "stage")?);
    Ok(())
}
