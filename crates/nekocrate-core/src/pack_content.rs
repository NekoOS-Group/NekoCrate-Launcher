//! Declarations describe intended content; filesystem entries describe local content.
//! This index never performs provider requests or hashes archives for a UI listing.
use crate::{modpack, storage};
use anyhow::{Result, bail};
use serde_json::Value;
use std::{collections::HashSet, fs, io::Read, path::Path};

#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    pub key: String,
    pub name: String,
    pub file_name: Option<String>,
    pub path: Option<String>,
    pub kind: &'static str,
    pub source: String,
    pub project: Option<u64>,
    pub file: Option<u64>,
    pub size: u64,
    pub sha1: String,
    pub sha512: String,
    pub urls: Vec<String>,
    pub optional: bool,
    pub selected: bool,
    pub manual: bool,
}

pub(crate) fn kind(path: Option<&str>) -> &'static str {
    match path {
        Some(path) if path.starts_with("mods/") => "mods",
        Some(path) if path.starts_with("resourcepacks/") => "resourcepacks",
        Some(path)
            if path.starts_with("datapacks/")
                || (path.starts_with("saves/") && path.contains("/datapacks/")) =>
        {
            "datapacks"
        }
        Some(_) => "other",
        None => "unresolved",
    }
}

pub(crate) fn from_manifest(value: &Value) -> Vec<Declaration> {
    let value = value
        .get("manifest")
        .filter(|v| v.is_object())
        .unwrap_or(value);
    let source = if value["manifestType"] == "minecraftModpack" {
        "CurseForge"
    } else if value["game"] == "minecraft" {
        "Modrinth"
    } else {
        "MCBBS"
    };
    let mut seen = HashSet::new();
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            if file["env"]["client"] == "unsupported" {
                return None;
            }
            let mut path = file["path"].as_str().map(|path| {
                let normalized = path.replace('\\', "/");
                normalized
                    .strip_prefix("./")
                    .unwrap_or(&normalized)
                    .to_owned()
            });
            if path
                .as_ref()
                .is_some_and(|path| storage::safe_relative(path).is_err())
            {
                return None;
            }
            path = path.map(|path| {
                Path::new(&path)
                    .components()
                    .map(|p| p.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            });
            let project = file["projectID"].as_u64();
            let id = file["fileID"].as_u64();
            let key = match (project, id, &path) {
                (Some(project), Some(id), _) => format!("{project}:{id}"),
                (_, _, Some(path)) => path.clone(),
                _ => return None,
            };
            if !seen.insert(key.clone()) {
                return None;
            }
            let name = file["fileName"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    path.as_ref()
                        .and_then(|path| path.rsplit('/').next().map(str::to_owned))
                })
                .unwrap_or_else(|| format!("CurseForge #{}", project.unwrap_or_default()));
            Some(Declaration {
                key,
                name,
                file_name: file["fileName"].as_str().map(str::to_owned),
                kind: kind(path.as_deref()),
                path,
                source: source.into(),
                project,
                file: id,
                size: file["fileSize"].as_u64().unwrap_or(0),
                sha1: file["hashes"]["sha1"].as_str().unwrap_or_default().into(),
                sha512: file["hashes"]["sha512"].as_str().unwrap_or_default().into(),
                urls: file["downloads"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|url| url.as_str().map(str::to_owned))
                    .collect(),
                optional: file["required"] == false || file["env"]["client"] == "optional",
                selected: true,
                manual: false,
            })
        })
        .collect()
}

pub(crate) fn from_pack(pack: &modpack::Pack) -> Vec<Declaration> {
    let mut files = from_manifest(&pack.manifest);
    let chosen: std::collections::HashMap<_, _> =
        pack.files.iter().map(|file| (&file.key, file)).collect();
    for declaration in &mut files {
        let Some(file) = chosen.get(&declaration.key) else {
            declaration.selected = false;
            continue;
        };
        declaration.path = file.path.clone();
        declaration.kind = kind(file.path.as_deref());
        declaration.name = file
            .display_name
            .clone()
            .or_else(|| {
                file.path
                    .as_ref()
                    .and_then(|path| path.rsplit('/').next().map(str::to_owned))
            })
            .unwrap_or_else(|| declaration.name.clone());
        declaration.size = file.size;
        declaration.sha1 = file.sha1.clone();
        declaration.sha512 = file.sha512.clone();
        declaration.urls = file.urls.clone();
        declaration.manual =
            file.path.is_some() && file.urls.is_empty() && pack.format == "curseforge";
    }
    files
}

pub(crate) fn read_manifest(path: &Path) -> Result<Value> {
    let mut source = String::new();
    fs::File::open(path)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut source)?;
    if source.len() > 8 * 1024 * 1024 {
        bail!("元数据文件超过 8 MiB");
    }
    Ok(serde_json::from_str(&source)?)
}

pub(crate) fn load(game: &Path, version: &str, working: &Path) -> Result<Vec<Declaration>> {
    if let Some(pack) = modpack::content_pack(game, version)? {
        let mut files = from_pack(&pack);
        match_local_files(working, &mut files);
        return Ok(files);
    }
    let directory = game.join("versions").join(version);
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for location in [&directory, working] {
        for name in [
            "manifest.json",
            "modrinth.index.json",
            "modpack.cfg",
            "mcbbs.packmeta",
        ] {
            let path = location.join(name);
            if !path.is_file() {
                continue;
            }
            // Corrupt optional metadata must not hide files already installed locally.
            if let Ok(manifest) = read_manifest(&path) {
                files.extend(
                    from_manifest(&manifest)
                        .into_iter()
                        .filter(|file| seen.insert(file.key.clone())),
                );
            }
        }
    }
    match_local_files(working, &mut files);
    Ok(files)
}

/// Some launchers enrich CurseForge manifests with exact filenames. Locate those
/// files in known content directories before treating their IDs as unresolved.
/// A unique filename match identifies location/type, not cryptographic validity.
fn match_local_files(working: &Path, files: &mut [Declaration]) {
    use std::collections::HashMap;
    let wanted: HashSet<_> = files
        .iter()
        .filter(|file| file.path.is_none())
        .filter_map(|file| file.file_name.as_ref())
        .map(|name| name.to_lowercase())
        .collect();
    if wanted.is_empty() {
        return;
    }
    let entries = |path: &Path| {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect::<Vec<_>>()
    };
    let mut directories = vec![
        working.join("mods"),
        working.join("resourcepacks"),
        working.join("datapacks"),
    ];
    // Legacy Forge also supports version-specific Mod folders.
    directories.extend(
        entries(&working.join("mods"))
            .into_iter()
            .filter(|p| p.is_dir()),
    );
    for world in entries(&working.join("saves")) {
        if world.join("level.dat").is_file() || world.join("level.dat_old").is_file() {
            directories.push(world.join("datapacks"));
        }
    }
    let mut matched: HashMap<String, Option<String>> = HashMap::new();
    for directory in directories {
        for path in entries(&directory) {
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
            let name = name
                .strip_suffix(".disabled")
                .or_else(|| name.strip_suffix(".off"))
                .unwrap_or(&name);
            if !wanted.contains(name) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(working) else {
                continue;
            };
            let relative = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let expected = relative
                .strip_suffix(".disabled")
                .or_else(|| relative.strip_suffix(".off"))
                .unwrap_or(&relative)
                .to_owned();
            // Do not guess when the same filename occurs in multiple locations.
            matched
                .entry(name.into())
                .and_modify(|value| *value = None)
                .or_insert(Some(expected));
        }
    }
    for file in files.iter_mut().filter(|file| file.path.is_none()) {
        if let Some(Some(path)) = file
            .file_name
            .as_ref()
            .and_then(|name| matched.get(&name.to_lowercase()))
        {
            file.path = Some(path.clone());
            file.kind = kind(Some(path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_paths_deduplicates_and_keeps_unknown_types_unresolved() {
        let manifest = json!({"manifest":{"manifestType":"minecraftModpack","files":[
            {"projectID":1,"fileID":2,"fileName":"guess.jar"},
            {"projectID":1,"fileID":2},
            {"path":"./mods//example.jar"},
            {"path":"resourcepacks\\style.zip"},
            {"path":"saves/world/datapacks/rules.zip"},
            {"path":"config/rules.json"},
            {"path":"../outside.jar"},
            {"path":"/absolute.jar"},
            {"path":"mods/server.jar","env":{"client":"unsupported"}}
        ]}});
        let files = from_manifest(&manifest);
        assert_eq!(files.len(), 5);
        assert_eq!(
            files[0].kind, "unresolved",
            "A filename alone does not prove project type"
        );
        assert_eq!(files[1].path.as_deref(), Some("mods/example.jar"));
        assert_eq!(
            files.iter().map(|f| f.kind).collect::<Vec<_>>(),
            ["unresolved", "mods", "resourcepacks", "datapacks", "other"]
        );
    }

    #[test]
    fn ambiguous_or_absent_filenames_do_not_claim_a_local_match() {
        let temp = tempfile::tempdir().unwrap();
        for folder in ["mods", "resourcepacks"] {
            fs::create_dir(temp.path().join(folder)).unwrap();
            fs::write(temp.path().join(folder).join("same.zip"), b"file").unwrap();
        }
        let mut files = from_manifest(&json!({"files":[
            {"projectID":1,"fileID":2,"fileName":"same.zip"},
            {"projectID":3,"fileID":4,"fileName":"missing.jar"},
            {"projectID":5,"fileID":6}
        ]}));
        match_local_files(temp.path(), &mut files);
        assert!(
            files
                .iter()
                .all(|file| file.kind == "unresolved" && file.path.is_none())
        );
    }
}
