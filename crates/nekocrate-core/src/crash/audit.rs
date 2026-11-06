//! Post-failure, read-only local audit. A pack manifest describes intended files,
//! not the complete Mod dependency graph, and extras are not inherently broken.
use super::Finding;
use crate::{model::ModLoader, pack_content, storage, version::ResolvedVersion};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[path = "audit_inventory.rs"]
mod inventory;
use inventory::{Inventory, runtime_components};

#[derive(Default)]
struct Findings(BTreeMap<&'static str, Vec<String>>);
impl Findings {
    fn add(&mut self, code: &'static str, text: impl Into<String>) {
        let values = self.0.entry(code).or_default();
        let text = crate::diagnostics::redact(&text.into());
        if !values.contains(&text) {
            values.push(text);
        }
    }
    fn finish(self) -> Vec<Finding> {
        self.0
            .into_iter()
            .map(|(code, mut evidence)| {
                let (title, advice) = match code {
                    "pack_disabled" => (
                        "整合包必需组件已被禁用",
                        "在 Mods 管理中重新启用列出的文件，再启动游戏。",
                    ),
                    "pack_missing" => (
                        "整合包清单中的必需文件缺失",
                        "按整合包清单补齐对应文件；不要用同名的任意版本替代。",
                    ),
                    "pack_changed" => (
                        "本地文件与整合包声明不同",
                        "核对是否主动更新过这些文件；差异本身不能证明它是崩溃原因。",
                    ),
                    "pack_extra" => (
                        "发现整合包清单之外的 Mod",
                        "这些可能是自行添加的 Mod 或整合包覆盖文件，不据此认定冲突或删除。",
                    ),
                    "component_versions" => (
                        "组件版本与声明的要求不匹配",
                        "根据下列要求核对游戏、加载器和 Mod 版本。",
                    ),
                    "component_dependencies" => (
                        "发现缺失、禁用或冲突的 Mod 依赖",
                        "在 Mods 管理中启用所需前置，或安装满足要求的版本。",
                    ),
                    "component_duplicate" => (
                        "多个启用文件声明了相同 Mod ID",
                        "核对重复文件，保留适用于当前整合包的版本。",
                    ),
                    "component_corrupt" => (
                        "Mod 文件无法作为 JAR 读取",
                        "重新下载列出的 Mod 文件，并保留整合包要求的版本。",
                    ),
                    "files" => (
                        "启动清单中的游戏文件或运行库缺失",
                        "校验并补齐所列文件；外部加载器可使用原安装方式修复。",
                    ),
                    _ => (
                        "部分组件信息未能核对",
                        "这些项目保留为未知，不会因此判定版本不兼容或自动修改文件。",
                    ),
                };
                if evidence.len() > 32 {
                    let omitted = evidence.len() - 32;
                    evidence.truncate(32);
                    evidence.push(format!("另有 {omitted} 项"));
                }
                for line in &mut evidence {
                    *line = line.chars().take(800).collect();
                }
                Finding {
                    code: code.into(),
                    title: title.into(),
                    advice: advice.into(),
                    evidence,
                }
            })
            .collect()
    }
}

fn disabled_name(path: &str) -> &str {
    path.strip_suffix(".disabled")
        .or_else(|| path.strip_suffix(".off"))
        .unwrap_or(path)
}
fn regular(root: &Path, relative: &str) -> Option<fs::Metadata> {
    let mut path = root.to_owned();
    for part in storage::safe_relative(relative).ok()?.components() {
        path.push(part);
        if fs::symlink_metadata(&path).ok()?.file_type().is_symlink() {
            return None;
        }
    }
    fs::metadata(path).ok().filter(|m| m.is_file())
}

fn pack_files(game: &Path, id: &str, working: &Path, inventory: &Inventory, out: &mut Findings) {
    let declarations = match pack_content::load(game, id, working) {
        Ok(files) => files,
        Err(e) => {
            out.add("audit_incomplete", format!("整合包清单：{e:#}"));
            return;
        }
    };
    if declarations.is_empty() {
        return;
    }
    let mut expected = BTreeSet::new();
    let mut unresolved = 0;
    let records = crate::resources::records(working).unwrap_or_default();
    for file in declarations {
        if !file.selected {
            continue;
        }
        if let Some(name) = &file.file_name {
            expected.insert(name.clone());
        }
        if let Some(path) = &file.path {
            expected.insert(path.rsplit('/').next().unwrap_or(path).into());
        }
        if file.optional {
            continue;
        }
        let candidates: Vec<_> = if let Some(path) = &file.path {
            vec![path.clone()]
        } else if let Some(name) = &file.file_name {
            if storage::safe_relative(name).is_err() || name.contains(['/', '\\']) {
                unresolved += 1;
                continue;
            }
            vec![
                format!("mods/{name}"),
                format!("resourcepacks/{name}"),
                format!("shaderpacks/{name}"),
                format!("datapacks/{name}"),
            ]
        } else {
            unresolved += 1;
            continue;
        };
        let active = candidates
            .iter()
            .find_map(|p| regular(working, p).map(|m| (p, m)));
        if let Some((path, metadata)) = active {
            if file.size > 0 && metadata.len() != file.size {
                out.add(
                    "pack_changed",
                    format!(
                        "{path}：清单 {} 字节，本地 {} 字节",
                        file.size,
                        metadata.len()
                    ),
                );
            }
            if let Some(record) = records.get(path)
                && let (Some(project), Some(version)) = (file.project, file.file)
                && record.origin.platform == crate::resources::Platform::Curseforge
                && record.origin.project.as_deref() == Some(project.to_string().as_str())
                && record
                    .origin
                    .version
                    .as_ref()
                    .is_some_and(|v| v != &version.to_string())
            {
                out.add(
                    "pack_changed",
                    format!(
                        "{path}：CurseForge 清单文件 #{version}，本地安装记录 #{}",
                        record.origin.version.as_deref().unwrap()
                    ),
                );
            }
            continue;
        }
        let disabled = candidates.iter().find_map(|p| {
            [".disabled", ".off"]
                .into_iter()
                .map(|suffix| format!("{p}{suffix}"))
                .find(|p| regular(working, p).is_some())
        });
        if let Some(path) = disabled {
            let ids: Vec<_> = inventory
                .files
                .iter()
                .filter(|f| f.path == path)
                .flat_map(|f| f.components.iter().map(|c| c.id.as_str()))
                .collect();
            out.add(
                "pack_disabled",
                if ids.is_empty() {
                    path
                } else {
                    format!("{path} · {}", ids.join(", "))
                },
            );
        } else {
            out.add(
                "pack_missing",
                file.path.or(file.file_name).unwrap_or(file.name),
            );
        }
    }
    if unresolved > 0 {
        out.add(
            "audit_incomplete",
            format!("{unresolved} 项清单只有项目/文件 ID，缺少本地文件名映射；未把它们算作缺失"),
        );
    } else {
        for file in inventory.files.iter().filter(|f| f.enabled) {
            let name = disabled_name(&file.path).rsplit('/').next().unwrap();
            if !expected.contains(name) {
                out.add("pack_extra", &file.path);
            }
        }
    }
}

fn pack_versions(
    game: &Path,
    id: &str,
    working: &Path,
    minecraft: &str,
    loader: Option<&ModLoader>,
    out: &mut Findings,
) {
    let mut seen = BTreeSet::new();
    for directory in [game.join("versions").join(id), working.to_owned()] {
        for name in [
            "manifest.json",
            "modrinth.index.json",
            "modpack.cfg",
            "mcbbs.packmeta",
        ] {
            let path = directory.join(name);
            if !seen.insert(path.clone()) || !path.exists() {
                continue;
            }
            let value = match pack_content::read_manifest(&path) {
                Ok(value) => value,
                Err(e) => {
                    out.add("audit_incomplete", format!("{name}：{e:#}"));
                    continue;
                }
            };
            let value = value.get("manifest").unwrap_or(&value);
            let expected = value["minecraft"]["version"]
                .as_str()
                .or_else(|| value["dependencies"]["minecraft"].as_str());
            if let Some(expected) = expected
                && expected != minecraft
            {
                out.add(
                    "component_versions",
                    format!("{name}：游戏版本声明 {expected}，当前 {minecraft}"),
                );
            }
            let mut loaders = Vec::new();
            for (key, kind) in [
                ("fabric-loader", "fabric"),
                ("forge", "forge"),
                ("neoforge", "neoforge"),
                ("quilt-loader", "quilt"),
            ] {
                if let Some(v) = value["dependencies"][key].as_str() {
                    loaders.push((kind, v));
                }
            }
            for entry in value["minecraft"]["modLoaders"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if entry["primary"] == false {
                    continue;
                }
                if let Some((kind, version)) = entry["id"].as_str().and_then(|s| s.split_once('-'))
                {
                    loaders.push((kind, version));
                }
            }
            for (kind, version) in loaders {
                if !loader.is_some_and(|l| l.kind == kind && l.version == version) {
                    out.add(
                        "component_versions",
                        format!(
                            "{name}：加载器声明 {kind} {version}，当前 {}",
                            loader
                                .map(|l| format!("{} {}", l.kind, l.version))
                                .unwrap_or_else(|| "原版".into())
                        ),
                    );
                }
            }
        }
    }
}

// Numeric Maven ranges are checked without borrowing semver's different rules.
// Non-numeric qualifiers stay unknown; never treat an unparsed range as false.
fn numeric(value: &str) -> Option<Vec<u64>> {
    value
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        })
        .collect()
}
fn order(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|o| !o.is_eq())
        .unwrap_or(std::cmp::Ordering::Equal)
}
fn maven_matches(range: &str, actual: &str) -> Option<bool> {
    let mut remaining = range.trim();
    if remaining.is_empty() || remaining == "*" {
        return Some(true);
    }
    if !remaining.starts_with(['[', '(']) {
        // A bare Maven version is recommended, not a hard restriction.
        return (!remaining.contains(['[', ']', '(', ')', ','])).then_some(true);
    }
    let actual = numeric(actual)?;
    let mut matched = false;
    let mut previous_upper: Option<Vec<u64>> = None;
    while !remaining.is_empty() {
        if !remaining.starts_with(['[', '(']) {
            return None;
        }
        let end = remaining.find([']', ')'])?;
        let inclusive_low = remaining.starts_with('[');
        let inclusive_high = remaining.as_bytes()[end] == b']';
        let body = &remaining[1..end];
        let (low, high) = if let Some((a, b)) = body.split_once(',') {
            (a.trim(), b.trim())
        } else if inclusive_low && inclusive_high {
            (body.trim(), body.trim())
        } else {
            return None;
        };
        let lower = if low.is_empty() {
            None
        } else {
            Some(numeric(low)?)
        };
        let upper = if high.is_empty() {
            None
        } else {
            Some(numeric(high)?)
        };
        if low == high && body.contains(',') {
            return None;
        }
        if let (Some(a), Some(b)) = (&lower, &upper)
            && order(a, b).is_gt()
        {
            return None;
        }
        if let Some(previous) = &previous_upper
            && lower.as_ref().is_none_or(|v| order(v, previous).is_lt())
        {
            return None;
        }
        matched |= lower.as_ref().is_none_or(|v| {
            if inclusive_low {
                !order(&actual, v).is_lt()
            } else {
                order(&actual, v).is_gt()
            }
        }) && upper.as_ref().is_none_or(|v| {
            if inclusive_high {
                !order(&actual, v).is_gt()
            } else {
                order(&actual, v).is_lt()
            }
        });
        previous_upper = upper;
        remaining = remaining[end + 1..].trim();
        if let Some(rest) = remaining.strip_prefix(',') {
            remaining = rest.trim();
        }
    }
    Some(matched)
}

fn dependencies(
    inventory: &Inventory,
    minecraft: &str,
    loader: Option<&ModLoader>,
    runtime: &BTreeMap<String, String>,
    text: &str,
    out: &mut Findings,
) {
    let mut versions: BTreeMap<String, Vec<String>> = runtime
        .iter()
        .map(|(k, v)| (k.clone(), vec![v.clone()]))
        .collect();
    versions.insert("minecraft".into(), vec![minecraft.into()]);
    if let Some(loader) = loader {
        versions.insert(
            if loader.kind == "fabric" {
                "fabricloader"
            } else {
                &loader.kind
            }
            .into(),
            vec![loader.version.clone()],
        );
    }
    let mut loaded_versions = versions.clone();
    let mut roots: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut disabled: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for file in &inventory.files {
        for c in &file.components {
            if !file.enabled {
                disabled.entry(&c.id).or_default().push(&file.path);
                continue;
            }
            if !c.nested {
                roots.entry(&c.id).or_default().insert(&file.path);
            }
            for id in std::iter::once(&c.id).chain(c.aliases.iter()) {
                let values = versions.entry(id.clone()).or_default();
                if let Some(version) = &c.version
                    && !values.contains(version)
                {
                    values.push(version.clone());
                }
                if !c.nested {
                    let loaded = loaded_versions.entry(id.clone()).or_default();
                    if let Some(version) = &c.version
                        && !loaded.contains(version)
                    {
                        loaded.push(version.clone());
                    }
                }
            }
        }
    }
    for (id, files) in roots {
        if files.len() > 1 {
            out.add(
                "component_duplicate",
                format!("{id}：{}", files.into_iter().collect::<Vec<_>>().join(", ")),
            );
        }
    }
    for (id, files) in &disabled {
        if !versions.contains_key(*id)
            && text.contains(&format!("{id}:"))
            && text.lines().any(|line| {
                line.contains(&format!("{id}:"))
                    && (line.contains("unknown string") || line.contains("Unknown item"))
            })
        {
            out.add(
                "pack_disabled",
                format!(
                    "异常引用 {id}:…，但提供该内容的 Mod 已禁用：{}",
                    files.join(", ")
                ),
            );
        }
    }
    for file in inventory.files.iter().filter(|f| f.enabled) {
        for c in &file.components {
            // A bundled Fabric jar is a candidate, not necessarily selected by
            // the loader. It may satisfy a root dependency, but its own bounds
            // cannot establish a failure without evidence it actually loaded.
            if c.nested && runtime.get(&c.id) != c.version.as_ref() {
                continue;
            }
            for dep in &c.dependencies {
                // Java is already assessed by Java selection. FML language versions
                // are not NeoForge distribution versions and must not be conflated.
                if dep.id == "java" || dep.id == "@fml" {
                    continue;
                }
                let providers = if dep.conflict {
                    &loaded_versions
                } else {
                    &versions
                };
                let Some(actual) = providers.get(&dep.id) else {
                    if !dep.conflict {
                        if let Some(files) = disabled.get(dep.id.as_str()) {
                            out.add(
                                "component_dependencies",
                                format!(
                                    "{} 需要 {} {}；对应文件已禁用：{}",
                                    c.id,
                                    dep.id,
                                    dep.range,
                                    files.join(", ")
                                ),
                            );
                        } else if inventory.identities_complete {
                            out.add(
                                "component_dependencies",
                                format!("{} 需要 {} {}，未找到", c.id, dep.id, dep.range),
                            );
                        } else {
                            out.add(
                                "audit_incomplete",
                                format!(
                                    "{} 的依赖 {} 无法确认：部分旧版注解或嵌套元数据未解析",
                                    c.id, dep.id
                                ),
                            );
                        }
                    }
                    continue;
                };
                let results: Vec<_> = actual
                    .iter()
                    .map(|v| {
                        if dep.fabric {
                            crate::fabric_version::matches_json(&dep.range, v)
                        } else {
                            dep.range.as_str().and_then(|r| maven_matches(r, v))
                        }
                    })
                    .collect();
                let accepts = results.contains(&Some(true));
                if dep.conflict && accepts
                    || !dep.conflict
                        && !accepts
                        && !results.is_empty()
                        && results.iter().all(Option::is_some)
                {
                    out.add(
                        "component_versions",
                        format!(
                            "{} {} {} {}；本地 {}",
                            c.id,
                            if dep.conflict { "不兼容" } else { "需要" },
                            dep.id,
                            dep.range,
                            actual.join(", ")
                        ),
                    );
                } else if !accepts && (results.is_empty() || results.iter().any(Option::is_none)) {
                    out.add(
                        "audit_incomplete",
                        format!(
                            "{} → {} {}：无法可靠比较本地版本 {}",
                            c.id,
                            dep.id,
                            dep.range,
                            actual.join(", ")
                        ),
                    );
                }
            }
        }
    }
}

pub(super) fn inspect(
    root: &Path,
    game: &Path,
    id: &str,
    working: &Path,
    text: &str,
) -> Vec<Finding> {
    let mut out = Findings::default();
    let resolved = crate::version::resolve(root, game, id);
    let (minecraft, loader) = resolved
        .as_ref()
        .map(|r| (r.minecraft_version.as_str(), r.loader.as_ref()))
        .unwrap_or((id, None));
    let runtime = runtime_components(text);
    let inventory = inventory::scan(
        working,
        minecraft,
        loader.map_or("", |l| l.kind.as_str()),
        &runtime,
    );
    pack_files(game, id, working, &inventory, &mut out);
    if let Ok(resolved) = &resolved {
        pack_versions(game, id, working, minecraft, loader, &mut out);
        runtime_files(game, resolved, &mut out);
    } else {
        out.add(
            "audit_incomplete",
            format!("版本配置：{:#}", resolved.as_ref().unwrap_err()),
        );
    }
    if working
        .join("config/fabric_loader_dependencies.json")
        .exists()
        && loader.is_some_and(|l| l.kind == "fabric")
    {
        out.add(
            "audit_incomplete",
            "存在 Fabric 依赖覆盖配置，静态依赖检查交由加载器处理",
        );
    } else {
        dependencies(&inventory, minecraft, loader, &runtime, text, &mut out);
    }
    for issue in &inventory.incomplete {
        out.add("audit_incomplete", issue);
    }
    for issue in &inventory.corrupt {
        out.add("component_corrupt", issue);
    }
    out.finish()
}

fn runtime_files(game: &Path, resolved: &ResolvedVersion, out: &mut Findings) {
    let v = &resolved.version;
    let mut files = Vec::new();
    if let Ok(path) = crate::version::client_path(game, v) {
        files.push(path);
    }
    let arch = crate::java::JavaRequirement::for_version(v).arch;
    for library in v
        .libraries
        .iter()
        .filter(|l| crate::minecraft::is_allowed_on(l, arch))
    {
        for artifact in [
            crate::minecraft::classpath_artifact(library),
            crate::minecraft::native_artifact(library),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(path) = crate::minecraft::library_path(game, artifact) {
                files.push(path);
            }
        }
    }
    for path in files {
        if !path.is_file() {
            out.add(
                "files",
                path.strip_prefix(game).unwrap_or(&path).to_string_lossy(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn archive(path: &Path, descriptor: &str, body: &str) {
        archive_entries(path, &[(descriptor, body.as_bytes())]);
    }
    fn archive_entries(path: &Path, entries: &[(&str, &[u8])]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    fn profile(game: &Path, kind: &str) -> std::path::PathBuf {
        let mut v = crate::test_http::version_fixture("Pack");
        let coordinate = if kind == "fabric" {
            "net.fabricmc:fabric-loader:0.20.1"
        } else if kind == "forge_modern" {
            "net.minecraftforge:forge:1.20.1-47.4.4"
        } else {
            "net.minecraftforge:forge:1.12.2-14.23.5.2860"
        };
        v.libraries
            .push(serde_json::from_value(json!({"name":coordinate})).unwrap());
        // Explicit game metadata, independent of the profile's custom name.
        let mut value = serde_json::to_value(v).unwrap();
        value["patches"] = json!([{"id":"game","version":if kind == "fabric" {"1.21.1"} else if kind == "forge_modern" {"1.20.1"} else {"1.12.2"}}]);
        storage::write_json(
            &crate::version::metadata_path(game, "Pack").unwrap(),
            &value,
        )
        .unwrap();
        game.join("versions/Pack")
    }
    fn evidence(findings: &[Finding], code: &str) -> String {
        findings
            .iter()
            .filter(|f| f.code == code)
            .flat_map(|f| f.evidence.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn disabled_required_mod_is_correlated_with_registry_failure_without_changing_files() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "forge");
        storage::write_json(&working.join("manifest.json"),&json!({"manifestType":"minecraftModpack","minecraft":{"version":"1.12.2","modLoaders":[{"id":"forge-14.23.5.2860","primary":true}]},"files":[
            {"projectID":1,"fileID":2,"fileName":"varied.jar","required":true},
            {"projectID":3,"fileID":4,"fileName":"missing.jar","required":true},
            {"projectID":5,"fileID":6,"fileName":"optional.jar","required":false}
        ]})).unwrap();
        archive(
            &working.join("mods/varied.jar.disabled"),
            "mcmod.info",
            r#"[{"modid":"variedcommodities","version":"${version}","mcversion":"1.11"}]"#,
        );
        archive(
            &working.join("mods/user.jar"),
            "mcmod.info",
            r#"[{"modid":"extra","mcversion":"1.10"}]"#,
        );
        let before = fs::read(working.join("mods/varied.jar.disabled")).unwrap();
        let error = "Caused by: com.google.gson.JsonSyntaxException: Expected name to be an item, was unknown string 'variedcommodities:coin_diamond'";
        assert_eq!(super::super::analyze(error)[0].code, "registry");
        let findings = inspect(game, game, "Pack", &working, error);
        assert!(evidence(&findings, "pack_disabled").contains("variedcommodities"));
        assert!(evidence(&findings, "pack_missing").contains("missing.jar"));
        assert!(!evidence(&findings, "pack_missing").contains("optional.jar"));
        assert!(evidence(&findings, "pack_extra").contains("user.jar"));
        assert!(
            evidence(&findings, "component_versions").is_empty(),
            "legacy mcversion is not a constraint"
        );
        assert_eq!(
            fs::read(working.join("mods/varied.jar.disabled")).unwrap(),
            before
        );
        assert!(!working.join("mods/varied.jar").exists());
        let log = game.join("launcher.log");
        fs::write(&log, "").unwrap();
        let report = super::super::CrashReport {
            version: "Pack".into(),
            time: 1,
            exit: "failed".into(),
            sources: vec![],
            findings,
            file: None,
            repairable: false,
        };
        let sink: crate::model::EventSink = std::sync::Arc::new(|_| {});
        super::super::write_to_log(&report, &log, "instance", &sink).unwrap();
        let log = fs::read_to_string(log).unwrap();
        assert!(log.contains("[INFO] [Diagnosis] mods/user.jar"));
        assert!(!log.contains("[ERROR] [Diagnosis] mods/user.jar"));
    }
    #[test]
    fn failed_launch_persists_manifest_audit_and_emits_it_in_the_log() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = crate::Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Pack", "Pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let working = profile(&game, "forge");
        storage::write_json(
            &working.join("manifest.json"),
            &json!({
                "manifestType":"minecraftModpack", "files":[
                    {"projectID":1,"fileID":2,"fileName":"required.jar","required":true}
                ]
            }),
        )
        .unwrap();
        archive(
            &working.join("mods/required.jar.disabled"),
            "mcmod.info",
            r#"[{"modid":"required"}]"#,
        );
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events = captured.clone();
        let sink: crate::model::EventSink =
            std::sync::Arc::new(move |event| events.lock().unwrap().push(event));
        launcher
            .report_launch_failure(&instance.id, "Unknown launch failure", &sink)
            .unwrap();
        let saved = launcher.crash_report(&instance.id).unwrap().unwrap();
        assert!(evidence(&saved.findings, "pack_disabled").contains("required.jar.disabled"));
        let path = launcher
            .crash_report_path(&instance.id, "Pack", saved.time)
            .unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("Unknown launch failure"));
        assert!(text.contains("[ERROR] [Diagnosis] 整合包必需组件已被禁用"));
        let events = captured.lock().unwrap();
        let report = events.last().unwrap().crash.as_ref().unwrap();
        assert!(evidence(&report.findings, "pack_disabled").contains("required.jar.disabled"));
    }
    #[test]
    fn fabric_uses_real_predicates_and_ignores_disabled_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "fabric");
        archive(
            &working.join("mods/main.jar"),
            "fabric.mod.json",
            r#"{"id":"main","version":"1","depends":{"minecraft":">=1.20.5-","lib":">=2","absent":"*"}}"#,
        );
        archive(
            &working.join("mods/lib.jar"),
            "fabric.mod.json",
            r#"{"id":"lib","version":"1"}"#,
        );
        archive(
            &working.join("mods/lib-old.jar.disabled"),
            "fabric.mod.json",
            r#"{"id":"lib","version":"0"}"#,
        );
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_dependencies").contains("absent"));
        assert!(evidence(&findings, "component_versions").contains("lib"));
        assert!(!evidence(&findings, "component_versions").contains("minecraft"));
        assert!(evidence(&findings, "component_duplicate").is_empty());
        archive(
            &working.join("mods/lib-new.jar"),
            "fabric.mod.json",
            r#"{"id":"lib","version":"2"}"#,
        );
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_duplicate").contains("lib-new.jar"));
        assert!(evidence(&findings, "component_versions").is_empty());
        storage::write_json(
            &working.join("config/fabric_loader_dependencies.json"),
            &json!({"version":1,"overrides":{}}),
        )
        .unwrap();
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_dependencies").is_empty());
        assert!(evidence(&findings, "audit_incomplete").contains("覆盖"));
    }
    #[test]
    fn legacy_forge_ignores_bundled_modern_descriptors_and_uses_runtime_versions() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "forge");
        archive_entries(&working.join("mods/dual.jar"), &[
            ("mcmod.info", br#"[{"modid":"legacy","version":"${version}","mcversion":"1.11","useDependencyInformation":true,"requiredMods":["lib@[1,2)"]}]"#),
            ("META-INF/mods.toml", b"modLoader='javafml'\nloaderVersion='[40,)'\n[[mods]]\nmodId='modern'\nversion='1'\n[[dependencies.modern]]\nmodId='minecraft'\nmandatory=true\nversionRange='[1.18,)'\n"),
        ]);
        archive(
            &working.join("mods/lib.jar"),
            "mcmod.info",
            r#"[{"modid":"lib","version":"3","mcversion":"1.10"}]"#,
        );
        let runtime =
            "| LCHI | legacy | 1 | dual.jar | None |\n| LCHI | lib | 1.5 | lib.jar | None |";
        let findings = inspect(game, game, "Pack", &working, runtime);
        assert!(evidence(&findings, "component_versions").is_empty());
        assert!(evidence(&findings, "component_dependencies").is_empty());
        let findings = inspect(game, game, "Pack", &working, &runtime.replace("1.5", "3"));
        let mismatches = evidence(&findings, "component_versions");
        assert!(mismatches.contains("legacy 需要 lib"));
        assert!(!mismatches.contains("modern"));
    }
    #[test]
    fn fabric_nested_candidates_are_providers_but_not_assumed_loaded() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "fabric");
        let nested = game.join("candidate.jar");
        archive(
            &nested,
            "fabric.mod.json",
            r#"{"id":"nested","version":"1","depends":{"minecraft":"<1.20","absent":"*"}}"#,
        );
        archive_entries(&working.join("mods/root.jar"), &[
            ("fabric.mod.json", br#"{"id":"root","version":"1","depends":{"nested":"*"},"breaks":{"nested":"*"},"jars":[{"file":"nested.jar"}]}"#),
            ("nested.jar", &fs::read(nested).unwrap()),
        ]);
        archive(
            &working.join("mods/forge-only.jar"),
            "mcmod.info",
            r#"[{"modid":"forgeonly","useDependencyInformation":true,"requiredMods":["absent"]}]"#,
        );
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_dependencies").is_empty());
        assert!(evidence(&findings, "component_versions").is_empty());
        let findings = inspect(
            game,
            game,
            "Pack",
            &working,
            "| LCHI | nested | 1 | nested.jar | None |",
        );
        assert!(evidence(&findings, "component_versions").contains("nested 需要 minecraft"));
        assert!(evidence(&findings, "component_versions").contains("root 不兼容 nested"));
    }
    #[test]
    fn manifest_versions_unknown_ids_and_current_platform_libraries_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "forge");
        let metadata = crate::version::metadata_path(game, "Pack").unwrap();
        let mut v: serde_json::Value = storage::read_json(&metadata).unwrap();
        v["libraries"].as_array_mut().unwrap().push(json!({
            "name":"test:foreign:1", "rules":[{"action":"allow","os":{"name":"unsupported-test-os"}}],
            "downloads":{"artifact":{"path":"test/foreign/1/foreign.jar","url":"https://invalid.example/foreign.jar","size":1,"sha1":""}}
        }));
        storage::write_json(&metadata, &v).unwrap();
        storage::write_json(&working.join("modrinth.index.json"),&json!({"game":"minecraft","dependencies":{"minecraft":"1.20.1","fabric-loader":"0.16.0"},"files":[{"path":"mods/missing.jar","env":{"client":"required"}},{"path":"mods/server.jar","env":{"client":"unsupported"}}]})).unwrap();
        storage::write_json(&working.join("manifest.json"),&json!({"manifestType":"minecraftModpack","files":[{"projectID":1,"fileID":2,"required":true}]})).unwrap();
        archive(&working.join("mods/user.jar"), "mcmod.info", "[]");
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_versions").contains("1.20.1"));
        assert!(evidence(&findings, "component_versions").contains("fabric"));
        assert!(evidence(&findings, "pack_missing").contains("mods/missing.jar"));
        assert!(!evidence(&findings, "pack_missing").contains("server.jar"));
        assert!(!evidence(&findings, "pack_missing").contains("CurseForge"));
        assert!(evidence(&findings, "audit_incomplete").contains("缺少本地文件名映射"));
        assert!(evidence(&findings, "pack_extra").is_empty());
        assert!(evidence(&findings, "files").contains("versions/Pack/Pack.jar"));
        assert!(!evidence(&findings, "files").contains("foreign.jar"));
    }
    #[test]
    fn modern_forge_only_checks_client_required_dependencies_and_preserves_unknown_versions() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        let working = profile(game, "forge_modern");
        archive(
            &working.join("mods/main.jar"),
            "META-INF/mods.toml",
            r#"
modLoader="javafml"
loaderVersion="[14,)"
[[mods]]
modId="main"
version="1"
[[dependencies.main]]
modId="lib"
mandatory=true
versionRange="[2,)"
[[dependencies.main]]
modId="server"
mandatory=true
side="SERVER"
versionRange="[1,)"
[[dependencies.main]]
modId="optional"
mandatory=false
[[dependencies.main]]
modId="missing"
mandatory=true
versionRange="[1,)"
"#,
        );
        archive(
            &working.join("mods/lib.jar"),
            "META-INF/mods.toml",
            "[[mods]]\nmodId='lib'\nversion='1'\n",
        );
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_versions").contains("lib"));
        let missing = evidence(&findings, "component_dependencies");
        assert!(missing.contains("missing"));
        assert!(!missing.contains("server"));
        assert!(!missing.contains("optional"));
        archive(
            &working.join("mods/lib.jar"),
            "META-INF/mods.toml",
            "[[mods]]\nmodId='lib'\nversion='${file.jarVersion}'\n",
        );
        let findings = inspect(game, game, "Pack", &working, "");
        assert!(evidence(&findings, "component_versions").is_empty());
        assert!(evidence(&findings, "audit_incomplete").contains("lib"));
    }
    #[test]
    fn maven_numeric_ranges_do_not_use_cargo_or_fabric_semantics() {
        for (range, value, result) in [
            ("[1,2)", "1.12.2", true),
            ("[1,2)", "2", false),
            ("(,1],[2,)", "1.5", false),
            ("[1.2]", "1.2.0", true),
            ("1.2", "9", true),
            ("[47,)", "47.4.4", true),
            ("[14.23.5.2860,)", "14.23.5.2859", false),
        ] {
            assert_eq!(maven_matches(range, value), Some(result), "{range} {value}");
        }
        for range in ["[2,1]", "[1,1)", "[1,2],broken", "[1,2],[1,3]"] {
            assert_eq!(maven_matches(range, "1"), None, "{range}");
        }
        assert_eq!(maven_matches("[1.0-beta,)", "1.0"), None);
        assert_eq!(maven_matches("[1,)", "${version}"), None);
    }
    #[test]
    fn numeric_maven_cases_match_forge_loader() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/maven-numeric-predicates.json"
        ))
        .unwrap();
        for row in data["predicates"].as_array().unwrap() {
            let range = row[0].as_str().unwrap();
            for (version, result) in data["versions"]
                .as_array()
                .unwrap()
                .iter()
                .zip(row[1].as_str().unwrap().bytes())
            {
                let expected = match result {
                    b'T' => Some(true),
                    b'F' => Some(false),
                    _ => None,
                };
                assert_eq!(
                    maven_matches(range, version.as_str().unwrap()),
                    expected,
                    "{range} {version}"
                );
            }
        }
    }
    #[test]
    #[ignore = "Read-only replay of NEKOCRATE_AUDIT_GAME / NEKOCRATE_AUDIT_VERSION"]
    fn replay_local_crash_audit() {
        let game = std::path::PathBuf::from(std::env::var_os("NEKOCRATE_AUDIT_GAME").unwrap());
        let id = std::env::var("NEKOCRATE_AUDIT_VERSION").unwrap();
        storage::safe_version(&id).unwrap();
        let working = game.join("versions").join(&id);
        let text =
            super::super::tail(&working.join(std::env::var("NEKOCRATE_AUDIT_REPORT").unwrap()))
                .unwrap();
        let root = std::env::var_os("NEKOCRATE_AUDIT_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| game.clone());
        let start = std::time::Instant::now();
        let findings = inspect(&root, &game, &id, &working, &text);
        for finding in findings {
            println!(
                "{}: {}\n{}",
                finding.code,
                finding.title,
                finding.evidence.join("\n")
            );
        }
        println!("Local audit elapsed: {:?}", start.elapsed());
    }
}
