//! Inspect known pack documents only; never traverse assets or execute shader code.
use super::*;

const PROPERTIES: &str = "shaders/shaders.properties";
const README_NAMES: &[&str] = &[
    "README.md",
    "readme.md",
    "Readme.md",
    "ReadMe.md",
    "README.MD",
    "README.txt",
    "readme.txt",
    "Readme.txt",
    "ReadMe.txt",
    "README.TXT",
    "README",
    "readme",
    "Readme",
    "ReadMe",
];

pub(super) fn allowed(kind: ContentKind, source: &str) -> bool {
    source == "pack.mcmeta"
        || (kind == ContentKind::Shaderpacks && source == PROPERTIES)
        || README_NAMES.contains(&source)
        || (kind == ContentKind::Shaderpacks
            && source
                .strip_prefix("shaders/")
                .is_some_and(|name| README_NAMES.contains(&name)))
}

fn documents(kind: ContentKind, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut result = Vec::new();
    for name in ["pack.mcmeta", PROPERTIES] {
        if allowed(kind, name) && exists(name) {
            result.push(name.into());
        }
    }
    // Prefer Markdown and the root README. Avoid displaying the same instructions twice.
    let readme = README_NAMES
        .iter()
        .map(|s| s.to_string())
        .chain(
            README_NAMES
                .iter()
                .filter(|_| kind == ContentKind::Shaderpacks)
                .map(|s| format!("shaders/{s}")),
        )
        .find(|name| exists(name));
    result.extend(readme);
    result
}

pub(super) fn available(path: &Path, kind: ContentKind) -> bool {
    if path.is_dir() {
        !documents(kind, |name| path.join(name).is_file()).is_empty()
    } else {
        fs::File::open(path)
            .ok()
            .and_then(|file| zip::ZipArchive::new(file).ok())
            .is_some_and(|archive| {
                !documents(kind, |name| archive.index_for_name(name).is_some()).is_empty()
            })
    }
}

pub(super) fn read(path: &Path, kind: ContentKind) -> Result<Vec<ContentSource>> {
    let mut archive = if path.is_dir() {
        None
    } else {
        Some(zip::ZipArchive::new(fs::File::open(path)?)?)
    };
    let names = documents(kind, |name| {
        archive.as_ref().map_or_else(
            || path.join(name).is_file(),
            |a| a.index_for_name(name).is_some(),
        )
    });
    let mut sources = Vec::new();
    for name in names {
        let mut source = ContentSource::new(&name);
        let parsed = (|| -> Result<()> {
            let bytes = if let Some(archive) = &mut archive {
                let entry = archive.by_name(&name)?;
                anyhow::ensure!(entry.size() <= TEXT_LIMIT, "元数据内容过大");
                bounded(entry, TEXT_LIMIT)?
            } else {
                let file = fs::File::open(contained(path, &path.join(&name))?)?;
                anyhow::ensure!(file.metadata()?.is_file(), "未找到元数据文件");
                bounded(file, TEXT_LIMIT)?
            };
            if name == "pack.mcmeta" {
                parse_pack(&mut source, &bytes)?;
            } else {
                let text = std::str::from_utf8(&bytes)?.trim_start_matches('\u{feff}');
                if name == PROPERTIES {
                    shader_properties(&mut source, text);
                } else if !text.trim().is_empty() {
                    source.description = Some(text.trim().into());
                    if name.to_ascii_lowercase().ends_with(".md") {
                        source.description_format = Some("markdown");
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = parsed {
            source.issue = Some(error.to_string());
        }
        sources.push(source);
    }
    Ok(sources)
}

// Only summarize declared profiles and feature flags. Do not infer MC/OptiFine/Iris
// compatibility or evaluate conditional shader preprocessor directives.
// https://github.com/IrisShaders/docs/blob/main/src/content/docs/current/Reference/Shaders.Properties/overview.mdx
fn shader_properties(source: &mut ContentSource, text: &str) {
    let mut profiles = Vec::new();
    let mut required = Vec::new();
    let mut optional = Vec::new();
    let mut logical = String::new();
    for line in text.lines() {
        let line = line.trim_start();
        if logical.is_empty() && (line.is_empty() || line.starts_with(['#', '!'])) {
            continue;
        }
        logical.push_str(line);
        if logical.bytes().rev().take_while(|c| *c == b'\\').count() % 2 == 1 {
            logical.pop();
            continue;
        }
        let pair = logical.split_once(['=', ':', ' ', '\t']);
        if let Some((key, value)) = pair {
            let value = value
                .trim_start()
                .trim_start_matches(['=', ':'])
                .trim_start();
            if let Some(name) = key.strip_prefix("profile.").filter(|s| !s.is_empty()) {
                push_unique(&mut profiles, name);
            } else if matches!(key, "iris.features.required" | "iris.features.optional") {
                let values = if key.ends_with("required") {
                    &mut required
                } else {
                    &mut optional
                };
                for value in value.split_whitespace() {
                    push_unique(values, value);
                }
            }
        }
        logical.clear();
    }
    for (label, values) in [
        ("配置档位", profiles),
        ("声明的 Iris 必需功能", required),
        ("声明的 Iris 可选功能", optional),
    ] {
        if !values.is_empty() {
            source.field(label, values.join(" · "));
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    // Guard against malformed packs producing huge unbroken UI fields.
    if values.len() < 128 && value.len() <= 128 && !values.iter().any(|v| v == value) {
        values.push(value.into());
    }
}
