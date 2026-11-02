//! Hard incompatibilities and recommendations are separate. Missing mod metadata
//! is not an upper bound on the JVM; known loader and mod constraints still apply.
use crate::{model::Version, version};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

#[derive(Clone, Debug)]
pub struct Compatibility {
    pub majors: Vec<u32>,
    pub reasons: Vec<String>,
    pub requires_jdk: bool,
    pub fingerprint: String,
    constraints: Vec<(String, Constraint)>,
}

impl Compatibility {
    pub fn rejection(&self, major: u32, version: &str) -> Option<String> {
        if !self.majors.contains(&major) {
            return Some(format!("Java {major} 不在游戏与加载器的兼容范围内"));
        }
        let number = java_version_number(version)?;
        self.constraints.iter().find_map(|(source, constraint)| {
            (constraint.allows_version(number) == Some(false)).then(|| {
                format!(
                    "{source} 的 Java 约束{} {}",
                    if constraint.deny { "排除" } else { "要求" },
                    constraint.ranges.join(" 或 ")
                )
            })
        })
    }
}

pub(crate) fn bytecode_ceiling(v: &Version) -> Option<(u32, String)> {
    if v.main_class == "net.minecraft.launchwrapper.Launch" {
        return Some((8, "LaunchWrapper".into()));
    }
    if v.main_class == "net.minecraft.client.main.Main" {
        return None;
    }
    v.libraries
        .iter()
        .filter_map(|library| {
            let asm = library.name.strip_prefix("org.ow2.asm:asm:")?;
            Some((asm_java_ceiling(asm)?, format!("ASM {asm}")))
        })
        .min_by_key(|(maximum, _)| *maximum)
}

pub fn assess(v: &Version, game: &Path) -> Result<Compatibility> {
    assess_in_directory(v, &version::working_directory(game, v)?)
}
pub fn assess_in_directory(v: &Version, working: &Path) -> Result<Compatibility> {
    let baseline = v.java_version.as_ref().map_or(8, |j| j.major);
    let loader = version::detect_loader(&serde_json::to_value(v)?);
    let mut result = Compatibility {
        majors: if v.compatible_java_majors.is_empty() {
            (baseline..=64).collect()
        } else {
            v.compatible_java_majors.clone()
        },
        reasons: vec![format!("游戏最低 Java {baseline}；优先复用本地推荐版本")],
        requires_jdk: false,
        fingerprint: String::new(),
        constraints: Vec::new(),
    };
    result
        .majors
        .retain(|major| *major >= baseline && *major <= 64);
    result.majors.sort_unstable();
    result.majors.dedup();
    if !v.compatible_java_majors.is_empty() {
        result
            .reasons
            .push("已应用版本配置声明的 Java 兼容列表".into());
    }
    if let Some((maximum, source)) = bytecode_ceiling(v) {
        result.majors.retain(|major| *major <= maximum);
        result.reasons.push(format!("{source} 最高 Java {maximum}"));
    }
    let game_id = v
        .jar
        .as_deref()
        .or(v.inherits_from.as_deref())
        .unwrap_or(&v.id);
    if baseline <= 8 && legacy_java8(v, game_id, cfg!(target_os = "linux")) {
        result.majors.retain(|major| *major <= 8);
        result
            .reasons
            .push("旧版游戏或 Linux 原生库需要 Java 8".into());
    }
    if let Some(loader) = &loader {
        // Versioned exclusions, not an allowlist of a handful of loader releases.
        // References: PCL ModJava.GetJavaRequirement and HMCL JavaVersionConstraint.
        // https://github.com/Meloong-Git/PCL/blob/93ede5d9e638c99fcec853a9ba1eecd91ceee7f5/Plain%20Craft%20Launcher%202/Modules/Minecraft/ModJava.vb
        if let Some((maximum, patch_limit)) = loader_limit(&loader.kind, &loader.version) {
            result.majors.retain(|major| *major <= maximum);
            result.reasons.push(format!(
                "{} {} 最高 Java {maximum}",
                loader.kind, loader.version
            ));
            if let Some(range) = patch_limit {
                result.constraints.push((
                    format!("{} {}", loader.kind, loader.version),
                    Constraint {
                        ranges: vec![range.into()],
                        deny: false,
                        maven: true,
                    },
                ));
            }
        }
    }

    let overrides_path = working.join("config/fabric_loader_dependencies.json");
    let overrides: Value =
        if loader.as_ref().is_some_and(|l| l.kind == "fabric") && overrides_path.is_file() {
            let value: Value = crate::storage::read_json(&overrides_path)?;
            anyhow::ensure!(
                value["version"] == 1 && value["overrides"].is_object(),
                "无法识别 Fabric Java 依赖覆盖配置"
            );
            result.reasons.push("已应用 Fabric 依赖覆盖配置".into());
            value
        } else {
            Value::Null
        };
    let mut fingerprint = ring::digest::Context::new(&ring::digest::SHA256);
    fingerprint.update(&serde_json::to_vec(v)?);
    fingerprint.update(&serde_json::to_vec(&overrides)?);
    if let Some(loader) = &loader {
        let mods = working.join("mods");
        let mut uncertain = 0;
        let mut declared = 0;
        if mods.exists() {
            let mut paths = std::fs::read_dir(&mods)
                .context("无法检查模组的 Java 兼容性")?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            for path in paths {
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                {
                    continue;
                }
                let metadata = path.metadata()?;
                fingerprint.update(path.file_name().unwrap().as_encoded_bytes());
                fingerprint.update(&metadata.len().to_le_bytes());
                fingerprint.update(
                    &metadata
                        .modified()?
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                        .to_le_bytes(),
                );
                let evidence = inspect_cached(&path, &loader.kind, &overrides);
                uncertain += usize::from(evidence.uncertain);
                declared += usize::from(!evidence.constraints.is_empty());
                result.requires_jdk |= evidence.requires_jdk;
                let source = path.file_name().unwrap().to_string_lossy().into_owned();
                for constraint in &evidence.constraints {
                    uncertain +=
                        usize::from(constraint.allows_version([baseline, 0, 0, 0]).is_none());
                    // Patch-level ranges must be checked against the actual JVM,
                    // not against a fictional x.0.0 JVM for the whole major.
                    result
                        .constraints
                        .push((source.clone(), constraint.clone()));
                }
            }
        }
        if declared > 0 {
            result
                .reasons
                .push(format!("已应用 {declared} 个模组的 Java 版本约束"));
        }
        if uncertain > 0 {
            result.reasons.push(format!(
                "{uncertain} 项模组的 Java 兼容信息不完整；保留满足已知约束的本地候选"
            ));
        }
        if result.requires_jdk {
            result
                .reasons
                .push("Preloading Tricks 3.5.6 需要完整 JDK，避免精简 JRE 缺少类调试信息".into());
        }
    }
    anyhow::ensure!(
        !result.majors.is_empty(),
        "没有同时满足游戏与加载器约束的 Java：{}",
        result.reasons.join("；")
    );
    result.fingerprint = hex::encode(fingerprint.finish());
    Ok(result)
}

fn legacy_java8(v: &Version, game: &str, linux: bool) -> bool {
    let parts: Vec<_> = game.split('.').map(str::parse::<u32>).collect();
    let older_than_16 = matches!(parts.as_slice(), [Ok(1), Ok(minor), ..] if *minor < 6);
    matches!(v.kind.as_str(), "old_alpha" | "old_beta")
        || older_than_16
        || (linux
            && v.libraries
                .iter()
                .any(|library| library.name.starts_with("org.lwjgl.lwjgl:")))
}

fn loader_limit(kind: &str, version: &str) -> Option<(u32, Option<&'static str>)> {
    // Forge's fourth version component is not SemVer (e.g. 14.23.5.2860).
    let v = java_version_number(version)?;
    match kind {
        "forge" if v[0] <= 14 => Some((8, None)),
        "forge" if v[0] <= 28 => Some((10, None)),
        "forge" if v[0] <= 31 => Some((15, None)),
        "forge" if ([34, 0, 0, 0]..=[36, 2, 25, 0]).contains(&v) => Some((8, Some("(,8.0.320]"))),
        "forge" if v[0] == 36 && v >= [36, 2, 26, 0] => Some((23, None)),
        "forge" if v[0] == 37 && v <= [37, 0, 79, 0] => Some((16, None)),
        "forge" if ([45, 0, 21, 0]..=[45, 0, 65, 0]).contains(&v) => Some((19, None)),
        "forge" if ([45, 0, 66, 0]..=[47, 4, 8, 0]).contains(&v) => Some((21, None)),
        "neoforge" if v <= [20, 2, 62, 0] && !version.contains("25w14craftmine") => {
            Some((21, None))
        }
        "fabric" if v < [0, 17, 0, 0] => Some((24, None)),
        _ => None,
    }
}

// Only a negative constraint: ASM support alone does not prove pack compatibility.
// https://asm.ow2.io/versions.html
fn asm_java_ceiling(version: &str) -> Option<u32> {
    let mut parts = version.split('.').map(str::parse::<u32>);
    let major = parts.next()?.ok()?;
    let minor = parts.next().transpose().ok()?.unwrap_or(0);
    let patch = parts.next().transpose().ok()?.unwrap_or(0);
    match (major, minor, patch) {
        (9, 11, _) => Some(28),
        (9, 10, _) => Some(27),
        (9, 9, _) => Some(26),
        (9, 8, _) => Some(25),
        (9, 7, 1..) => Some(24),
        (9, 7, 0) => Some(23),
        (9, 6, _) => Some(22),
        (9, 5, _) => Some(21),
        (9, 4, _) => Some(20),
        (9, 3, _) => Some(19),
        (9, 2, _) => Some(18),
        (9, 1, _) => Some(17),
        (9, 0, _) => Some(16),
        (5, _, _) => Some(8),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct Constraint {
    ranges: Vec<String>,
    deny: bool,
    maven: bool,
}
impl Constraint {
    fn allows_version(&self, version: [u32; 4]) -> Option<bool> {
        let mut unknown = false;
        for range in &self.ranges {
            let result = if self.maven {
                maven_matches_version(range, version)
            } else {
                fabric_matches_version(range, version)
            };
            match result {
                Some(true) => return Some(!self.deny),
                None => unknown = true,
                _ => (),
            }
        }
        if unknown { None } else { Some(self.deny) }
    }
}

/// Numeric Java release, excluding vendor/build metadata; normalizes 1.8.0_321.
/// Also accepts the quoted version lines retained in older runtime indexes.
pub(crate) fn java_version_number(value: &str) -> Option<[u32; 4]> {
    let value = value.split('"').nth(1).unwrap_or(value).trim();
    let value = value.split(['-', '+', ' ']).next()?;
    let mut parts = value
        .split(['.', '_'])
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() >= 2 && parts[0] == 1 && parts[1] <= 8 {
        parts.remove(0);
    }
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut result = [0; 4];
    result[..parts.len()].copy_from_slice(&parts);
    Some(result)
}

fn fabric_matches_version(range: &str, version: [u32; 4]) -> Option<bool> {
    let mut terms = vec![];
    for term in range.split_whitespace() {
        let term = if term.chars().next()?.is_ascii_digit() && !term.contains(['x', 'X', '*']) {
            // Fabric plain versions are exact, whereas semver's default is ^.
            let mut pieces: Vec<_> = term.split('.').collect();
            while pieces.len() < 3 {
                pieces.push("0");
            }
            format!("={}", pieces.join("."))
        } else {
            term.to_owned()
        };
        terms.push(term);
    }
    if terms.is_empty() {
        return None;
    }
    let req = semver::VersionReq::parse(&terms.join(", ")).ok()?;
    Some(req.matches(&semver::Version::new(
        version[0].into(),
        version[1].into(),
        version[2].into(),
    )))
}

fn maven_matches_version(range: &str, version: [u32; 4]) -> Option<bool> {
    let range = range.trim();
    if !range.starts_with(['[', '(']) || !range.ends_with([']', ')']) {
        return None;
    }
    let body = &range[1..range.len() - 1];
    if let Some((lower, upper)) = body.split_once(',') {
        let lower = if lower.trim().is_empty() {
            true
        } else {
            let n = java_version_number(lower.trim())?;
            if range.starts_with('[') {
                version >= n
            } else {
                version > n
            }
        };
        let upper = if upper.trim().is_empty() {
            true
        } else {
            let n = java_version_number(upper.trim())?;
            if range.ends_with(']') {
                version <= n
            } else {
                version < n
            }
        };
        Some(lower && upper)
    } else if range.starts_with('[') && range.ends_with(']') {
        Some(version == java_version_number(body)?)
    } else {
        None
    }
}

#[derive(Clone, Debug, Default)]
struct ModEvidence {
    constraints: Vec<Constraint>,
    uncertain: bool,
    requires_jdk: bool,
}
#[derive(Hash, PartialEq, Eq)]
struct CacheKey {
    path: PathBuf,
    loader: String,
    size: u64,
    modified: SystemTime,
    overrides: String,
}
fn inspect_cached(path: &Path, loader: &str, overrides: &Value) -> Arc<ModEvidence> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Arc<ModEvidence>>>> = OnceLock::new();
    let result = || -> Result<Arc<ModEvidence>> {
        let metadata = path.metadata()?;
        let key = CacheKey {
            path: path.to_owned(),
            loader: loader.into(),
            size: metadata.len(),
            modified: metadata.modified()?,
            overrides: overrides.to_string(),
        };
        let cache = CACHE.get_or_init(Default::default);
        if let Some(value) = cache.lock().unwrap().get(&key) {
            return Ok(value.clone());
        }
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        let mut budget = 64 * 1024 * 1024;
        let value = Arc::new(inspect_archive(
            &mut zip,
            loader,
            true,
            0,
            &mut budget,
            overrides,
        )?);
        let mut cache = cache.lock().unwrap();
        cache.retain(|k, _| k.path != path || k.loader != loader);
        if cache.len() >= 2048 {
            cache.clear();
        }
        cache.insert(key, value.clone());
        Ok(value)
    };
    result().unwrap_or_else(|_| {
        Arc::new(ModEvidence {
            uncertain: true,
            ..Default::default()
        })
    })
}
fn read_entry<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>> {
    let mut entry = match zip.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if entry.size() > limit {
        bail!("Java 兼容元数据过大");
    }
    let mut bytes = Vec::new();
    (&mut entry).take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("Java 兼容元数据过大");
    }
    Ok(Some(bytes))
}
fn inspect_archive<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    loader: &str,
    root: bool,
    depth: u32,
    budget: &mut u64,
    overrides: &Value,
) -> Result<ModEvidence> {
    if depth > 4 {
        bail!("嵌套模组过深");
    }
    let mut result = ModEvidence::default();
    let mut nested = vec![];
    let fabric = read_entry(zip, "fabric.mod.json", 1024 * 1024)?
        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
        .transpose()?;
    // Some Forge packs load this Fabric mod through a bridge. Inspect its identity
    // independently of the selected loader; do not require a JDK for all mods.
    result.requires_jdk = fabric.as_ref().is_some_and(|meta| {
        meta["id"] == "preloading_tricks"
            && meta["version"] == "3.5.6"
            && meta["environment"] != "server"
    });
    if loader == "fabric" {
        if let Some(mut meta) = fabric {
            apply_fabric_overrides(&mut meta, overrides);
            if meta["environment"] == "server" {
                return Ok(result);
            }
            result.uncertain = meta["depends"]["java"].is_null();
            for (key, deny) in [("depends", false), ("breaks", true)] {
                let value = &meta[key]["java"];
                if value.is_null() {
                    continue;
                }
                let ranges = if let Some(text) = value.as_str() {
                    vec![text.into()]
                } else {
                    value
                        .as_array()
                        .context("Java 约束格式无效")?
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned).context("Java 约束格式无效"))
                        .collect::<Result<Vec<_>>>()?
                };
                result.constraints.push(Constraint {
                    ranges,
                    deny,
                    maven: false,
                });
            }
            if let Some(jars) = meta["jars"].as_array() {
                for jar in jars {
                    nested.push(jar["file"].as_str().context("嵌套模组路径无效")?.to_owned());
                }
            }
        } else {
            result.uncertain = root;
        }
    } else {
        let name = if loader == "neoforge" {
            "META-INF/neoforge.mods.toml"
        } else {
            "META-INF/mods.toml"
        };
        let bytes = read_entry(zip, name, 1024 * 1024)?;
        // Early NeoForge still used mods.toml.
        let bytes = if bytes.is_none() && loader == "neoforge" {
            read_entry(zip, "META-INF/mods.toml", 1024 * 1024)?
        } else {
            bytes
        };
        if let Some(bytes) = bytes {
            let meta: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
            if let Some(features) = meta.get("features").and_then(toml::Value::as_table) {
                for feature in features.values() {
                    if let Some(range) = feature.get("javaVersion").and_then(toml::Value::as_str) {
                        result.constraints.push(Constraint {
                            ranges: vec![range.into()],
                            deny: false,
                            maven: true,
                        });
                    }
                }
            }
            result.uncertain = result.constraints.is_empty();
            if let Some(mods) = meta.get("mods").and_then(toml::Value::as_array) {
                result.uncertain |= mods.iter().any(|module| {
                    let Some(id) = module.get("modId").and_then(toml::Value::as_str) else {
                        return true;
                    };
                    meta.get("features")
                        .and_then(|features| features.get(id))
                        .and_then(|feature| feature.get("javaVersion"))
                        .and_then(toml::Value::as_str)
                        .is_none()
                });
            }
        } else {
            result.uncertain = root;
        }
        if let Some(bytes) = read_entry(zip, "META-INF/jarjar/metadata.json", 1024 * 1024)? {
            let meta: Value = serde_json::from_slice(&bytes)?;
            for jar in meta["jars"].as_array().context("嵌套模组清单无效")? {
                nested.push(jar["path"].as_str().context("嵌套模组路径无效")?.to_owned());
            }
        }
    }
    if nested.len() > 128 {
        bail!("嵌套模组过多");
    }
    for path in nested {
        let child = (|| -> Result<ModEvidence> {
            let bytes =
                read_entry(zip, &path, (*budget).min(32 * 1024 * 1024))?.context("嵌套模组缺失")?;
            *budget = budget
                .checked_sub(bytes.len() as u64)
                .context("嵌套模组过大")?;
            let mut child = zip::ZipArchive::new(Cursor::new(bytes))?;
            inspect_archive(&mut child, loader, false, depth + 1, budget, overrides)
        })();
        match child {
            Ok(evidence) => {
                result.uncertain |= evidence.uncertain;
                result.requires_jdk |= evidence.requires_jdk;
                result.constraints.extend(evidence.constraints);
            }
            Err(_) => result.uncertain = true,
        }
    }
    Ok(result)
}

// Fabric applies replacement, removal, and addition to each mod independently.
fn apply_fabric_overrides(meta: &mut Value, overrides: &Value) {
    let Some(id) = meta["id"].as_str() else {
        return;
    };
    let patch = &overrides["overrides"][id];
    for kind in ["depends", "breaks"] {
        if let Some(replacement) = patch.get(kind) {
            meta[kind] = replacement.clone();
        }
        if let Some(remove) = patch.get(format!("-{kind}")).and_then(Value::as_object)
            && let Some(deps) = meta.get_mut(kind).and_then(Value::as_object_mut)
        {
            for name in remove.keys() {
                deps.remove(name);
            }
        }
        if let Some(add) = patch.get(format!("+{kind}")).and_then(Value::as_object) {
            if !meta[kind].is_object() {
                meta[kind] = serde_json::json!({});
            }
            meta[kind].as_object_mut().unwrap().extend(add.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn profile(loader: &str, asm: &str) -> Version {
        let mut v = serde_json::to_value(crate::test_http::version_fixture("1.20.1")).unwrap();
        v["javaVersion"] = json!({"majorVersion":17});
        v["compatibleJavaMajors"] = json!((17..=26).collect::<Vec<_>>());
        v["libraries"] = json!([{"name":loader,"url":"https://example.invalid/"},
            {"name":format!("org.ow2.asm:asm:{asm}"),"url":"https://example.invalid/"}]);
        v["mainClass"] = json!("loader.Main");
        crate::version::resolve_chain(&[v]).unwrap().version
    }
    fn jar(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn install_mod(game: &Path, name: &str, entries: &[(&str, Vec<u8>)]) {
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods").join(name), jar(entries)).unwrap();
    }
    fn fabric_meta(java: Value, breaks: Value) -> Vec<u8> {
        serde_json::to_vec(
            &json!({"id":"fixture", "depends":{"java":java},"breaks":{"java":breaks}}),
        )
        .unwrap()
    }

    fn accepted(v: &Version, dir: &Path) -> Vec<u32> {
        let c = assess(v, dir).unwrap();
        c.majors
            .iter()
            .copied()
            .filter(|m| c.rejection(*m, &format!("{m}.0.0")).is_none())
            .collect()
    }

    #[test]
    fn explicit_17_to_26_support_allows_26_and_mod_constraints_intersect() {
        let temp = tempfile::tempdir().unwrap();
        let v = profile("net.fabricmc:fabric-loader:0.18.6", "9.9");
        install_mod(
            temp.path(),
            "mod.jar",
            &[(
                "fabric.mod.json",
                fabric_meta(json!(">=17 <27"), Value::Null),
            )],
        );
        assert_eq!(accepted(&v, temp.path()), (17..=26).collect::<Vec<_>>());
        install_mod(
            temp.path(),
            "other.jar",
            &[(
                "fabric.mod.json",
                fabric_meta(json!(["17.x", ">=21 <27"]), json!("26.x")),
            )],
        );
        assert_eq!(accepted(&v, temp.path()), vec![17, 21, 22, 23, 24, 25]);
    }

    #[test]
    fn asm_is_an_upper_bound_not_positive_evidence_of_compatibility() {
        let temp = tempfile::tempdir().unwrap();
        let mut v = profile("net.minecraftforge:forge:1.20.1-47.4.4", "9.7.1");
        assert_eq!(accepted(&v, temp.path()).last(), Some(&21));
        v.compatible_java_majors.clear();
        assert_eq!(accepted(&v, temp.path()), (17..=21).collect::<Vec<_>>());
        v.libraries[1].name = "org.ow2.asm:asm:9.9".into();
        assert_eq!(accepted(&v, temp.path()), (17..=21).collect::<Vec<_>>());
    }

    #[test]
    fn nested_constraints_apply_and_changed_mods_invalidate_cached_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let v = profile("net.fabricmc:fabric-loader:0.18.6", "9.9");
        let child = jar(&[(
            "fabric.mod.json",
            fabric_meta(json!(">=17 <=21"), Value::Null),
        )]);
        let parent =
            serde_json::to_vec(&json!({"depends":{"java":">=17"}, "jars":[{"file":"nested.jar"}]}))
                .unwrap();
        install_mod(
            temp.path(),
            "parent.jar",
            &[("fabric.mod.json", parent), ("nested.jar", child)],
        );
        assert_eq!(accepted(&v, temp.path()).last(), Some(&21));
        install_mod(
            temp.path(),
            "parent.jar",
            &[(
                "fabric.mod.json",
                fabric_meta(json!(">=17 <=26"), Value::Null),
            )],
        );
        assert_eq!(accepted(&v, temp.path()).last(), Some(&26));
        install_mod(
            temp.path(),
            "unknown.jar",
            &[("fabric.mod.json", b"{\"id\":\"unknown\"}".to_vec())],
        );
        assert_eq!(accepted(&v, temp.path()).last(), Some(&26));
    }

    #[test]
    fn forge_and_neoforge_features_restrict_automatic_selection() {
        for (loader, file) in [
            (
                "net.minecraftforge:forge:1.20.1-47.4.10",
                "META-INF/mods.toml",
            ),
            (
                "net.neoforged:neoforge:20.4.100",
                "META-INF/neoforge.mods.toml",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let v = profile(loader, "9.9");
            install_mod(
                temp.path(),
                "mod.jar",
                &[(
                    file,
                    b"[[mods]]\nmodId='test'\n[features.test]\njavaVersion='[17,22)'\n".to_vec(),
                )],
            );
            assert_eq!(accepted(&v, temp.path()), vec![17, 18, 19, 20, 21]);
        }
    }

    #[test]
    fn unknown_and_overridden_predicates_do_not_guess_support_or_ignore_known_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let v = profile("net.fabricmc:fabric-loader:0.18.6", "9.9");
        install_mod(
            temp.path(),
            "mod.jar",
            &[("fabric.mod.json", fabric_meta(json!(">=30"), Value::Null))],
        );
        assert!(accepted(&v, temp.path()).is_empty());
        std::fs::create_dir_all(temp.path().join("config")).unwrap();
        std::fs::write(
            temp.path().join("config/fabric_loader_dependencies.json"),
            r#"{"version":1,"overrides":{"fixture":{"-depends":{"java":""}}}}"#,
        )
        .unwrap();
        assert_eq!(accepted(&v, temp.path()), (17..=26).collect::<Vec<_>>());
        assert_eq!(
            fabric_matches_version("something unsupported", [26, 0, 0, 0]),
            None
        );
        assert_eq!(
            maven_matches_version("[17.0.8,)", [17, 0, 7, 0]),
            Some(false)
        );
        assert_eq!(fabric_matches_version("17", [17, 0, 0, 0]), Some(true));
        assert_eq!(fabric_matches_version("17", [18, 0, 0, 0]), Some(false));
    }

    #[test]
    fn missing_java_metadata_does_not_reject_newer_jvms_and_only_specific_mods_need_jdk() {
        let temp = tempfile::tempdir().unwrap();
        let mut v = profile("net.minecraftforge:forge:1.20.1-47.4.4", "9.7.1");
        v.compatible_java_majors.clear();
        install_mod(
            temp.path(),
            "ordinary.jar",
            &[(
                "META-INF/mods.toml",
                b"[[mods]]\nmodId='ordinary'\n".to_vec(),
            )],
        );
        let original = assess(&v, temp.path()).unwrap();
        assert!(original.rejection(21, "21.0.7").is_none());
        assert!(original.rejection(26, "26.0.2").is_some());
        assert!(!original.requires_jdk);
        install_mod(
            temp.path(),
            "preloading.jar",
            &[(
                "fabric.mod.json",
                br#"{"id":"preloading_tricks","version":"3.5.6"}"#.to_vec(),
            )],
        );
        let changed = assess(&v, temp.path()).unwrap();
        assert!(changed.requires_jdk);
        assert_ne!(original.fingerprint, changed.fingerprint);
        std::fs::rename(
            temp.path().join("mods/preloading.jar"),
            temp.path().join("mods/preloading.jar.disabled"),
        )
        .unwrap();
        assert!(!assess(&v, temp.path()).unwrap().requires_jdk);
        assert_eq!(
            original.fingerprint,
            assess(&v, temp.path()).unwrap().fingerprint
        );
    }

    #[test]
    fn java_patch_constraints_and_fabric_soft_conflicts_are_not_major_version_bans() {
        let temp = tempfile::tempdir().unwrap();
        let v = profile("net.fabricmc:fabric-loader:0.18.6", "9.9");
        install_mod(
            temp.path(),
            "mod.jar",
            &[(
                "fabric.mod.json",
                br#"{"id":"test","depends":{"java":">=17.0.8 <22"},"conflicts":{"java":"21.x"}}"#
                    .to_vec(),
            )],
        );
        let compatibility = assess(&v, temp.path()).unwrap();
        assert!(compatibility.rejection(17, "17.0.7").is_some());
        assert!(compatibility.rejection(17, "17.0.8").is_none());
        assert!(compatibility.rejection(21, "21.0.7").is_none());
        assert!(compatibility.rejection(25, "25.0.1").is_some());
        assert_eq!(
            maven_matches_version("[17.0.8.1,21)", [17, 0, 8, 0]),
            Some(false)
        );
        assert_eq!(
            maven_matches_version("[17.0.8.1,21)", [17, 0, 8, 1]),
            Some(true)
        );
        assert_eq!(java_version_number("1.8.0_321-b07"), Some([8, 0, 321, 0]));
    }

    #[test]
    fn loader_exclusions_preserve_legacy_and_patch_level_incompatibilities() {
        let temp = tempfile::tempdir().unwrap();
        let mut v = profile("net.minecraftforge:forge:1.16.5-36.2.20", "9.1");
        v.java_version = Some(crate::model::JavaVersion { major: 8 });
        v.compatible_java_majors.clear();
        let c = assess(&v, temp.path()).unwrap();
        assert!(c.rejection(8, "1.8.0_320").is_none());
        assert!(c.rejection(8, "1.8.0_321").is_some());
        v.main_class = "net.minecraft.launchwrapper.Launch".into();
        assert_eq!(assess(&v, temp.path()).unwrap().majors, vec![8]);
        assert_eq!(loader_limit("fabric", "0.16.14"), Some((24, None)));
        assert_eq!(loader_limit("neoforge", "20.2.62-beta"), Some((21, None)));
        assert_eq!(loader_limit("neoforge", "21.1.200"), None);
    }

    #[test]
    fn fabric_overrides_only_change_the_targeted_mod_and_dependency_kind() {
        let temp = tempfile::tempdir().unwrap();
        let v = profile("net.fabricmc:fabric-loader:0.18.6", "9.9");
        install_mod(
            temp.path(),
            "first.jar",
            &[(
                "fabric.mod.json",
                br#"{"id":"first","depends":{"java":">=30"}}"#.to_vec(),
            )],
        );
        install_mod(
            temp.path(),
            "second.jar",
            &[(
                "fabric.mod.json",
                br#"{"id":"second","breaks":{"java":"21.x"}}"#.to_vec(),
            )],
        );
        std::fs::create_dir_all(temp.path().join("config")).unwrap();
        std::fs::write(
            temp.path().join("config/fabric_loader_dependencies.json"),
            br#"{"version":1,"overrides":{"first":{"+depends":{"java":">=17"}}}}"#,
        )
        .unwrap();
        let c = assess(&v, temp.path()).unwrap();
        assert!(c.rejection(17, "17.0.8").is_none());
        assert!(c.rejection(21, "21.0.7").is_some());
        assert!(c.rejection(25, "25.0.1").is_none());
    }
}
