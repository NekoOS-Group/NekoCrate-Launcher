//! Resource discovery and transactional imports into a profile's actual game directory.
//! Plans and provenance live with that directory, never in the launcher database.
use crate::{Launcher, model::ModLoader, storage};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod imports;
pub use imports::{DetectedResource, LocalImport, ResourceCopySource};
mod install;
mod modpacks;
mod provider;
pub use modpacks::{ModpackArchive, ModpackVersions};
pub use provider::ResourceScope;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Mods,
    Resourcepacks,
    Datapacks,
    Shaderpacks,
    Saves,
}
impl ResourceKind {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::Resourcepacks => "resourcepacks",
            Self::Datapacks => "datapacks",
            Self::Shaderpacks => "shaderpacks",
            Self::Saves => "saves",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Modrinth,
    Curseforge,
    Local,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceTarget {
    pub instance: String,
    pub version: String,
    pub directory: PathBuf,
    pub minecraft: String,
    pub loader: Option<ModLoader>,
    pub worlds: Vec<String>,
    pub shared: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub downloads: u64,
    pub url: String,
    pub kind: Option<ResourceKind>,
}
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub hits: Vec<Project>,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceDependency {
    pub project: Option<String>,
    pub version: Option<String>,
    pub relation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceVersion {
    pub id: String,
    pub project: String,
    pub name: String,
    pub number: String,
    pub release_type: String,
    pub files: Vec<ResourceFile>,
    pub dependencies: Vec<ResourceDependency>,
    pub games: Vec<String>,
    pub loaders: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceFile {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha1: String,
    pub primary: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceOrigin {
    pub platform: Platform,
    pub project: Option<String>,
    pub version: Option<String>,
    pub name: String,
    pub dependencies: Vec<ResourceDependency>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    Remote {
        file: ResourceFile,
    },
    Local {
        path: PathBuf,
        digest: String,
        archive_prefix: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanEntry {
    pub kind: ResourceKind,
    pub destination: String,
    pub size: u64,
    pub origin: ResourceOrigin,
    pub input: Input,
    pub reuse: bool,
    #[serde(default)]
    pub content_sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourcePlan {
    pub id: String,
    pub target: ResourceTarget,
    pub entries: Vec<PlanEntry>,
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
    pub created_at: u64,
    pub complete: bool,
    #[serde(default)]
    pub started: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceRecord {
    pub kind: ResourceKind,
    pub origin: ResourceOrigin,
    pub sha256: String,
    pub sha1: Option<String>,
    pub installed_at: u64,
}
pub type Records = BTreeMap<String, ResourceRecord>;

impl Launcher {
    pub fn resource_target(&self, id: &str, version: &str) -> Result<ResourceTarget> {
        let instance = self.instance(id)?;
        let tracked = instance
            .versions
            .iter()
            .find(|v| v.id == version)
            .context("版本不存在")?;
        let (directory, resolved) = self.content_location(id, version)?;
        let game = self.available_game_dir(id)?;
        let minecraft = resolved
            .as_ref()
            .map(|v| v.minecraft_version.clone())
            .or_else(|| tracked.minecraft_version.clone())
            .context("无法确定游戏版本")?;
        let loader = resolved
            .as_ref()
            .and_then(|v| v.loader.clone())
            .or_else(|| tracked.loader.clone());
        let mut worlds = Vec::new();
        let saves = directory.join("saves");
        if saves.is_dir() {
            for entry in fs::read_dir(&saves)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() && entry.path().join("level.dat").is_file() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if component(&name).is_ok() {
                        worlds.push(name);
                    }
                }
            }
        }
        worlds.sort();
        Ok(ResourceTarget {
            instance: id.into(),
            version: version.into(),
            shared: directory == game,
            directory,
            minecraft,
            loader,
            worlds,
        })
    }

    pub fn resource_plans(&self, id: &str, version: &str) -> Result<Vec<ResourcePlan>> {
        let target = self.resource_target(id, version)?;
        pending_plans(&target.directory, id, version)
    }
}

pub(crate) fn pending_plans(working: &Path, id: &str, version: &str) -> Result<Vec<ResourcePlan>> {
    let directory = safe_join(working, ".nekocrate/resource-tasks")?;
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut plans = Vec::new();
    for entry in fs::read_dir(directory)?.take(512) {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = safe_join(&entry.path(), "plan.json")?;
            if let Ok(plan) = storage::read_json::<ResourcePlan>(&path)
                && plan.started
                && !plan.complete
                && plan.target.version == version
                && plan.target.instance == id
            {
                plans.push(plan);
            }
        }
    }
    plans.sort_by_key(|p| std::cmp::Reverse(p.created_at));
    Ok(plans)
}

fn component(name: &str) -> Result<()> {
    let relative = crate::archive_support::relative(name)?;
    if relative.components().count() != 1 {
        bail!("无效资源文件名：{name}");
    }
    Ok(())
}
pub(crate) fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = crate::archive_support::relative(relative)?;
    // The user may deliberately associate a symlinked game root. Child paths may not redirect writes.
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("资源路径包含符号链接：{}", path.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}
fn destination(
    target: &ResourceTarget,
    kind: ResourceKind,
    file: &str,
    world: Option<&str>,
) -> Result<String> {
    component(file)?;
    let path = if kind == ResourceKind::Datapacks {
        let world = world.context("请先选择要安装数据包的存档")?;
        component(world)?;
        if !target.worlds.iter().any(|w| w == world) {
            bail!("目标存档不存在");
        }
        format!("saves/{world}/datapacks/{file}")
    } else {
        format!("{}/{file}", kind.directory())
    };
    safe_join(&target.directory, &path)?;
    Ok(path)
}
fn task_dir(target: &ResourceTarget, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id).context("无效资源任务")?;
    safe_join(
        &target.directory,
        &format!(".nekocrate/resource-tasks/{id}"),
    )
}

/// Shared game directories can contain tasks belonging to different profiles.
/// Remove only the deleted profile's plans/staging, never published resources.
pub(crate) fn remove_version_tasks(
    game: &Path,
    instance: &str,
    version: Option<&str>,
) -> Result<()> {
    let directory = safe_join(game, ".nekocrate/resource-tasks")?;
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if uuid::Uuid::parse_str(&id).is_err() {
            continue;
        }
        let plan: ResourcePlan = storage::read_json(&safe_join(&entry.path(), "plan.json")?)?;
        if plan.id == id
            && plan.target.instance == instance
            && version.is_none_or(|v| plan.target.version == v)
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
pub(crate) fn records(root: &Path) -> Result<Records> {
    let path = safe_join(root, ".nekocrate/resources.json")?;
    if path.exists() {
        storage::read_json(&path)
    } else {
        Ok(Records::new())
    }
}
fn save_plan(plan: &ResourcePlan) -> Result<()> {
    let directory = task_dir(&plan.target, &plan.id)?;
    fs::create_dir_all(&directory)?;
    storage::write_json(&safe_join(&directory, "plan.json")?, plan)
}
fn new_plan(target: ResourceTarget) -> ResourcePlan {
    ResourcePlan {
        id: uuid::Uuid::new_v4().to_string(),
        target,
        entries: Vec::new(),
        conflicts: Vec::new(),
        warnings: Vec::new(),
        created_at: storage::now(),
        complete: false,
        started: false,
    }
}

pub(crate) fn provenance_source(
    root: &Path,
    path: &Path,
) -> Result<Option<crate::version_management::details::ContentSource>> {
    use crate::version_management::details::{ContentField, ContentSource};
    let Some(relative) = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
    else {
        return Ok(None);
    };
    let Some(record) = records(root)?.remove(&relative) else {
        return Ok(None);
    };
    let mut fields = Vec::new();
    for (label, value) in [
        (
            "来源",
            Some(
                match record.origin.platform {
                    Platform::Modrinth => "Modrinth",
                    Platform::Curseforge => "CurseForge",
                    Platform::Local => "Local",
                }
                .to_owned(),
            ),
        ),
        ("项目 ID", record.origin.project),
        ("版本 ID", record.origin.version),
        ("SHA-256", Some(record.sha256)),
        ("SHA-1", record.sha1),
    ] {
        if let Some(value) = value {
            fields.push(ContentField {
                label: label.into(),
                value,
                format: None,
            });
        }
    }
    Ok(Some(ContentSource {
        source: ".nekocrate/resources.json".into(),
        revealable: false,
        title: Some("安装来源".into()),
        description: None,
        fields,
        dependencies: record
            .origin
            .dependencies
            .into_iter()
            .map(|d| crate::version_management::details::ContentDependency {
                id: d.project.unwrap_or_default(),
                requirement: d.version.unwrap_or_default(),
                relation: if d.relation == "required" {
                    "required"
                } else {
                    "optional"
                },
            })
            .collect(),
        issue: None,
    }))
}
