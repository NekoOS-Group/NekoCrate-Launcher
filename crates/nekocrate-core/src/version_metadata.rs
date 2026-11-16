//! Read-only inspection of a tracked profile and its own storage, without a game scan.
use crate::{Launcher, model::ModLoader, storage, version};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, fs, io::Read, path::PathBuf};

mod document;

#[derive(Debug, Serialize)]
pub struct MetadataField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct MetadataEntry {
    pub name: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetadataSection {
    pub label: String,
    pub target: Option<&'static str>,
    pub note: Option<String>,
    pub entries: Vec<MetadataEntry>,
}

#[derive(Debug, Serialize)]
pub struct MetadataDocument {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: PathBuf,
    pub json: Option<String>,
    pub fields: Vec<MetadataField>,
    pub sections: Vec<MetadataSection>,
    pub issue: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VersionMetadata {
    pub directory: PathBuf,
    pub game_directory: PathBuf,
    pub working_directory: Option<PathBuf>,
    pub minecraft_version: String,
    pub loader: Option<ModLoader>,
    pub main_class: Option<String>,
    pub java_major: Option<u32>,
    pub documents: Vec<MetadataDocument>,
    pub libraries: Vec<MetadataEntry>,
    pub issue: Option<String>,
}

impl Launcher {
    fn inspected_version_root(&self, id: &str, version_id: &str) -> Result<PathBuf> {
        storage::safe_version(version_id)?;
        if !self
            .instance(id)?
            .versions
            .iter()
            .any(|v| v.id == version_id)
        {
            bail!("此实例未添加该版本");
        }
        Ok(self
            .available_game_dir(id)?
            .join("versions")
            .join(version_id))
    }

    pub fn version_metadata(&self, id: &str, version_id: &str) -> Result<VersionMetadata> {
        let directory = self.inspected_version_root(id, version_id)?;
        let game = self.available_game_dir(id)?;
        let instance = self.instance(id)?;
        let tracked = instance
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .context("此实例未添加该版本")?;
        let mut documents = Vec::new();
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = version_id.to_owned();
        let pending_pack = tracked.modpack
            && !tracked.installed
            && !version::metadata_path(&game, version_id)?.exists();
        let pending_loader = crate::loader::has_record(&game, version_id)
            && !version::metadata_path(&game, version_id)?.exists();
        let mut issue = None;
        loop {
            if pending_pack || pending_loader {
                break;
            }
            if chain.len() >= 32 || !seen.insert(current.clone()) {
                issue = Some("版本继承存在循环或层数过多".into());
                break;
            }
            let path = match version::metadata_path(&game, &current) {
                Ok(path) => path,
                Err(error) => {
                    issue = Some(error.to_string());
                    break;
                }
            };
            let mut document = document::empty(
                path.clone(),
                current.clone(),
                if current == version_id {
                    "版本配置"
                } else {
                    "继承版本配置"
                },
            );
            // Keep unknown loader fields in the original document. Bound memory even
            // for corrupt/partially downloaded profiles; never read JAR contents here.
            let value = (|| -> Result<Value> {
                let mut text = String::new();
                fs::File::open(&path)?
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_string(&mut text)?;
                if text.len() > 8 * 1024 * 1024 {
                    bail!("版本配置超过 8 MiB");
                }
                document.json = Some(text.clone());
                let value: Value = serde_json::from_str(&text)?;
                if value["id"].as_str() != Some(&current) {
                    bail!("版本元数据与所选版本不一致");
                }
                Ok(value)
            })();
            match value {
                Ok(value) => {
                    document::parse_profile(&mut document, &value);
                    let parent = value["inheritsFrom"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned);
                    documents.push(document);
                    chain.push(value);
                    let Some(parent) = parent else { break };
                    current = parent;
                }
                Err(error) => {
                    document.issue = Some(error.to_string());
                    documents.push(document);
                    break;
                }
            }
        }
        chain.reverse();
        let resolved = if !pending_pack
            && !pending_loader
            && issue.is_none()
            && documents.iter().all(|d| d.issue.is_none())
        {
            match version::resolve_chain(&chain) {
                Ok(value) => Some(value),
                Err(error) => {
                    issue = Some(error.to_string());
                    None
                }
            }
        } else {
            None
        };
        let working_directory = resolved
            .as_ref()
            .and_then(|resolved| {
                match version::instance_working_directory(&game, &instance, &resolved.version.id) {
                    Ok(path) => Some(path),
                    Err(error) => {
                        issue = Some(error.to_string());
                        None
                    }
                }
            })
            .or_else(|| {
                (pending_pack || pending_loader)
                    .then(|| version::instance_working_directory(&game, &instance, version_id).ok())
                    .flatten()
            });
        // Only inspect known metadata files beside this profile and in its actual
        // working directory. Do not recurse into mods/saves or scan other versions.
        let mut locations = vec![directory.clone()];
        if let Some(working) = &working_directory
            && !locations.contains(working)
        {
            locations.push(working.clone());
        }
        let mut paths: HashSet<_> = documents.iter().map(|d| d.path.clone()).collect();
        for location in locations {
            for name in [
                "manifest.json",
                "modrinth.index.json",
                "modpack.cfg",
                "mcbbs.packmeta",
                "hmclversion.cfg",
            ] {
                let path = location.join(name);
                if path.is_file() && paths.insert(path.clone()) {
                    documents.push(document::read_pack(path));
                }
            }
        }
        if crate::loader::has_record(&game, version_id) {
            let path = crate::loader::record_path(&game, version_id)?;
            let mut doc = document::empty(path.clone(), "loader-install".into(), "加载器安装记录");
            match crate::loader::read_record(&game, version_id) {
                Ok(record) => {
                    doc.json = Some(serde_json::to_string_pretty(&record)?);
                    doc.fields = vec![
                        MetadataField {
                            label: "游戏版本".into(),
                            value: record.minecraft,
                        },
                        MetadataField {
                            label: "加载器".into(),
                            value: format!("{} {}", record.loader.kind, record.loader.version),
                        },
                        MetadataField {
                            label: "状态".into(),
                            value: if record.completed {
                                "已安装"
                            } else {
                                "待下载"
                            }
                            .into(),
                        },
                    ];
                }
                Err(error) => doc.issue = Some(error.to_string()),
            }
            documents.push(doc);
        }
        if let Ok(declarations) = crate::pack_content::load(
            &game,
            version_id,
            working_directory.as_deref().unwrap_or(&directory),
        ) {
            for document in &mut documents {
                document::enrich_declarations(document, &declarations);
            }
        }
        Ok(VersionMetadata {
            directory,
            game_directory: game,
            working_directory,
            minecraft_version: resolved
                .as_ref()
                .map(|r| r.minecraft_version.clone())
                .or_else(|| tracked.minecraft_version.clone())
                .unwrap_or_default(),
            loader: resolved
                .as_ref()
                .and_then(|r| r.loader.clone())
                .or_else(|| tracked.loader.clone()),
            main_class: resolved.as_ref().map(|r| r.version.main_class.clone()),
            java_major: resolved
                .as_ref()
                .and_then(|r| r.version.java_version.as_ref().map(|j| j.major)),
            documents,
            libraries: resolved
                .as_ref()
                .map(|resolved| {
                    resolved
                        .version
                        .libraries
                        .iter()
                        .map(|library| MetadataEntry {
                            name: library.name.clone(),
                            detail: library
                                .downloads
                                .artifact
                                .as_ref()
                                .and_then(|a| a.path.clone()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            issue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn runtime_library_page_and_count_use_the_same_resolved_inheritance() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pack", "child").unwrap();
        let game = instance.game_dir(launcher.root());
        let mut parent = serde_json::to_value(crate::test_http::version_fixture("1.20.1")).unwrap();
        parent["libraries"] = json!([
            {"name":"org.example:common:1"},
            {"name":"org.example:base:1"}
        ]);
        storage::write_json(&version::metadata_path(&game, "1.20.1").unwrap(), &parent).unwrap();
        storage::write_json(
            &version::metadata_path(&game, "child").unwrap(),
            &json!({
                "id":"child", "inheritsFrom":"1.20.1", "libraries":[
                    {"name":"org.example:common:2"},
                    {"name":"org.example:child:1"},
                    {"name":"org.example:server:1","clientreq":false}
                ]
            }),
        )
        .unwrap();
        let metadata = launcher.version_metadata(&instance.id, "child").unwrap();
        assert_eq!(
            metadata
                .libraries
                .iter()
                .map(|library| library.name.as_str())
                .collect::<Vec<_>>(),
            [
                "org.example:base:1",
                "org.example:common:2",
                "org.example:child:1"
            ]
        );
        assert!(
            metadata
                .documents
                .iter()
                .all(|document| document.sections[0].target == Some("libraries"))
        );
        assert_eq!(
            metadata.libraries[1].detail.as_deref(),
            Some("org/example/common/2/common-2.jar")
        );
        assert_eq!(
            launcher
                .version_content_counts(&instance.id, "child")
                .unwrap()
                .libraries,
            metadata.libraries.len()
        );
    }

    #[test]
    fn reads_local_inherited_source_without_rewriting_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pack", "child").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let path = version::metadata_path(&game, "child").unwrap();
        let source =
            json!({"id":"child", "inheritsFrom":"1.20.1", "customPackField":{"hello":"world"}});
        storage::write_json(&path, &source).unwrap();
        let bytes = fs::read(&path).unwrap();
        let parent = version::metadata_path(&game, "1.20.1").unwrap();
        storage::write_json(&parent, &crate::test_http::version_fixture("1.20.1")).unwrap();
        fs::create_dir_all(game.join("versions/child/mods")).unwrap();
        // Inspection is also allowed while another download owns the operation lock.
        let _guard = launcher.0.operation.try_lock().unwrap();
        let metadata = launcher.version_metadata(&instance.id, "child").unwrap();
        assert_eq!(metadata.documents.len(), 2);
        assert_eq!(metadata.documents[0].path, path);
        assert_eq!(metadata.documents[1].path, parent);
        assert_eq!(
            serde_json::from_str::<Value>(metadata.documents[0].json.as_ref().unwrap()).unwrap(),
            source
        );
        assert_eq!(
            metadata.working_directory,
            Some(game.join("versions/child"))
        );
        assert!(metadata.main_class.is_some());
        assert!(metadata.issue.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(
            !metadata.documents[0]
                .fields
                .iter()
                .any(|f| f.label == "Java 版本" || f.label == "启动入口")
        );
        assert!(
            metadata.documents[1]
                .fields
                .iter()
                .any(|f| f.label == "启动入口")
        );
        assert_eq!(metadata.documents[1].kind, "继承版本配置");
    }

    #[test]
    fn discovers_pack_sources_only_for_the_selected_profile_and_working_directory() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pack", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let profile = game.join("versions/pack");
        storage::write_json(
            &profile.join("pack.json"),
            &crate::test_http::version_fixture("pack"),
        )
        .unwrap();
        fs::create_dir_all(profile.join("mods")).unwrap();
        let manifest = json!({"manifestType":"minecraftModpack", "minecraft":{"version":"1.20.1", "modLoaders":[{"id":"forge-47.4.4"}]}, "files":[{"projectID":12,"fileID":34,"fileName":"mod.jar"}]});
        storage::write_json(&profile.join("manifest.json"), &manifest).unwrap();
        storage::write_json(&profile.join("modpack.cfg"), &json!({"manifest":manifest})).unwrap();
        fs::write(profile.join("modrinth.index.json"), b"{broken").unwrap();
        storage::write_json(
            &game.join("manifest.json"),
            &json!({"name":"unrelated shared pack"}),
        )
        .unwrap();
        storage::write_json(&game.join("versions/other/manifest.json"), &manifest).unwrap();
        let result = launcher.version_metadata(&instance.id, "pack").unwrap();
        assert_eq!(result.documents.len(), 4);
        assert!(
            result
                .documents
                .iter()
                .all(|d| d.path.starts_with(&profile))
        );
        let manifest = result
            .documents
            .iter()
            .find(|d| d.name == "manifest.json")
            .unwrap();
        assert_eq!(manifest.sections[0].entries[0].name, "mod.jar");
        let wrapped = result
            .documents
            .iter()
            .find(|d| d.name == "modpack.cfg")
            .unwrap();
        assert_eq!(wrapped.kind, "HMCL · CurseForge");
        assert_eq!(
            wrapped.sections[0].entries[0].detail.as_deref(),
            Some("Project ID: 12 · File ID: 34")
        );
        assert!(
            result
                .documents
                .iter()
                .find(|d| d.name == "modrinth.index.json")
                .unwrap()
                .issue
                .is_some()
        );
        assert!(result.issue.is_none()); // One bad manifest must not break the version.
        assert!(!profile.join("manifest.json.bak").exists());
    }

    #[test]
    fn pending_corrupt_and_cyclic_profiles_still_have_inspectable_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Pack", "pack").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let pending = launcher.version_metadata(&instance.id, "pack").unwrap();
        assert!(pending.documents[0].issue.is_some());
        let path = version::metadata_path(&game, "pack").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{broken JSON").unwrap();
        let corrupt = launcher.version_metadata(&instance.id, "pack").unwrap();
        assert_eq!(corrupt.documents[0].json.as_deref(), Some("{broken JSON"));
        assert!(corrupt.documents[0].issue.is_some());
        storage::write_json(&path, &json!({"id":"pack","inheritsFrom":"pack"})).unwrap();
        assert!(
            launcher
                .version_metadata(&instance.id, "pack")
                .unwrap()
                .issue
                .is_some()
        );
        assert!(
            launcher
                .version_metadata(&instance.id, "untracked")
                .is_err()
        );
        assert!(launcher.version_metadata(&instance.id, "../pack").is_err());
    }
}
