//! Read metadata, never load Mod classes. Legacy mcmod.info's mcversion is a
//! build hint, not @Mod.acceptedMinecraftVersions, so it is not a hard bound.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone)]
pub(super) struct Dependency {
    pub id: String,
    pub range: Value,
    pub fabric: bool,
    pub conflict: bool,
}
pub(super) struct Component {
    pub id: String,
    pub version: Option<String>,
    pub aliases: Vec<String>,
    pub dependencies: Vec<Dependency>,
    pub nested: bool,
}
pub(super) struct ModFile {
    pub path: String,
    pub enabled: bool,
    pub components: Vec<Component>,
}
#[derive(Default)]
pub(super) struct Inventory {
    pub files: Vec<ModFile>,
    pub incomplete: Vec<String>,
    pub corrupt: Vec<String>,
    pub identities_complete: bool,
}
fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && !s.contains("${"))
        .map(str::to_owned)
}
fn metadata(
    path: &Path,
    loader: &str,
    legacy: bool,
    runtime: &BTreeMap<String, String>,
) -> Result<(Vec<Component>, bool)> {
    let metadata = crate::mod_metadata::read(path);
    metadata.check()?;
    if loader == "fabric" && metadata.has("fabric.mod.json") {
        let mods = crate::resources::fabric_mod_metadata(
            path,
            &tokio_util::sync::CancellationToken::new(),
        )?;
        return Ok((
            mods.into_iter()
                .enumerate()
                .map(|(index, m)| {
                    let mut dependencies = Vec::new();
                    for (map, conflict) in [(m.depends, false), (m.breaks, true)] {
                        for (id, range) in map {
                            dependencies.push(Dependency {
                                id,
                                range,
                                conflict,
                                fabric: true,
                            });
                        }
                    }
                    Component {
                        id: m.id,
                        version: Some(m.version),
                        aliases: m.provides,
                        dependencies,
                        nested: index > 0,
                    }
                })
                .collect(),
            true,
        ));
    }
    if loader == "fabric" {
        // Fabric ignores ordinary / Forge-only jars without fabric.mod.json.
        return Ok((vec![], true));
    }
    let descriptor = if loader == "neoforge" && metadata.has("META-INF/neoforge.mods.toml") {
        "META-INF/neoforge.mods.toml"
    } else if !legacy
        && matches!(loader, "forge" | "neoforge")
        && metadata.has("META-INF/mods.toml")
    {
        "META-INF/mods.toml"
    } else {
        "mcmod.info"
    };
    let Some(value) = metadata.value(descriptor)? else {
        return Ok((vec![], false));
    };
    let modern = descriptor.ends_with(".toml");
    let infos = crate::mod_metadata::infos(descriptor, value)?;
    let mut components = Vec::new();
    for info in infos {
        let id = text(&info[if modern { "modId" } else { "modid" }]).context("缺少 Mod ID")?;
        let version = runtime
            .get(&id)
            .cloned()
            .or_else(|| modern.then(|| text(&info["version"])).flatten());
        let mut dependencies = Vec::new();
        if modern {
            for dep in value["dependencies"][&id].as_array().into_iter().flatten() {
                if dep["side"] == "SERVER" {
                    continue;
                }
                let conflict = dep["type"] == "incompatible";
                if !conflict
                    && (dep["mandatory"] == false
                        || dep["type"] == "optional"
                        || dep["type"] == "discouraged")
                {
                    continue;
                }
                let Some(id) = text(&dep["modId"]) else {
                    continue;
                };
                dependencies.push(Dependency {
                    id,
                    range: dep["versionRange"].clone(),
                    conflict,
                    fabric: false,
                });
            }
            if matches!(value["modLoader"].as_str(), Some("javafml" | "lowcodefml")) {
                dependencies.push(Dependency {
                    id: if loader == "neoforge" {
                        "@fml"
                    } else {
                        "forge"
                    }
                    .into(),
                    range: value["loaderVersion"].clone(),
                    conflict: false,
                    fabric: false,
                });
            }
        } else if info["useDependencyInformation"] == true {
            // The ordinary dependencies list only controls load ordering.
            for dep in info["requiredMods"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let dep = dep
                    .strip_prefix("required-after:")
                    .or_else(|| dep.strip_prefix("required-before:"))
                    .unwrap_or(dep);
                let (id, range) = dep.split_once('@').unwrap_or((dep, ""));
                dependencies.push(Dependency {
                    id: id.into(),
                    range: Value::String(range.into()),
                    conflict: false,
                    fabric: false,
                });
            }
        }
        components.push(Component {
            id,
            version,
            dependencies,
            aliases: vec![],
            nested: false,
        });
    }
    // Nested Forge jars and annotation-only old Forge Mods need the loader's
    // actual discovery table to establish missing IDs. Never guess absence.
    let complete = modern && !metadata.nested_forge;
    Ok((components, complete))
}

pub(super) fn runtime_components(text: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let cells: Vec<_> = line.trim().split('|').map(str::trim).collect();
        if cells.len() >= 6
            && cells[0].is_empty()
            && !cells[1].is_empty()
            && cells[1].bytes().all(|b| b"ULCHIJADE".contains(&b))
            && !cells[1].contains('D')
            && !cells[2].is_empty()
            && !cells[3].is_empty()
        {
            result.insert(cells[2].into(), cells[3].into());
        }
    }
    result
}

pub(super) fn scan(
    working: &Path,
    minecraft: &str,
    loader: &str,
    runtime: &BTreeMap<String, String>,
) -> Inventory {
    let mut result = Inventory {
        identities_complete: true,
        ..Inventory::default()
    };
    let mut parts = minecraft.split('.');
    let legacy = loader == "forge"
        && parts.next() == Some("1")
        && parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|minor| minor <= 12);
    let mut directories = vec![working.join("mods")];
    if legacy && crate::storage::safe_version(minecraft).is_ok() {
        directories.push(working.join("mods").join(minecraft));
    }
    for directory in directories {
        if !directory.exists() {
            continue;
        }
        if fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
            result.incomplete.push("跳过链接的 Mods 目录".into());
            result.identities_complete = false;
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) => {
                result.incomplete.push(format!("无法读取 Mods 目录：{e}"));
                result.identities_complete = false;
                continue;
            }
        };
        for (index, entry) in entries.enumerate() {
            if index >= 4096 {
                result.incomplete.push("Mod 文件数超过检查上限".into());
                result.identities_complete = false;
                break;
            }
            let Ok(entry) = entry else {
                result.identities_complete = false;
                continue;
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let base = name
                .strip_suffix(".disabled")
                .or_else(|| name.strip_suffix(".off"))
                .unwrap_or(&name);
            if name.starts_with('.') || !base.ends_with(".jar") {
                continue;
            }
            if !entry.file_type().is_ok_and(|m| m.is_file()) {
                result.identities_complete = false;
                continue;
            }
            let enabled = name == base;
            let relative = path
                .strip_prefix(working)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let mut components = vec![];
            match metadata(&path, loader, legacy, runtime) {
                Ok((found, complete)) => {
                    if enabled && !complete {
                        result.identities_complete = false;
                    }
                    components = found;
                }
                Err(e) => {
                    if enabled {
                        result.identities_complete = false;
                        if fs::File::open(&path)
                            .ok()
                            .is_some_and(|file| zip::ZipArchive::new(file).is_err())
                        {
                            result.corrupt.push(format!("{relative}：{e:#}"));
                        } else {
                            result.incomplete.push(format!("{relative}：{e:#}"));
                        }
                    }
                }
            }
            result.files.push(ModFile {
                path: relative,
                enabled,
                components,
            });
        }
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result
}
