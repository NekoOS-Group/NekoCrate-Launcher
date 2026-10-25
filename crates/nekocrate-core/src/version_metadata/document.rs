//! Interpret each source independently: inherited or inferred values must not
//! appear as declarations of the child file. This reader never resolves URLs.
use super::{MetadataDocument, MetadataEntry, MetadataField, MetadataSection};
use anyhow::{Result, bail};
use serde_json::Value;
use std::{fs, io::Read, path::PathBuf};

#[cfg(test)]
mod tests;

pub(super) fn empty(path: PathBuf, id: String, kind: &str) -> MetadataDocument {
    MetadataDocument {
        id,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        kind: kind.into(),
        path,
        json: None,
        fields: Vec::new(),
        sections: Vec::new(),
        issue: None,
    }
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn field(document: &mut MetadataDocument, label: &str, value: impl Into<String>) {
    document.fields.push(MetadataField {
        label: label.into(),
        value: value.into(),
    });
}

fn fields(document: &mut MetadataDocument, value: &Value, keys: &[(&str, &str)]) {
    for (label, pointer) in keys {
        if let Some(value) = value.pointer(pointer).and_then(text) {
            field(document, label, value);
        }
    }
}

fn section(
    document: &mut MetadataDocument,
    label: &str,
    entries: Vec<MetadataEntry>,
    target: Option<&'static str>,
    note: Option<&str>,
) {
    if entries.is_empty() {
        return;
    }
    document.sections.push(MetadataSection {
        label: label.into(),
        target,
        note: note.map(str::to_owned),
        entries,
    });
}

pub(super) fn parse_profile(document: &mut MetadataDocument, value: &Value) {
    fields(
        document,
        value,
        &[
            ("版本 ID", "/id"),
            ("继承自", "/inheritsFrom"),
            ("Java 版本", "/javaVersion/majorVersion"),
            ("Java 组件", "/javaVersion/component"),
            ("启动入口", "/mainClass"),
            ("客户端版本", "/jar"),
            ("资源索引", "/assetIndex/id"),
            ("发布时间", "/releaseTime"),
        ],
    );
    if value["inheritsFrom"].as_str().is_none()
        && let Ok(resolved) = crate::version::resolve_chain(std::slice::from_ref(value))
    {
        field(document, "游戏版本", resolved.minecraft_version);
        if value["javaVersion"]["majorVersion"].is_null()
            && let Some(java) = resolved.version.java_version
        {
            field(document, "Java 版本（推断）", java.major.to_string());
        }
    }
    if let Some(loader) = crate::version::detect_loader(value) {
        let name = match loader.kind.as_str() {
            "forge" => "Forge",
            "neoforge" => "NeoForge",
            "fabric" => "Fabric",
            "quilt" => "Quilt",
            other => other,
        };
        field(document, "Mod 加载器", format!("{name} {}", loader.version));
    }
    if let Some(libraries) = value["libraries"].as_array() {
        section(
            document,
            "运行依赖",
            libraries
                .iter()
                .filter_map(|library| {
                    Some(MetadataEntry {
                        name: library["name"].as_str()?.into(),
                        detail: text(&library["downloads"]["artifact"]["path"]),
                    })
                })
                .collect(),
            Some("libraries"),
            None,
        );
    }
}

pub(super) fn read_pack(path: PathBuf) -> MetadataDocument {
    let mut document = empty(path, String::new(), "整合包清单");
    let result = (|| -> Result<()> {
        let mut source = String::new();
        fs::File::open(&document.path)?
            .take(8 * 1024 * 1024 + 1)
            .read_to_string(&mut source)?;
        if source.len() > 8 * 1024 * 1024 {
            bail!("元数据文件超过 8 MiB");
        }
        document.json = Some(source.clone());
        let value: Value = serde_json::from_str(&source)?;
        if !value.is_object() {
            bail!("元数据文件必须是 JSON 对象");
        }
        if document.name == "hmclversion.cfg" {
            document.kind = "HMCL 版本设置".into();
            fields(
                &mut document,
                &value,
                &[
                    ("Java 选择", "/java"),
                    ("Java 路径", "/defaultJavaPath"),
                    ("最大内存（MB）", "/maxMemory"),
                    ("运行目录", "/gameDir"),
                ],
            );
            if let Some(global) = value["usesGlobal"].as_bool() {
                field(&mut document, "使用全局配置", global.to_string());
            }
        } else {
            // HMCL retains the original manifest inside modpack.cfg.
            let manifest = value
                .get("manifest")
                .filter(|v| v.is_object())
                .unwrap_or(&value);
            parse_pack(&mut document, manifest);
            if document.name == "modpack.cfg" {
                document.kind = format!("HMCL · {}", document.kind);
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        document.issue = Some(error.to_string());
    }
    document
}

pub(super) fn cached_pack(path: PathBuf, manifest: &Value) -> MetadataDocument {
    let mut document = empty(path, String::new(), "整合包清单");
    document.json = Some(manifest.to_string());
    parse_pack(&mut document, manifest);
    document
}

fn parse_pack(document: &mut MetadataDocument, value: &Value) {
    fields(
        document,
        value,
        &[
            ("整合包名称", "/name"),
            ("整合包版本", "/version"),
            ("整合包版本 ID", "/versionId"),
            ("作者", "/author"),
            ("简介", "/summary"),
            ("最低内存（MB）", "/launchInfo/minMemory"),
            ("Java 版本", "/launchInfo/javaVersion"),
        ],
    );
    let curseforge = value["manifestType"].as_str() == Some("minecraftModpack");
    let modrinth = value["game"].as_str() == Some("minecraft") && value["dependencies"].is_object();
    if curseforge {
        document.kind = "CurseForge".into();
        fields(document, value, &[("游戏版本", "/minecraft/version")]);
        if let Some(loaders) = value["minecraft"]["modLoaders"].as_array() {
            for loader in loaders {
                if let Some(id) = loader["id"].as_str() {
                    field(document, "Mod 加载器", id);
                }
            }
        }
    } else if modrinth {
        document.kind = "Modrinth".into();
        fields(document, value, &[("游戏版本", "/dependencies/minecraft")]);
        for (name, version) in value["dependencies"].as_object().unwrap() {
            if name != "minecraft"
                && let Some(version) = text(version)
            {
                field(document, "依赖组件", format!("{name} {version}"));
            }
        }
    } else if let Some(addons) = value["addons"].as_array() {
        document.kind = "MCBBS".into();
        for addon in addons {
            if let (Some(id), Some(version)) = (addon["id"].as_str(), text(&addon["version"])) {
                field(
                    document,
                    if id == "game" {
                        "游戏版本"
                    } else {
                        "依赖组件"
                    },
                    if id == "game" {
                        version
                    } else {
                        format!("{id} {version}")
                    },
                );
            }
        }
    }
    if matches!(document.kind.as_str(), "CurseForge" | "Modrinth" | "MCBBS") {
        document.fields.insert(
            0,
            MetadataField {
                label: "清单格式".into(),
                value: document.kind.clone(),
            },
        );
    }
    apply_declaration_sections(document, &crate::pack_content::from_manifest(value));
}

pub(super) fn enrich_declarations(
    document: &mut MetadataDocument,
    declarations: &[crate::pack_content::Declaration],
) {
    let Some(value) = document
        .json
        .as_ref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
    else {
        return;
    };
    let declared = crate::pack_content::from_manifest(&value);
    if declared.is_empty() {
        return;
    }
    let lookup: std::collections::HashMap<_, _> =
        declarations.iter().map(|file| (&file.key, file)).collect();
    let files: Vec<_> = declared
        .iter()
        .map(|file| lookup.get(&file.key).copied().unwrap_or(file).clone())
        .collect();
    apply_declaration_sections(document, &files);
}

fn apply_declaration_sections(
    document: &mut MetadataDocument,
    files: &[crate::pack_content::Declaration],
) {
    document.sections.retain(|section| {
        ![
            "声明的 Mods",
            "声明的资源包",
            "声明的数据包",
            "待解析文件",
            "其他文件",
            "Mod / 资源声明",
        ]
        .contains(&section.label.as_str())
    });
    for (kind, label, target) in [
        ("mods", "声明的 Mods", Some("mods")),
        ("resourcepacks", "声明的资源包", Some("resourcepacks")),
        ("datapacks", "声明的数据包", Some("datapacks")),
        ("unresolved", "待解析文件", Some("unresolved")),
        ("other", "其他文件", None),
    ] {
        let entries: Vec<_> = files
            .iter()
            .filter(|file| file.kind == kind)
            .map(|file| MetadataEntry {
                name: file.name.clone(),
                detail: file.path.clone().or_else(|| {
                    Some(format!(
                        "Project ID: {} · File ID: {}",
                        file.project?, file.file?
                    ))
                }),
            })
            .collect();
        if !entries.is_empty() {
            section(document, label, entries, target, None);
        }
    }
}
