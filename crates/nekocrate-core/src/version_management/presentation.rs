//! Small, read-only presentation data for visible local resources. No network or extraction.
use super::ContentKind;
use crate::{Launcher, resources::Platform, storage};
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::{fs, io::Read, path::Path};

const ICON_LIMIT: u64 = 512 * 1024;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize)]
pub struct ContentProject {
    pub platform: Platform,
    pub id: String,
}
#[derive(Default, Debug)]
pub struct ContentPresentation {
    pub icon: Option<(Vec<u8>, &'static str)>,
    pub project: Option<ContentProject>,
}

fn project_link(value: &str) -> Option<ContentProject> {
    let url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (platform, id) = match (url.host_str()?, segments.as_slice()) {
        ("modrinth.com" | "www.modrinth.com", ["mod" | "project", id, ..]) => {
            (Platform::Modrinth, *id)
        }
        ("curseforge.com" | "www.curseforge.com", ["minecraft", "mc-mods", id, ..]) => {
            (Platform::Curseforge, *id)
        }
        _ => return None,
    };
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    Some(ContentProject {
        platform,
        id: id.into(),
    })
}

fn entry(archive: &mut zip::ZipArchive<fs::File>, name: &str, limit: u64) -> Option<Vec<u8>> {
    storage::safe_relative(name).ok()?;
    crate::mod_metadata::entry(archive, name, limit)
        .ok()
        .flatten()
}

fn icon_name(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| {
        // Fabric/Quilt size maps: choose the nearest size at or above 64 px.
        let map = value.as_object()?;
        map.iter()
            .filter_map(|(size, path)| Some((size.parse::<u32>().ok()?, path.as_str()?)))
            .min_by_key(|(size, _)| {
                if *size >= 64 {
                    *size - 64
                } else {
                    10_000 - *size
                }
            })
            .map(|(_, path)| path)
    })
}

fn from_archive(path: &Path, kind: ContentKind) -> ContentPresentation {
    let mut result = ContentPresentation::default();
    let Ok(file) = fs::File::open(path) else {
        return result;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return result;
    };
    let mut icons = Vec::new();
    if matches!(kind, ContentKind::Mods) {
        let metadata = crate::mod_metadata::read(path);
        for document in &metadata.documents {
            let Ok(value) = &document.value else { continue };
            let Ok(infos) = crate::mod_metadata::infos(document.name, value) else {
                continue;
            };
            for info in infos {
                let info = if document.name == "quilt.mod.json" {
                    &info["metadata"]
                } else {
                    info
                };
                if let Some(name) = icon_name(&info["icon"])
                    .or_else(|| info["logoFile"].as_str())
                    .or_else(|| value["logoFile"].as_str())
                {
                    icons.push(name.to_owned());
                }
                if result.project.is_none() {
                    result.project = [
                        &info["contact"]["homepage"],
                        &info["displayURL"],
                        &info["url"],
                        &info["contact"]["sources"],
                    ]
                    .into_iter()
                    .filter_map(Value::as_str)
                    .find_map(project_link);
                }
            }
        }
    } else if matches!(
        kind,
        ContentKind::Resourcepacks | ContentKind::Shaderpacks | ContentKind::Datapacks
    ) {
        icons.push("pack.png".into());
    }
    result.icon = icons.iter().take(32).find_map(|name| {
        let bytes = entry(&mut archive, name, ICON_LIMIT)?;
        let mime = crate::resources::image_type(&bytes).ok()?;
        Some((bytes, mime))
    });
    result
}

fn directory_icon(path: &Path, kind: ContentKind) -> Option<(Vec<u8>, &'static str)> {
    if !matches!(
        kind,
        ContentKind::Resourcepacks | ContentKind::Shaderpacks | ContentKind::Datapacks
    ) {
        return None;
    }
    let root = path.canonicalize().ok()?;
    let icon = path.join("pack.png").canonicalize().ok()?;
    if !icon.starts_with(&root) {
        return None;
    }
    let mut file = fs::File::open(icon).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > ICON_LIMIT {
        return None;
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(ICON_LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > ICON_LIMIT {
        return None;
    }
    let mime = crate::resources::image_type(&bytes).ok()?;
    Some((bytes, mime))
}

impl Launcher {
    pub fn version_content_presentation(
        &self,
        id: &str,
        version: &str,
        kind: ContentKind,
        file: &str,
    ) -> Result<ContentPresentation> {
        let path = self.version_content_path(id, version, kind, file, None)?;
        let (working, _) = self.content_location(id, version)?;
        content_presentation(&path, &working, kind)
    }
}

pub(crate) fn content_presentation(
    path: &Path,
    working: &Path,
    kind: ContentKind,
) -> Result<ContentPresentation> {
    let mut result = if path.is_file() {
        from_archive(path, kind)
    } else {
        ContentPresentation {
            icon: directory_icon(path, kind),
            ..Default::default()
        }
    };
    if let Ok(relative) = path.strip_prefix(working.canonicalize()?)
        && let Some(record) =
            crate::resources::records(working)?.get(&relative.to_string_lossy().replace('\\', "/"))
        && record.origin.platform != Platform::Local
        && let Some(project) = &record.origin.project
    {
        result.project = Some(ContentProject {
            platform: record.origin.platform,
            id: project.clone(),
        });
    }
    Ok(result)
}
