//! Conservative Fabric missing-dependency repair. Search results are suggestions;
//! only the artifact's own metadata establishes a Mod ID and its constraints.
use super::*;
use crate::{
    download::{Downloader, FileSpec},
    model::{Event, EventSink},
};
use serde_json::Value;
use std::{
    collections::HashSet,
    io::{Cursor, Read, Seek, Write},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FabricMod {
    pub(crate) id: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) provides: Vec<String>,
    #[serde(default)]
    pub(crate) depends: BTreeMap<String, Value>,
    #[serde(default)]
    pub(crate) breaks: BTreeMap<String, Value>,
}
impl FabricMod {
    fn provides(&self, id: &str) -> bool {
        self.id == id || self.provides.iter().any(|alias| alias == id)
    }
}
type Requirements = BTreeMap<String, Vec<Value>>;

use crate::fabric_version::matches_json as matches;

const MEMORY_JAR_LIMIT: u64 = 2 * 1024 * 1024;
const NESTED_JAR_LIMIT: u64 = 256 * 1024 * 1024;
const NESTED_TOTAL_LIMIT: u64 = 1024 * 1024 * 1024;

// Large bundled libraries are not metadata. Spool their ZIP container to a
// private temporary file so opening a big Mod never retains its entire nested
// library tree in RAM. Drop closes the handle before unlinking (also on Windows).
enum NestedJar {
    Memory(Cursor<Vec<u8>>),
    Disk {
        file: Option<fs::File>,
        path: PathBuf,
    },
}
impl NestedJar {
    fn reader(&mut self) -> &mut dyn ReadSeek {
        match self {
            Self::Memory(cursor) => cursor,
            Self::Disk { file, .. } => file.as_mut().unwrap(),
        }
    }
    fn read(input: &mut dyn Read, size: u64, cancel: &CancellationToken) -> Result<Self> {
        anyhow::ensure!(
            size <= NESTED_JAR_LIMIT,
            "单个嵌套 Mod 超过 256 MiB 读取限制"
        );
        let mut archive = if size <= MEMORY_JAR_LIMIT {
            Self::Memory(Cursor::new(Vec::with_capacity(size as usize)))
        } else {
            let path =
                std::env::temp_dir().join(format!("nekocrate-mod-{}.jar", uuid::Uuid::new_v4()));
            let mut options = fs::OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&path).context("无法暂存嵌套 Mod")?;
            Self::Disk {
                file: Some(file),
                path,
            }
        };
        let output: &mut dyn Write = match &mut archive {
            Self::Memory(cursor) => cursor,
            Self::Disk { file, .. } => file.as_mut().unwrap(),
        };
        let mut copied = 0u64;
        let mut buffer = [0; 64 * 1024];
        loop {
            anyhow::ensure!(!cancel.is_cancelled(), "操作已取消");
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied += count as u64;
            anyhow::ensure!(copied <= size, "嵌套 Mod 实际大小超过声明值");
            output.write_all(&buffer[..count])?;
        }
        anyhow::ensure!(copied == size, "嵌套 Mod 数据不完整");
        archive.reader().rewind()?;
        Ok(archive)
    }
}
impl Drop for NestedJar {
    fn drop(&mut self) {
        if let Self::Disk { file, path } = self {
            drop(file.take());
            let _ = fs::remove_file(path);
        }
    }
}

fn read_jar(
    input: &mut dyn ReadSeek,
    depth: usize,
    budget: &mut u64,
    result: &mut Vec<FabricMod>,
    cancel: &CancellationToken,
) -> Result<()> {
    anyhow::ensure!(
        depth <= 8 && result.len() < 2048,
        "嵌套 Mod 结构过于复杂，无法自动修复"
    );
    anyhow::ensure!(!cancel.is_cancelled(), "操作已取消");
    let mut zip = zip::ZipArchive::new(input)?;
    let metadata: Value = {
        let Ok(mut file) = zip.by_name("fabric.mod.json") else {
            return Ok(());
        };
        anyhow::ensure!(file.size() <= 1024 * 1024, "Mod 元数据过大");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        crate::fabric_metadata::parse(&bytes)?
    };
    if metadata["environment"] == "server" {
        return Ok(());
    }
    result.push(serde_json::from_value(metadata.clone())?);
    for nested in metadata["jars"].as_array().into_iter().flatten() {
        let name = nested["file"].as_str().context("嵌套 Mod 路径无效")?;
        storage::safe_relative(name)?;
        let mut archive = {
            let mut file = zip.by_name(name)?;
            let size = file.size();
            *budget = budget
                .checked_sub(size)
                .context("嵌套 Mod 累计读取超过 1 GiB 限制")?;
            NestedJar::read(&mut file, size, cancel)?
        };
        read_jar(archive.reader(), depth + 1, budget, result, cancel)?;
    }
    Ok(())
}
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
pub(crate) fn metadata(path: &Path, cancel: &CancellationToken) -> Result<Vec<FabricMod>> {
    let mut result = Vec::new();
    let mut budget = NESTED_TOTAL_LIMIT;
    read_jar(
        &mut fs::File::open(path)?,
        0,
        &mut budget,
        &mut result,
        cancel,
    )
    .with_context(|| {
        format!(
            "无法读取 Mod 元数据：{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })?;
    Ok(result)
}
fn installed(target: &ResourceTarget, cancel: &CancellationToken) -> Result<Vec<FabricMod>> {
    anyhow::ensure!(
        target.loader.as_ref().is_some_and(|l| l.kind == "fabric"),
        "目前自动补全前置适用于 Fabric；请在管理 Mods 中处理此加载器的依赖"
    );
    anyhow::ensure!(
        !target
            .directory
            .join("config/fabric_loader_dependencies.json")
            .exists(),
        "此版本使用自定义 Fabric 依赖覆盖，请在管理 Mods 中处理依赖"
    );
    let mut mods = Vec::new();
    let root = target.directory.join("mods");
    for directory in [&root, &root.join(&target.minecraft)] {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry.path().extension().is_some_and(|e| e == "jar")
                && !entry.file_name().to_string_lossy().starts_with('.')
            {
                mods.extend(metadata(&entry.path(), cancel)?);
            }
        }
    }
    Ok(mods)
}
fn requirements(mods: &[FabricMod]) -> Requirements {
    let mut result = Requirements::new();
    for item in mods {
        for (id, range) in &item.depends {
            if !["java", "minecraft", "fabricloader"].contains(&id.as_str()) {
                result.entry(id.clone()).or_default().push(range.clone());
            }
        }
    }
    result
}
fn satisfies(mods: &[FabricMod], id: &str, constraints: &[Value]) -> bool {
    mods.iter().any(|m| {
        m.provides(id)
            && constraints
                .iter()
                .all(|r| matches(r, &m.version) == Some(true))
    })
}
fn missing(mods: &[FabricMod]) -> Result<Requirements> {
    let mut missing = Requirements::new();
    for (id, ranges) in requirements(mods) {
        if satisfies(mods, &id, &ranges) {
            continue;
        }
        anyhow::ensure!(
            !mods.iter().any(|m| m.provides(&id)),
            "已有 Mod {id} 不满足依赖约束，请在管理 Mods 中选择兼容版本；不会自动替换"
        );
        provider::identifier(&id)?;
        missing.insert(id, ranges);
    }
    Ok(missing)
}
fn check_compatibility(mods: &[FabricMod], target: &ResourceTarget) -> Result<()> {
    for item in mods {
        for (id, range) in item.depends.iter().chain(&item.breaks) {
            anyhow::ensure!(
                matches(range, "0").is_some(),
                "{} 的 {id} 版本约束格式无效：{range}；未执行自动修复",
                item.id
            );
        }
        for (id, version) in [
            ("minecraft", target.minecraft.as_str()),
            (
                "fabricloader",
                target
                    .loader
                    .as_ref()
                    .context("缺少加载器")?
                    .version
                    .as_str(),
            ),
        ] {
            if let Some(range) = item.depends.get(id) {
                anyhow::ensure!(
                    matches(range, version) == Some(true),
                    "{} 要求 {id} {range}，当前为 {version}",
                    item.id
                );
            }
            if let Some(range) = item.breaks.get(id) {
                anyhow::ensure!(
                    matches(range, version) == Some(false),
                    "{} 与 {id} {version} 不兼容",
                    item.id
                );
            }
        }
        for (id, range) in &item.breaks {
            for other in mods.iter().filter(|m| m.provides(id)) {
                anyhow::ensure!(
                    matches(range, &other.version) == Some(false),
                    "{} 与 {id} {} 存在冲突或无法确认兼容，未自动安装",
                    item.id,
                    other.version
                );
            }
        }
    }
    Ok(())
}

/// Re-read the source Mods and staged artifacts after a pause/resume as well.
/// Nothing is published until the entire graph passes these checks.
pub(super) fn validate(
    plan: &ResourcePlan,
    directory: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    if plan.repair_dependencies.is_empty() {
        return Ok(());
    }
    let mut mods = installed(&plan.target, cancel)?;
    let required = requirements(&mods);
    for id in &plan.repair_dependencies {
        anyhow::ensure!(
            required.contains_key(id),
            "需要 {id} 的 Mod 已发生变化，请重新启动游戏后诊断"
        );
    }
    for (index, entry) in plan.entries.iter().enumerate() {
        anyhow::ensure!(
            entry.kind == ResourceKind::Mods,
            "依赖修复计划中出现非 Mod 文件"
        );
        if !entry.reuse {
            mods.extend(metadata(
                &install::staged_path(directory, index, entry)?,
                cancel,
            )?);
        }
    }
    check_compatibility(&mods, &plan.target)?;
    let pending = missing(&mods)?;
    anyhow::ensure!(
        pending.is_empty(),
        "仍缺少必要 Mod：{}",
        pending.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    Ok(())
}

impl Launcher {
    pub async fn prepare_missing_dependencies(
        &self,
        id: &str,
        version: &str,
        time: u64,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<ResourcePlan> {
        let _guard = self.resource_directory_idle(id, version)?;
        let report = self.crash_report(id)?.context("未找到本次启动的诊断")?;
        anyhow::ensure!(
            report.version == version && report.time == time,
            "诊断已被新的启动记录替换，请重新打开日志"
        );
        let ids = crate::crash::missing_dependencies(
            &report
                .findings
                .iter()
                .filter(|f| f.code == "missing_dependencies")
                .flat_map(|f| f.evidence.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        anyhow::ensure!(!ids.is_empty(), "此诊断没有可自动修复的缺失前置");
        let target = self.resource_target(id, version)?;
        let sink = Event::scoped(sink, id, None);
        sink(Event::workflow("repair_check", "读取已安装 Mod 的依赖"));
        let cloned = target.clone();
        let token = cancel.clone();
        let mut mods = tokio::task::spawn_blocking(move || installed(&cloned, &token)).await??;
        let declared = requirements(&mods);
        for id in &ids {
            anyhow::ensure!(
                declared.contains_key(id),
                "当前 Mod 未声明 {id} 依赖，请重新启动游戏后诊断"
            );
        }
        let mut plan = new_plan(target);
        plan.repair_dependencies = ids;
        let directory = task_dir(&plan.target, &plan.id)?;
        let result = self
            .prepare_dependency_plan(&mut plan, &directory, &mut mods, cancel, &sink)
            .await;
        if let Err(error) = &result {
            // This directory is private to a not-yet-published repair plan.
            let _ = fs::remove_dir_all(&directory);
            sink(Event::new(
                "warning",
                format!("[Diagnosis] 前置修复未完成：{error:#}"),
            ));
        }
        result?;
        Ok(plan)
    }

    async fn prepare_dependency_plan(
        &self,
        plan: &mut ResourcePlan,
        directory: &Path,
        mods: &mut Vec<FabricMod>,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        // Reuse the resource installer and its verified staging files. Provider
        // dependency IDs alone are not trusted to establish Fabric identities.
        sink(Event::workflow("repair_resolve", "查找并下载兼容前置"));
        let mut probes = 0;
        loop {
            anyhow::ensure!(!cancel.is_cancelled(), "操作已取消");
            check_compatibility(mods, &plan.target)?;
            let pending = missing(mods)?;
            let Some((id, ranges)) = pending.into_iter().next() else {
                break;
            };
            anyhow::ensure!(
                plan.entries.len() < 32,
                "前置依赖超过自动修复范围，请在管理 Mods 中处理"
            );
            sink(Event::new(
                "stage",
                format!(
                    "[INFO] [Diagnosis] 查找 {id} {ranges:?} · {} · Fabric",
                    plan.target.minecraft
                ),
            ));
            let (entry, found) = self
                .find_dependency(
                    &plan.target,
                    directory,
                    plan.entries.len(),
                    mods,
                    &id,
                    &ranges,
                    &mut probes,
                    cancel,
                    sink,
                )
                .await?;
            sink(Event::new(
                "log",
                format!("[INFO] [Diagnosis] 已核对前置 {id}：{}", entry.destination),
            ));
            mods.extend(found);
            plan.entries.push(entry);
        }
        // A persisted diagnosis may outlive a completed/manual repair. The
        // graph was checked above; an empty plan is a successful no-op.
        if plan.entries.is_empty() {
            return Ok(());
        }
        let mut checked = plan.clone();
        sink(Event::workflow("repair_validate", "核对前置与依赖约束"));
        let token = cancel.clone();
        let directory = directory.to_owned();
        *plan = tokio::task::spawn_blocking(move || {
            install::preflight(&mut checked, &token)?;
            anyhow::ensure!(
                checked.conflicts.is_empty(),
                "{}",
                checked.conflicts.join("\n")
            );
            validate(&checked, &directory, &token)?;
            save_plan(&checked)?;
            Ok::<_, anyhow::Error>(checked)
        })
        .await??;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn find_dependency(
        &self,
        target: &ResourceTarget,
        directory: &Path,
        index: usize,
        installed: &[FabricMod],
        id: &str,
        ranges: &[Value],
        probes: &mut usize,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<(PlanEntry, Vec<FabricMod>)> {
        let platforms = if self.settings().mod_download_source == "curseforge" {
            [Platform::Curseforge, Platform::Modrinth]
        } else {
            [Platform::Modrinth, Platform::Curseforge]
        };
        for platform in platforms {
            anyhow::ensure!(!cancel.is_cancelled(), "操作已取消");
            let Ok(api) = provider::Provider::new(self, platform) else {
                continue;
            };
            let scope = ResourceCatalog {
                minecraft: target.minecraft.clone(),
                loader: target.loader.clone(),
                kind: ResourceKind::Mods,
                platform,
            };
            let mut projects = Vec::new();
            if platform == Platform::Modrinth
                && let Ok(project) = api.project(id, cancel).await
            {
                projects.push(project);
            }
            match self.search_resource_catalog(&scope, id, 0, cancel).await {
                Ok(result) => projects.extend(result.hits.into_iter().take(8)),
                Err(error) => sink(Event::new(
                    "warning",
                    format!("[Diagnosis] {platform:?} 查找 {id}：{error:#}"),
                )),
            }
            let mut seen = HashSet::new();
            for project in projects {
                if !seen.insert(project.id.clone()) {
                    continue;
                }
                anyhow::ensure!(!cancel.is_cancelled(), "操作已取消");
                let versions = match api
                    .versions(target, ResourceKind::Mods, &project.id, cancel)
                    .await
                {
                    Ok(mut versions) => {
                        versions.sort_by(|a, b| b.published.cmp(&a.published));
                        versions
                    }
                    Err(error) => {
                        sink(Event::new(
                            "warning",
                            format!("[Diagnosis] {}：{error:#}", project.name),
                        ));
                        continue;
                    }
                };
                for version in versions.into_iter().take(6) {
                    let Ok(file) = provider::primary_file(&version).cloned() else {
                        continue;
                    };
                    if !file.name.ends_with(".jar")
                        || file.size == 0
                        || file.size > 64 * 1024 * 1024
                    {
                        continue;
                    }
                    *probes += 1;
                    anyhow::ensure!(
                        *probes <= 48,
                        "无法在有限候选中确认 {id}，请在管理 Mods 中手动选择"
                    );
                    let path = safe_join(directory, &format!("probe/{}.jar", file.sha1))?;
                    if path.exists() {
                        fs::remove_file(&path)?;
                    }
                    let mut downloader = Downloader::new(self.root(), self.settings())?;
                    if platform == Platform::Curseforge {
                        downloader = downloader.with_curseforge_key(
                            self.root(),
                            &self.settings(),
                            crate::curseforge::key()?,
                        )?;
                    }
                    let spec = FileSpec {
                        url: file.url.clone(),
                        path: path.clone(),
                        hash: file.sha1.clone(),
                        sha256: false,
                        size: file.size,
                    };
                    sink(Event::new(
                        "log",
                        format!(
                            "[INFO] [Diagnosis] 核对候选 {} · {}",
                            project.name, file.name
                        ),
                    ));
                    if let Err(error) = downloader.batch(vec![spec.clone()], cancel, sink).await {
                        sink(Event::new(
                            "warning",
                            format!("[Diagnosis] 候选下载失败：{error:#}"),
                        ));
                        continue;
                    }
                    let candidate_path = path.clone();
                    let token = cancel.clone();
                    let candidate =
                        tokio::task::spawn_blocking(move || metadata(&candidate_path, &token))
                            .await?;
                    if let Ok(candidate) = candidate
                        && candidate_matches(installed, &candidate, target, id, ranges)
                    {
                        let entry = PlanEntry {
                            kind: ResourceKind::Mods,
                            destination: format!("mods/{}", file.name),
                            size: file.size,
                            origin: ResourceOrigin {
                                platform,
                                project: Some(project.id.clone()),
                                version: Some(version.id),
                                name: project.name.clone(),
                                dependencies: version.dependencies,
                            },
                            input: Input::Remote { file: file.clone() },
                            reuse: false,
                            content_sha256: None,
                        };
                        let output = install::staged_path(directory, index, &entry)?;
                        fs::create_dir_all(output.parent().context("无效暂存目录")?)?;
                        fs::rename(&path, output)?;
                        downloader.discard_installer(&spec).await?;
                        return Ok((entry, candidate));
                    }
                    downloader.discard_installer(&spec).await?;
                }
            }
        }
        bail!("未找到可确认兼容的 {id} {ranges:?}；请在管理 Mods 中手动选择。现有文件未改变")
    }
}

fn candidate_matches(
    installed: &[FabricMod],
    candidate: &[FabricMod],
    target: &ResourceTarget,
    id: &str,
    ranges: &[Value],
) -> bool {
    if !satisfies(candidate, id, ranges) {
        return false;
    }
    let mut combined = installed.to_vec();
    combined.extend_from_slice(candidate);
    check_compatibility(&combined, target).is_ok() && missing(&combined).is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{archive, fixture, install, local, sink};
    use super::*;
    use serde_json::json;

    fn item(id: &str, version: &str, depends: Value) -> FabricMod {
        serde_json::from_value(json!({"id":id, "version":version, "depends":depends})).unwrap()
    }
    #[test]
    fn large_nested_jar_is_read_without_the_old_64_mib_limit() {
        let temp = tempfile::tempdir().unwrap();
        let child = temp.path().join("child.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&child).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(br#"{"id":"bundled","version":"1.5","depends":{"library":"*"}}"#)
            .unwrap();
        zip.start_file("payload.bin", options).unwrap();
        std::io::copy(&mut std::io::repeat(0).take(65 * 1024 * 1024), &mut zip).unwrap();
        zip.finish().unwrap();
        let parent = temp.path().join("parent.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&parent).unwrap());
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(br#"{"id":"parent","version":"1","jars":[{"file":"child.jar"}]}"#)
            .unwrap();
        zip.start_file("child.jar", options).unwrap();
        std::io::copy(&mut fs::File::open(&child).unwrap(), &mut zip).unwrap();
        zip.finish().unwrap();
        let mods = metadata(&parent, &CancellationToken::new()).unwrap();
        assert_eq!(
            mods.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["parent", "bundled"]
        );
        assert!(requirements(&mods).contains_key("library"));
        let mut archive = NestedJar::read(
            &mut fs::File::open(&child).unwrap(),
            fs::metadata(&child).unwrap().len(),
            &CancellationToken::new(),
        )
        .unwrap();
        let NestedJar::Disk { path, .. } = &archive else {
            panic!("large archives must stay out of RAM")
        };
        let path = path.clone();
        assert!(path.exists());
        let mut magic = [0; 4];
        archive.reader().read_exact(&mut magic).unwrap();
        assert_eq!(&magic, b"PK\x03\x04");
        drop(archive);
        assert!(!path.exists());
    }
    #[test]
    fn nested_reads_reject_oversize_truncated_and_cancelled_input() {
        let cancel = CancellationToken::new();
        assert!(NestedJar::read(&mut std::io::empty(), NESTED_JAR_LIMIT + 1, &cancel).is_err());
        assert!(NestedJar::read(&mut Cursor::new(b"short"), 10, &cancel).is_err());
        assert!(NestedJar::read(&mut Cursor::new(b"too long"), 2, &cancel).is_err());
        cancel.cancel();
        assert!(
            NestedJar::read(&mut Cursor::new(b"data"), 4, &cancel)
                .err()
                .unwrap()
                .to_string()
                .contains("取消")
        );
    }
    #[test]
    #[ignore = "Read-only metadata check of NEKOCRATE_INSPECT_MOD on the local machine"]
    fn inspect_local_fabric_metadata() {
        let path = PathBuf::from(
            std::env::var_os("NEKOCRATE_INSPECT_MOD").expect("NEKOCRATE_INSPECT_MOD"),
        );
        let mods = if path.is_dir() {
            let target = ResourceTarget {
                instance: "read-only".into(),
                version: "read-only".into(),
                directory: path,
                minecraft: std::env::var("NEKOCRATE_INSPECT_MC").expect("NEKOCRATE_INSPECT_MC"),
                loader: Some(crate::model::ModLoader {
                    kind: "fabric".into(),
                    version: std::env::var("NEKOCRATE_INSPECT_LOADER")
                        .expect("NEKOCRATE_INSPECT_LOADER"),
                }),
                worlds: vec![],
                shared: false,
            };
            let mods = installed(&target, &CancellationToken::new()).unwrap();
            check_compatibility(&mods, &target).unwrap();
            println!("Missing dependencies: {:?}", missing(&mods).unwrap());
            mods
        } else {
            metadata(&path, &CancellationToken::new()).unwrap()
        };
        assert!(!mods.is_empty());
        println!(
            "{} {}: {} Fabric metadata entries",
            mods[0].id,
            mods[0].version,
            mods.len()
        );
    }
    #[tokio::test]
    async fn repair_search_verifies_real_ids_and_reuses_staging_through_installation() {
        use super::super::tests::{cache, cache_project, remote};
        use sha1::{Digest, Sha1};
        let (temp, launcher, id, working) = fixture(true);
        archive(
            &working.join("mods/ali.jar"),
            &[(
                "fabric.mod.json",
                r#"{"id":"ali","version":"2.5.0","depends":{"aci":">=1.5"}}"#,
            )],
        );
        let report = crate::crash::CrashReport {
            version: "1.20.1".into(),
            time: 1,
            exit: "failed".into(),
            sources: vec![],
            file: None,
            repairable: false,
            findings: crate::crash::analyze(
                "模组 'AdvancedLootInfo' (ali) 2.5.0 需要 aci 的 1.5 及以上版本，但没有安装它！",
            ),
        };
        storage::write_json(
            &launcher
                .instance_dir(&id)
                .unwrap()
                .join("launcher-logs/last-crash.json"),
            &report,
        )
        .unwrap();
        cache_project(&launcher, "aci");
        cache(
            &launcher,
            "search",
            &[
                ("query", "aci".into()),
                ("offset", "0".into()),
                ("limit", "20".into()),
                (
                    "facets",
                    json!([
                        ["project_type:mod"],
                        ["versions:1.20.1"],
                        ["categories:fabric"]
                    ])
                    .to_string(),
                ),
            ],
            json!({"hits":[], "total_hits":0}),
        );
        let mut versions = Vec::new();
        let mut cache_paths = Vec::new();
        for (version_id, metadata) in [
            ("wrong", r#"{"id":"other","version":"99"}"#),
            (
                "right",
                r#"{"id":"aci","version":"1.5.1","depends":{"minecraft":"1.20.1"}}"#,
            ),
        ] {
            let jar = temp.path().join(format!("{version_id}.jar"));
            archive(&jar, &[("fabric.mod.json", metadata)]);
            let bytes = fs::read(jar).unwrap();
            let hash = hex::encode(Sha1::digest(&bytes));
            let path = launcher
                .root()
                .join("cache/objects/sha1")
                .join(&hash[..2])
                .join(&hash);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &bytes).unwrap();
            cache_paths.push(path);
            let mut version = remote(version_id, "aci", json!([]));
            version["date_published"] = json!(if version_id == "wrong" {
                "2026-02-01"
            } else {
                "2026-01-01"
            });
            version["files"][0]["size"] = json!(bytes.len());
            version["files"][0]["hashes"]["sha1"] = json!(hash);
            version["files"][0]["url"] = json!(format!(
                "https://nekocrate-fixture.invalid/{version_id}.jar"
            ));
            versions.push(version);
        }
        cache(
            &launcher,
            "project/aci/version",
            &[
                ("game_versions", json!(["1.20.1"]).to_string()),
                ("loaders", json!(["fabric"]).to_string()),
            ],
            json!(versions),
        );
        let plan = launcher
            .prepare_missing_dependencies(&id, "1.20.1", 1, &CancellationToken::new(), &sink())
            .await
            .unwrap();
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].origin.version.as_deref(), Some("right"));
        assert!(
            !working.join("mods/aci.jar").exists(),
            "preview must not publish"
        );
        assert!(
            cache_paths.iter().all(|p| !p.exists()),
            "verified staging replaces download copies"
        );
        install(&launcher, &plan).await.unwrap();
        let installed = metadata(&working.join("mods/aci.jar"), &CancellationToken::new()).unwrap();
        assert_eq!(installed[0].id, "aci");
        assert_eq!(installed[0].version, "1.5.1");
        assert!(working.join("mods/ali.jar").is_file());
        assert!(launcher.resource_plans(&id, "1.20.1").unwrap().is_empty());
        let already_fixed = launcher
            .prepare_missing_dependencies(&id, "1.20.1", 1, &CancellationToken::new(), &sink())
            .await
            .unwrap();
        assert!(already_fixed.entries.is_empty());
        assert!(launcher.resource_plans(&id, "1.20.1").unwrap().is_empty());
    }
    #[test]
    fn fabric_ranges_use_equality_and_fabric_caret_and_include_prereleases() {
        for (range, version, expected) in [
            ("1.5", "1.5.0+fabric", true),
            ("1.5", "1.6", false),
            (">=1.5", "1.4.9", false),
            (">=1.5", "1.5.1+1.20.1", true),
            (">=1.5 <2", "2.0", false),
            ("^0.2", "0.8", true),
            ("~1.5", "1.6", false),
            ("~1", "1.1", false),
            ("1.5.x", "1.5.0-beta", true),
            ("*", "custom", true),
            (">=1.5", "1.6.0-beta", true),
            ("=custom", "custom", true),
        ] {
            assert_eq!(
                matches(&json!(range), version),
                Some(expected),
                "{range} / {version}"
            );
        }
        assert_eq!(matches(&json!(["~1.4", ">=2"]), "2.1"), Some(true));
        assert_eq!(matches(&json!({"invalid":true}), "1.5"), None);
        assert_eq!(matches(&json!(">invalid"), "1.5"), None);
    }
    #[test]
    fn candidates_require_actual_identity_constraints_game_loader_and_no_conflicts() {
        let (_temp, launcher, id, _) = fixture(true);
        let target = launcher.resource_target(&id, "1.20.1").unwrap();
        let installed = vec![item("ali", "2.5.0", json!({"aci": ">=1.5"}))];
        let ranges = vec![json!(">=1.5")];
        for candidate in [
            item("unrelated", "9", json!({})),
            item("aci", "1.4", json!({})),
            item("aci", "1.5", json!({"minecraft": "1.21"})),
            item("aci", "1.5", json!({"fabricloader": ">=0.99"})),
            item("aci", "1.5", json!({"ali": ">=3"})),
        ] {
            assert!(!candidate_matches(
                &installed,
                &[candidate],
                &target,
                "aci",
                &ranges
            ));
        }
        let valid = item(
            "aci",
            "1.5.1",
            json!({"minecraft":"1.20.x", "fabricloader": ">=0.16"}),
        );
        assert!(candidate_matches(
            &installed,
            std::slice::from_ref(&valid),
            &target,
            "aci",
            &ranges
        ));
        let mut conflict = valid;
        conflict.breaks.insert("ali".into(), json!("*"));
        assert!(!candidate_matches(
            &installed,
            &[conflict],
            &target,
            "aci",
            &ranges
        ));
    }
    #[test]
    fn invalid_constraints_are_reported_separately_from_incompatibility() {
        let (_temp, launcher, id, _) = fixture(true);
        let mut target = launcher.resource_target(&id, "1.20.1").unwrap();
        target.minecraft = "1.21.1".into();
        let valid = item(
            "monsters_in_the_closet",
            "1",
            json!({"minecraft":">=1.20.5-"}),
        );
        check_compatibility(&[valid], &target).unwrap();
        let invalid = item("broken", "1", json!({"minecraft":["*", ">invalid"]}));
        assert!(
            check_compatibility(&[invalid], &target)
                .unwrap_err()
                .to_string()
                .contains("约束格式无效")
        );
        let incompatible = item("older", "1", json!({"minecraft":"<1.20.5-"}));
        let error = check_compatibility(&[incompatible], &target)
            .unwrap_err()
            .to_string();
        assert!(error.contains("older 要求 minecraft"));
        assert!(!error.contains("格式无效"));
    }
    #[tokio::test]
    async fn installation_revalidates_repair_and_never_publishes_wrong_or_stale_artifacts() {
        for mode in [
            "valid",
            "wrong_id",
            "old_version",
            "changed_parent",
            "cancelled",
        ] {
            let (temp, launcher, id, working) = fixture(true);
            archive(
                &working.join("mods/ali.jar"),
                &[(
                    "fabric.mod.json",
                    r#"{"id":"ali","version":"2.5.0","depends":{"aci":">=1.5"}}"#,
                )],
            );
            let dependency = temp.path().join("aci.jar");
            let data = match mode {
                "wrong_id" => r#"{"id":"other","version":"1.5"}"#,
                "old_version" => r#"{"id":"aci","version":"1.4"}"#,
                _ => r#"{"id":"aci","version":"1.5","depends":{"minecraft":"1.20.1"}}"#,
            };
            archive(&dependency, &[("fabric.mod.json", data)]);
            let cancel = CancellationToken::new();
            let mut plan = launcher
                .prepare_local_resources(&id, "1.20.1", &[local(&dependency)], None, &cancel)
                .unwrap();
            plan.repair_dependencies = vec!["aci".into()];
            save_plan(&plan).unwrap();
            if mode == "changed_parent" {
                fs::remove_file(working.join("mods/ali.jar")).unwrap();
            }
            let result = if mode == "cancelled" {
                cancel.cancel();
                launcher
                    .install_resources(&id, "1.20.1", &plan.id, &cancel, &sink())
                    .await
            } else {
                install(&launcher, &plan).await
            };
            assert_eq!(result.is_ok(), mode == "valid", "{mode}: {result:?}");
            assert_eq!(
                working.join("mods/aci.jar").exists(),
                mode == "valid",
                "{mode}"
            );
            assert!(dependency.is_file());
            if mode != "changed_parent" {
                assert!(working.join("mods/ali.jar").is_file());
            }
        }
    }
    #[test]
    fn disabled_and_optional_mods_do_not_create_required_dependencies() {
        let (_temp, launcher, id, working) = fixture(true);
        archive(
            &working.join("mods/disabled.jar.disabled"),
            &[(
                "fabric.mod.json",
                r#"{"id":"disabled","version":"1","depends":{"missing":"*"}}"#,
            )],
        );
        archive(
            &working.join("mods/client.jar"),
            &[(
                "fabric.mod.json",
                r#"{"id":"client","version":"1","recommends":{"optional":"*"}}"#,
            )],
        );
        archive(
            &working.join("mods/server.jar"),
            &[(
                "fabric.mod.json",
                r#"{"id":"server","version":"1","environment":"server","depends":{"server_only":"*"}}"#,
            )],
        );
        let target = launcher.resource_target(&id, "1.20.1").unwrap();
        let mods = installed(&target, &CancellationToken::new()).unwrap();
        assert_eq!(mods.len(), 1);
        assert!(missing(&mods).unwrap().is_empty());
    }
    #[tokio::test]
    async fn stale_or_unproven_report_cannot_start_network_lookup() {
        let (_temp, launcher, id, working) = fixture(true);
        let report = crate::crash::CrashReport {
            version: "1.20.1".into(),
            time: 1,
            exit: "failed".into(),
            sources: vec![],
            file: None,
            repairable: false,
            findings: crate::crash::analyze(
                "模组 'AdvancedLootInfo' (ali) 2.5.0 需要 aci 的 1.5 及以上版本，但没有安装它！",
            ),
        };
        storage::write_json(
            &launcher
                .instance_dir(&id)
                .unwrap()
                .join("launcher-logs/last-crash.json"),
            &report,
        )
        .unwrap();
        for time in [1, 2] {
            let error = launcher
                .prepare_missing_dependencies(
                    &id,
                    "1.20.1",
                    time,
                    &CancellationToken::new(),
                    &sink(),
                )
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(if time == 1 { "未声明" } else { "替换" }),
                "{error:#}"
            );
        }
        assert!(!working.join(".nekocrate/resource-tasks").exists());
    }
}
