//! On-demand, read-only inspection of a listed item. No archive extraction,
//! network lookups, recursive world scans, or loading/executing mod code.
use super::{ContentKind, read_content};
use crate::{Launcher, storage};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;
mod world;

const TEXT_LIMIT: u64 = 1024 * 1024;
pub(super) const MOD_DESCRIPTORS: [&str; 5] = [
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/neoforge.mods.toml",
    "META-INF/mods.toml",
    "mcmod.info",
];

#[derive(Debug, Serialize)]
pub struct ContentMetadata {
    pub path: PathBuf,
    pub size: Option<u64>,
    pub sources: Vec<ContentSource>,
}
#[derive(Debug, Serialize)]
pub struct ContentSource {
    pub source: String,
    pub revealable: bool,
    pub title: Option<String>,
    pub description: Option<String>,
    pub fields: Vec<ContentField>,
    pub dependencies: Vec<ContentDependency>,
    pub issue: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct ContentField {
    pub label: String,
    pub value: String,
    pub format: Option<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct ContentDependency {
    pub id: String,
    pub requirement: String,
    pub relation: &'static str,
}

impl ContentSource {
    fn new(source: &str) -> Self {
        Self {
            source: source.into(),
            revealable: false,
            title: None,
            description: None,
            fields: Vec::new(),
            dependencies: Vec::new(),
            issue: None,
        }
    }
    fn field(&mut self, label: &str, value: impl ToString) {
        self.fields.push(ContentField {
            label: label.into(),
            value: value.to_string(),
            format: None,
        });
    }
    fn value(&mut self, label: &str, value: &Value) {
        if let Some(value) = scalar(value).filter(|s| !s.is_empty() && !s.contains("${")) {
            self.field(label, value);
        }
    }
    fn enum_field(&mut self, label: &str, value: &str) {
        self.fields.push(ContentField {
            label: label.into(),
            value: value.into(),
            format: Some("translate"),
        });
    }
}

impl Launcher {
    pub fn version_content_metadata(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
        file: &str,
    ) -> Result<ContentMetadata> {
        let path = self.listed_content_path(id, version, kind, file)?;
        let info = fs::metadata(&path)?;
        let mut sources = match kind {
            ContentKind::Unresolved => bail!("此文件尚未下载，暂无本地元数据"),
            ContentKind::Mods => read_mod(&path)?,
            ContentKind::Datapacks | ContentKind::Resourcepacks => read_pack(&path)?,
            ContentKind::Saves => world::read(&path),
            ContentKind::Shaderpacks => Vec::new(),
        };
        if info.is_dir() {
            for source in &mut sources {
                source.revealable =
                    contained(&path, &path.join(&source.source)).is_ok_and(|p| p.is_file());
            }
        }
        let (working, _) = self.content_location(id, version)?;
        if let Some(source) = crate::resources::provenance_source(&working, &path)? {
            sources.push(source);
        }
        Ok(ContentMetadata {
            path,
            size: info.is_file().then_some(info.len()),
            sources,
        })
    }

    pub fn version_content_path(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
        file: &str,
        source: Option<&str>,
    ) -> Result<PathBuf> {
        let path = self.listed_content_path(id, version, kind, file)?;
        if let Some(source) = source {
            check_source(kind, source)?;
            if !path.is_dir() {
                bail!("元数据文件位于压缩包内");
            }
            return contained(&path, &path.join(source));
        }
        Ok(path)
    }

    pub fn version_content_source(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
        file: &str,
        source: &str,
    ) -> Result<String> {
        check_source(kind, source)?;
        let path = self.listed_content_path(id, version, kind, file)?;
        if matches!(kind, ContentKind::Saves) {
            return world::raw(&path, source);
        }
        let bytes = if path.is_dir() {
            bounded(
                fs::File::open(contained(&path, &path.join(source))?)?,
                TEXT_LIMIT,
            )?
        } else {
            let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
            let entry = archive.by_name(source)?;
            if entry.size() > TEXT_LIMIT {
                bail!("元数据内容过大");
            }
            bounded(entry, TEXT_LIMIT)?
        };
        Ok(String::from_utf8(bytes)?)
    }

    fn listed_content_path(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
        file: &str,
    ) -> Result<PathBuf> {
        let relative = storage::safe_relative(file)?;
        let (working, resolved) = self.content_location(id, version)?;
        let listing = read_content(&working, resolved.as_ref(), kind, false)?;
        if !listing.files.iter().any(|item| item.file_name == file) {
            bail!("此文件不在当前版本的内容列表中");
        }
        contained(&listing.directory, &listing.directory.join(relative))
    }
}

fn check_source(kind: ContentKind, source: &str) -> Result<()> {
    let allowed = match kind {
        ContentKind::Unresolved => false,
        ContentKind::Mods => MOD_DESCRIPTORS.contains(&source),
        ContentKind::Datapacks | ContentKind::Resourcepacks => source == "pack.mcmeta",
        ContentKind::Saves => matches!(source, "level.dat" | "level.dat_old"),
        ContentKind::Shaderpacks => false,
    };
    if !allowed {
        bail!("未找到元数据文件");
    }
    Ok(())
}

fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if !path.starts_with(root.canonicalize()?) {
        bail!("内容路径不能离开所选目录");
    }
    Ok(path)
}
fn bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("元数据内容过大");
    }
    Ok(bytes)
}
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().into()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn names(value: &Value) -> Option<String> {
    let result = match value {
        Value::String(s) => s.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(|v| scalar(v).or_else(|| scalar(&v["name"])))
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(values) => values.keys().cloned().collect::<Vec<_>>().join(", "),
        _ => return None,
    };
    (!result.is_empty()).then_some(result)
}
fn plain_text(value: &Value) -> Option<String> {
    fn append(value: &Value, out: &mut String, depth: usize) {
        if depth > 32 {
            return;
        }
        match value {
            Value::String(s) => out.push_str(s),
            Value::Array(values) => {
                for v in values {
                    append(v, out, depth + 1);
                }
            }
            Value::Object(_) => {
                if let Some(v) = value
                    .get("text")
                    .or_else(|| value.get("fallback"))
                    .or_else(|| value.get("translate"))
                {
                    append(v, out, depth + 1);
                }
                if let Some(extra) = value.get("extra") {
                    append(extra, out, depth + 1);
                }
            }
            _ => (),
        }
    }
    let mut out = String::new();
    append(value, &mut out, 0);
    let mut chars = out.chars();
    let mut clean = String::new();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
        } else {
            clean.push(ch);
        }
    }
    let clean = clean.trim().to_owned();
    (!clean.is_empty()).then_some(clean)
}
fn dependency(source: &mut ContentSource, id: &str, requirement: &Value, relation: &'static str) {
    let requirement = scalar(requirement)
        .or_else(|| {
            requirement
                .as_array()
                .map(|a| a.iter().filter_map(scalar).collect::<Vec<_>>().join(" | "))
        })
        .unwrap_or_default();
    source.dependencies.push(ContentDependency {
        id: id.into(),
        requirement,
        relation,
    });
}

fn read_mod(path: &Path) -> Result<Vec<ContentSource>> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut sources = Vec::new();
    // Multiple descriptors can coexist in a cross-loader JAR. Keep their provenance.
    for descriptor in MOD_DESCRIPTORS {
        let mut entry = match archive.by_name(descriptor) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => continue,
            Err(error) => return Err(error.into()),
        };
        let parsed = (|| -> Result<Vec<ContentSource>> {
            if entry.size() > TEXT_LIMIT {
                bail!("元数据内容过大");
            }
            let bytes = bounded(&mut entry, TEXT_LIMIT)?;
            let value: Value = if descriptor.ends_with(".toml") {
                serde_json::to_value(toml::from_str::<toml::Value>(std::str::from_utf8(&bytes)?)?)?
            } else {
                serde_json::from_slice(&bytes)?
            };
            parse_mod(descriptor, &value)
        })();
        match parsed {
            Ok(parsed) => sources.extend(parsed),
            Err(error) => {
                let mut source = ContentSource::new(descriptor);
                source.issue = Some(error.to_string());
                sources.push(source);
            }
        }
    }
    Ok(sources)
}

fn parse_mod(descriptor: &str, value: &Value) -> Result<Vec<ContentSource>> {
    let toml = descriptor.ends_with(".toml");
    let quilt = descriptor == "quilt.mod.json";
    let infos: Vec<&Value> = if toml {
        value["mods"]
            .as_array()
            .context("Mod 描述缺少 mods 列表")?
            .iter()
            .collect()
    } else if descriptor == "mcmod.info" {
        value
            .as_array()
            .or_else(|| value["modList"].as_array())
            .context("Mod 描述格式无效")?
            .iter()
            .collect()
    } else if quilt {
        vec![&value["quilt_loader"]]
    } else {
        vec![value]
    };
    let mut sources = Vec::new();
    for info in infos {
        if !info.is_object() {
            bail!("Mod 描述格式无效");
        }
        let mut source = ContentSource::new(descriptor);
        let display = if quilt { &info["metadata"] } else { info };
        source.title = scalar(&display["name"]).or_else(|| scalar(&info["displayName"]));
        source.description = plain_text(&display["description"]);
        let id = info["id"]
            .as_str()
            .or_else(|| info["modId"].as_str())
            .or_else(|| info["modid"].as_str());
        if let Some(id) = id {
            source.field("Mod ID", id);
        }
        source.value("版本", &info["version"]);
        source.value("适用游戏版本", &info["mcversion"]);
        let authors = names(&display["authors"])
            .or_else(|| names(&display["authorList"]))
            .or_else(|| names(&display["contributors"]));
        if let Some(authors) = authors {
            source.field("作者", authors);
        }
        if let Some(license) = names(&display["license"]).or_else(|| names(&value["license"])) {
            source.field("许可证", license);
        }
        for (label, field) in [
            ("主页", "homepage"),
            ("源代码", "sources"),
            ("问题反馈", "issues"),
        ] {
            source.value(label, &display["contact"][field]);
        }
        source.value("主页", &info["displayURL"]);
        source.value("主页", &info["url"]);
        source.value("运行环境", &value["environment"]);
        if toml {
            if let Some(deps) = id.and_then(|id| value["dependencies"][id].as_array()) {
                for dep in deps {
                    if let Some(id) = dep["modId"].as_str() {
                        let relation = match dep["type"].as_str() {
                            Some("optional") => "可选",
                            Some("incompatible") => "冲突",
                            Some("discouraged") => "不推荐",
                            _ if dep["mandatory"].as_bool() == Some(false) => "可选",
                            _ => "必需",
                        };
                        dependency(&mut source, id, &dep["versionRange"], relation);
                    }
                }
            }
            merge_loader(&mut source, value);
        } else if quilt {
            if let Some(deps) = info["depends"].as_array() {
                for dep in deps {
                    if let Some(id) = dep["id"].as_str() {
                        dependency(
                            &mut source,
                            id,
                            &dep["versions"],
                            if dep["optional"].as_bool() == Some(true) {
                                "可选"
                            } else {
                                "必需"
                            },
                        );
                    }
                }
            }
        } else {
            for (key, relation) in [
                ("depends", "必需"),
                ("recommends", "推荐"),
                ("suggests", "可选"),
                ("breaks", "冲突"),
                ("conflicts", "不推荐"),
            ] {
                if let Some(deps) = info[key].as_object() {
                    for (id, version) in deps {
                        dependency(&mut source, id, version, relation);
                    }
                }
            }
            for (key, relation) in [("dependencies", "声明依赖"), ("requiredMods", "必需")] {
                if let Some(deps) = info[key].as_array() {
                    for dep in deps.iter().filter_map(Value::as_str) {
                        legacy_dependency(&mut source, dep, relation);
                    }
                }
            }
        }
        separate_game_requirement(&mut source);
        sources.push(source);
    }
    Ok(sources)
}

fn legacy_dependency(source: &mut ContentSource, text: &str, default_relation: &'static str) {
    let (prefix, rest) = text.split_once(':').unwrap_or(("", text));
    let (id, range) = rest.split_once('@').unwrap_or((rest, ""));
    let relation = if prefix.starts_with("required-") {
        "必需"
    } else {
        default_relation
    };
    dependency(source, id, &Value::String(range.into()), relation);
}

fn separate_game_requirement(source: &mut ContentSource) {
    let mut ranges = Vec::new();
    source.dependencies.retain(|dep| {
        if dep.id == "minecraft" && matches!(dep.relation, "必需" | "声明依赖") {
            if !dep.requirement.is_empty() && !ranges.contains(&dep.requirement) {
                ranges.push(dep.requirement.clone());
            }
            false
        } else {
            true
        }
    });
    if ranges.is_empty() {
        return;
    }
    // Prefer the explicit dependency constraint over the legacy build-version hint.
    source.fields.retain(|f| f.label != "适用游戏版本");
    source.field("适用游戏版本", ranges.join("; "));
}

fn merge_loader(source: &mut ContentSource, value: &Value) {
    let language = scalar(&value["modLoader"]).unwrap_or_default();
    let range = scalar(&value["loaderVersion"]).unwrap_or_default();
    let language_requirement = format!("{language} {range}").trim().to_owned();
    let mut loaders = Vec::new();
    source.dependencies.retain(|dep| {
        let name = match (dep.id.as_str(), dep.relation) {
            ("forge", "必需") => "Forge",
            ("neoforge", "必需") => "NeoForge",
            _ => return true,
        };
        // Language-loader and platform versions can be different version domains
        // (notably javafml 1 vs. NeoForge 20.x). Keep both constraints when unequal.
        if !language.is_empty() && !range.is_empty() && dep.requirement == range {
            loaders.push(format!("{name} / {language} {range}"));
        } else {
            let platform = format!("{name} {}", dep.requirement).trim().to_owned();
            loaders.push(if language_requirement.is_empty() {
                platform
            } else {
                format!("{platform} · {language_requirement}")
            });
        }
        false
    });
    if loaders.is_empty() {
        if !language_requirement.is_empty() {
            source.field("加载器", language_requirement);
        }
    } else {
        source.field("加载器", loaders.join("; "));
    }
}

fn read_pack(path: &Path) -> Result<Vec<ContentSource>> {
    let mut source = ContentSource::new("pack.mcmeta");
    let result = (|| -> Result<()> {
        let bytes = if path.is_dir() {
            bounded(
                fs::File::open(contained(path, &path.join("pack.mcmeta"))?)?,
                TEXT_LIMIT,
            )?
        } else {
            let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
            let mut entry = archive
                .by_name("pack.mcmeta")
                .context("未找到 pack.mcmeta")?;
            if entry.size() > TEXT_LIMIT {
                bail!("元数据内容过大");
            }
            bounded(&mut entry, TEXT_LIMIT)?
        };
        let value: Value = serde_json::from_slice(&bytes)?;
        let pack = &value["pack"];
        if !pack.is_object() {
            bail!("pack.mcmeta 缺少 pack 配置");
        }
        source.description = plain_text(&pack["description"]);
        for (label, key) in [
            ("包格式", "pack_format"),
            ("最低格式", "min_format"),
            ("最高格式", "max_format"),
            ("支持的格式", "supported_formats"),
        ] {
            if let Some(v) = pack.get(key) {
                source.field(label, scalar(v).unwrap_or_else(|| v.to_string()));
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        source.issue = Some(error.to_string());
    }
    Ok(vec![source])
}
