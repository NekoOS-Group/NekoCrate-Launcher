//! Evidence-based Java selection. A minimum version is not proof that every
//! future JVM is compatible. Unknown constraints never widen automatic choices.
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
    let baseline = v.java_version.as_ref().map_or(8, |j| j.major);
    let loader = version::detect_loader(&serde_json::to_value(v)?);
    let mut reasons = vec![];
    let mut majors = if !v.compatible_java_majors.is_empty() {
        reasons.push("版本配置明确声明了 Java 兼容列表".into());
        v.compatible_java_majors.clone()
    } else if let Some(loader) = &loader {
        let ceiling = verified_loader_ceiling(&loader.kind, &loader.version, baseline);
        if ceiling > baseline {
            reasons.push(format!(
                "{} {} 的已核实 Java 支持范围",
                loader.kind, loader.version
            ));
        } else {
            reasons.push("缺少更新 Java 的兼容依据，保留游戏声明的版本".into());
        }
        (baseline..=ceiling.max(baseline)).collect()
    } else {
        reasons.push("缺少更新 Java 的兼容依据，保留游戏声明的版本".into());
        vec![baseline]
    };
    // Reject malformed/unbounded declarations and never undercut the game minimum.
    majors.retain(|major| *major >= baseline && *major <= 64);
    majors.sort_unstable();
    majors.dedup();
    if v.main_class == "net.minecraft.launchwrapper.Launch" {
        majors.retain(|major| *major <= 8);
        reasons.push("LaunchWrapper 使用旧类加载器，自动匹配限制为 Java 8".into());
    }
    if loader.is_some() {
        for library in &v.libraries {
            if let Some(asm) = library.name.strip_prefix("org.ow2.asm:asm:")
                && let Some(max) = asm_java_ceiling(asm)
            {
                majors.retain(|major| *major <= max);
                reasons.push(format!("ASM {asm} 最多识别 Java {max} 字节码"));
            }
        }
    }

    if let Some(loader) = &loader {
        let working = version::working_directory(game, v)?;
        let mods = working.join("mods");
        let overrides = loader.kind == "fabric"
            && working
                .join("config/fabric_loader_dependencies.json")
                .exists();
        let mut uncertain = 0;
        let mut declared = 0;
        if mods.exists() {
            for entry in std::fs::read_dir(&mods).context("无法检查模组的 Java 兼容性")? {
                let path = entry?.path();
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                {
                    continue;
                }
                let evidence = inspect_cached(&path, &loader.kind);
                if evidence.uncertain {
                    uncertain += 1;
                }
                if !evidence.constraints.is_empty() {
                    declared += 1;
                }
                for constraint in evidence.constraints.iter().filter(|_| !overrides) {
                    majors.retain(|major| constraint.allows(*major) != Some(false));
                    if majors
                        .iter()
                        .any(|major| constraint.allows(*major).is_none())
                    {
                        uncertain += 1;
                    }
                }
                if majors.is_empty() {
                    bail!("模组 Java 要求与已知兼容范围冲突：{}", path.display());
                }
            }
        }
        // Dependency overrides can replace/remove Java constraints. Until their
        // full semantics are resolved, do not upgrade a pack based on stale ones.
        if overrides {
            uncertain += 1;
        }
        if declared > 0 {
            reasons.push(format!("已应用 {declared} 个模组的 Java 版本约束"));
        }
        if uncertain > 0 {
            majors.retain(|major| *major == baseline);
            reasons.push(format!(
                "{uncertain} 项模组兼容信息不完整，暂不自动跨 Java 主版本"
            ));
        }
    }
    if majors.is_empty() {
        bail!(
            "没有同时满足游戏、加载器和模组约束的 Java：{}",
            reasons.join("；")
        );
    }
    Ok(Compatibility { majors, reasons })
}

// Narrow, versioned evidence; never infer compatibility of future loader releases.
// Fabric 0.18.6 CI tests Java 21/25 and its Mixin service declares JAVA_25:
// https://github.com/FabricMC/fabric-loader/blob/0.18.6/.github/workflows/build.yml
// Forge 1.20.1 gained Java 25 support in 47.4.9 (ASM upgrade, PR #10664):
// https://github.com/MinecraftForge/MinecraftForge/pull/10664
fn verified_loader_ceiling(kind: &str, version: &str, baseline: u32) -> u32 {
    let Ok(v) = semver::Version::parse(version) else {
        return baseline;
    };
    if !v.pre.is_empty() {
        return baseline;
    }
    match kind {
        "fabric" if v == semver::Version::new(0, 18, 6) && baseline >= 17 => 25,
        "forge"
            if v.major == 47 && v.minor == 4 && (9..=10).contains(&v.patch) && baseline == 17 =>
        {
            25
        }
        _ => baseline,
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
    fn allows(&self, major: u32) -> Option<bool> {
        let mut unknown = false;
        for range in &self.ranges {
            let result = if self.maven {
                maven_matches(range, major)
            } else {
                fabric_matches(range, major)
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

fn fabric_matches(range: &str, major: u32) -> Option<bool> {
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
    Some(req.matches(&semver::Version::new(major.into(), 0, 0)))
}

fn maven_matches(range: &str, major: u32) -> Option<bool> {
    let range = range.trim();
    if !range.starts_with(['[', '(']) || !range.ends_with([']', ')']) {
        return None;
    }
    let body = &range[1..range.len() - 1];
    if let Some((lower, upper)) = body.split_once(',') {
        let lower = if lower.trim().is_empty() {
            true
        } else {
            let n = major_bound(lower.trim())?;
            if range.starts_with('[') {
                major >= n
            } else {
                major > n
            }
        };
        let upper = if upper.trim().is_empty() {
            true
        } else {
            let n = major_bound(upper.trim())?;
            if range.ends_with(']') {
                major <= n
            } else {
                major < n
            }
        };
        Some(lower && upper)
    } else if range.starts_with('[') && range.ends_with(']') {
        Some(major == major_bound(body)?)
    } else {
        None
    }
}
fn major_bound(value: &str) -> Option<u32> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    parts.all(|p| p == "0").then_some(major)
}

#[derive(Clone, Debug, Default)]
struct ModEvidence {
    constraints: Vec<Constraint>,
    uncertain: bool,
}
#[derive(Hash, PartialEq, Eq)]
struct CacheKey {
    path: PathBuf,
    loader: String,
    size: u64,
    modified: SystemTime,
}
fn inspect_cached(path: &Path, loader: &str) -> Arc<ModEvidence> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Arc<ModEvidence>>>> = OnceLock::new();
    let result = || -> Result<Arc<ModEvidence>> {
        let metadata = path.metadata()?;
        let key = CacheKey {
            path: path.to_owned(),
            loader: loader.into(),
            size: metadata.len(),
            modified: metadata.modified()?,
        };
        let cache = CACHE.get_or_init(Default::default);
        if let Some(value) = cache.lock().unwrap().get(&key) {
            return Ok(value.clone());
        }
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        let mut budget = 64 * 1024 * 1024;
        let value = Arc::new(inspect_archive(&mut zip, loader, true, 0, &mut budget)?);
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
) -> Result<ModEvidence> {
    if depth > 4 {
        bail!("嵌套模组过深");
    }
    let mut result = ModEvidence::default();
    let mut nested = vec![];
    if loader == "fabric" {
        if let Some(bytes) = read_entry(zip, "fabric.mod.json", 1024 * 1024)? {
            let meta: Value = serde_json::from_slice(&bytes)?;
            if meta["environment"] == "server" {
                return Ok(result);
            }
            result.uncertain = meta["depends"]["java"].is_null();
            for (key, deny) in [("depends", false), ("breaks", true), ("conflicts", true)] {
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
            inspect_archive(&mut child, loader, false, depth + 1, budget)
        })();
        match child {
            Ok(evidence) => {
                result.uncertain |= evidence.uncertain;
                result.constraints.extend(evidence.constraints);
            }
            Err(_) => result.uncertain = true,
        }
    }
    Ok(result)
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
        assert_eq!(
            assess(&v, temp.path()).unwrap().majors,
            (17..=26).collect::<Vec<_>>()
        );
        install_mod(
            temp.path(),
            "other.jar",
            &[(
                "fabric.mod.json",
                fabric_meta(json!(["17.x", ">=21 <27"]), json!("26.x")),
            )],
        );
        assert_eq!(
            assess(&v, temp.path()).unwrap().majors,
            vec![17, 21, 22, 23, 24, 25]
        );
    }

    #[test]
    fn asm_is_an_upper_bound_not_positive_evidence_of_compatibility() {
        let temp = tempfile::tempdir().unwrap();
        let mut v = profile("net.minecraftforge:forge:1.20.1-47.4.4", "9.7.1");
        assert_eq!(assess(&v, temp.path()).unwrap().majors.last(), Some(&24));
        v.compatible_java_majors.clear();
        assert_eq!(assess(&v, temp.path()).unwrap().majors, vec![17]);
        v.libraries[1].name = "org.ow2.asm:asm:9.9".into();
        assert_eq!(assess(&v, temp.path()).unwrap().majors, vec![17]);
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
        assert_eq!(assess(&v, temp.path()).unwrap().majors.last(), Some(&21));
        install_mod(
            temp.path(),
            "parent.jar",
            &[(
                "fabric.mod.json",
                fabric_meta(json!(">=17 <=26"), Value::Null),
            )],
        );
        assert_eq!(assess(&v, temp.path()).unwrap().majors.last(), Some(&26));
        install_mod(
            temp.path(),
            "unknown.jar",
            &[("fabric.mod.json", b"{\"id\":\"unknown\"}".to_vec())],
        );
        assert_eq!(assess(&v, temp.path()).unwrap().majors, vec![17]);
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
            assert_eq!(
                assess(&v, temp.path()).unwrap().majors,
                vec![17, 18, 19, 20, 21]
            );
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
        assert!(assess(&v, temp.path()).is_err());
        std::fs::create_dir_all(temp.path().join("config")).unwrap();
        std::fs::write(
            temp.path().join("config/fabric_loader_dependencies.json"),
            "{}",
        )
        .unwrap();
        assert_eq!(assess(&v, temp.path()).unwrap().majors, vec![17]);
        assert_eq!(fabric_matches("something unsupported", 26), None);
        assert_eq!(maven_matches("[17.0.8,)", 17), None);
        assert_eq!(fabric_matches("17", 17), Some(true));
        assert_eq!(fabric_matches("17", 18), Some(false));
    }
}
