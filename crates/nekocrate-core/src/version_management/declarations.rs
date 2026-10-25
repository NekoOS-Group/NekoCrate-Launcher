use super::*;
use crate::pack_content::Declaration;
use std::collections::HashMap;

fn enabled_name(name: &str) -> &str {
    name.strip_suffix(".disabled")
        .or_else(|| name.strip_suffix(".off"))
        .unwrap_or(name)
}

pub(super) fn merge(
    directory: &Path,
    kind: ContentKind,
    declarations: &[Declaration],
    content: &mut VersionContent,
) {
    let category = match kind {
        ContentKind::Mods => "mods",
        ContentKind::Resourcepacks => "resourcepacks",
        ContentKind::Datapacks => "datapacks",
        ContentKind::Unresolved => "unresolved",
        ContentKind::Saves => return,
    };
    let mut local: HashMap<_, _> = content
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| (enabled_name(&file.file_name).to_lowercase(), index))
        .collect();
    for declared in declarations.iter().filter(|file| file.kind == category) {
        let relative = match kind {
            ContentKind::Mods | ContentKind::Resourcepacks => declared
                .path
                .as_deref()
                .and_then(|path| path.split_once('/').map(|(_, tail)| tail.to_owned()))
                .unwrap_or_default(),
            ContentKind::Datapacks => declared.path.clone().unwrap_or_default(),
            ContentKind::Unresolved => format!("curseforge-{}", declared.key.replace(':', "-")),
            ContentKind::Saves => unreachable!(),
        };
        let found = local.get(&relative.to_lowercase()).copied();
        let present = found.map(|index| &content.files[index]);
        let complete = present.is_some_and(|file| declared.size == 0 || file.size == declared.size);
        let mut downloaded = present.map_or(0, |file| file.size);
        if !complete
            && declared.sha1.len() == 40
            && declared.sha1.bytes().all(|c| c.is_ascii_hexdigit())
        {
            let cache = directory
                .join(".nekocrate/downloads/sha1")
                .join(&declared.sha1[..2])
                .join(&declared.sha1);
            for path in [cache.clone(), cache.with_extension("part")] {
                downloaded = downloaded.max(fs::metadata(path).map_or(0, |meta| meta.len()));
            }
        }
        if declared.size > 0 {
            downloaded = downloaded.min(declared.size);
        }
        let status = if complete {
            "local"
        } else if !declared.selected {
            "skipped"
        } else if declared.path.is_none() {
            "unresolved"
        } else if downloaded > 0 {
            "partial"
        } else if declared.manual {
            "manual"
        } else {
            "pending"
        };
        let declaration = ContentDeclaration {
            key: declared.key.clone(),
            source: declared.source.clone(),
            project: declared.project,
            file: declared.file,
            path: declared.path.clone(),
            status,
            downloaded,
            total: declared.size,
            optional: declared.optional,
        };
        if let Some(index) = found {
            content.files[index].declaration = Some(declaration);
        } else {
            local.insert(relative.to_lowercase(), content.files.len());
            content.files.push(LocalContentFile {
                file_name: relative,
                name: declared.name.clone(),
                version: None,
                enabled: true,
                size: declared.size,
                mod_ids: vec![],
                declaration: Some(declaration),
            });
        }
    }
}
