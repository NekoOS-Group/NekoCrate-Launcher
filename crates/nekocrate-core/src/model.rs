use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

// Stable persisted IDs, matching app/src/instanceIcons.ts (0–3 are legacy icons).
pub const INSTANCE_ICON_COUNT: u8 = 16;

/// Write-only IPC input. Never serialize or include credentials in debug output.
#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct ProxyPassword(pub String);
impl std::fmt::Debug for ProxyPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub microsoft_client_id: String,
    pub download_source: String,
    pub mod_download_source: String,
    pub proxy_mode: String,
    pub proxy_url: String,
    pub proxy_username: String,
    pub proxy_password_key: Option<String>,
    #[serde(skip_serializing)]
    pub proxy_password: Option<ProxyPassword>,
    pub concurrency: usize,
    pub adaptive_concurrency: bool,
    pub retries: usize,
    pub auto_java: bool,
    pub language: String,
    pub apply_language_to_game: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            microsoft_client_id: String::new(),
            download_source: "auto".into(),
            mod_download_source: "auto".into(),
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            proxy_username: String::new(),
            proxy_password_key: None,
            proxy_password: None,
            concurrency: 8,
            adaptive_concurrency: false,
            retries: 2,
            auto_java: true,
            language: "system".into(),
            apply_language_to_game: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<u8>,
    /// Imported directories stay in place. None denotes a launcher-managed directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_directory: Option<PathBuf>,
    pub versions: Vec<InstanceVersion>,
    pub selected_version: Option<String>,
    pub memory_mb: u32,
    #[serde(default)]
    pub memory_mode: MemoryMode,
    pub java_path: Option<String>,
    pub created_at: u64,
    pub last_played: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    #[default]
    Auto,
    Recommended,
    Minimum,
    Custom,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstanceVersion {
    pub id: String,
    /// User-facing alias; version metadata, inheritance and directories keep their IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<ModLoader>,
    pub installed: bool,
    #[serde(default)]
    pub download_progress: f64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModLoader {
    pub kind: String,
    pub version: String,
}
impl InstanceVersion {
    pub fn pending(id: &str) -> Self {
        Self {
            id: id.into(),
            minecraft_version: Some(id.into()),
            ..Self::default()
        }
    }
}
impl Instance {
    pub fn game_dir(&self, root: &Path) -> PathBuf {
        self.game_directory
            .clone()
            .unwrap_or_else(|| root.join("instances").join(&self.id).join(".minecraft"))
    }
    pub fn selected(&self) -> Option<&InstanceVersion> {
        self.versions
            .iter()
            .find(|v| Some(&v.id) == self.selected_version.as_ref())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub expires_at: u64,
    pub client_id: String,
    pub owned: bool,
    #[serde(default)]
    pub skin: Option<AccountSkin>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountSkin {
    pub url: String,
    pub slim: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Database {
    pub schema: u32,
    pub settings: Settings,
    pub instances: Vec<Instance>,
    pub accounts: Vec<Account>,
    pub selected_account: Option<String>,
}
impl Default for Database {
    fn default() -> Self {
        Self {
            schema: 3,
            settings: Settings::default(),
            instances: vec![],
            accounts: vec![],
            selected_account: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: HashMap<String, String>,
    pub versions: Vec<VersionEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    pub path: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: HashMap<String, Artifact>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: LibraryDownloads,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    pub extract: Option<Extract>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: serde_json::Value,
    },
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    pub downloads: HashMap<String, Artifact>,
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    /// Explicit compatibility declaration used by launcher/pack metadata.
    #[serde(
        default,
        rename = "compatibleJavaMajors",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub compatible_java_majors: Vec<u32>,
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "logging")]
    pub logging: Option<serde_json::Value>,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    pub virtual_: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileProgress {
    pub id: String,
    pub name: String,
    pub downloaded: u64,
    pub total: u64,
    pub status: String,
    #[serde(skip_serializing_if = "is_false")]
    pub cached: bool,
}
fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub kind: String,
    pub message: String,
    pub instance_id: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub total_bytes: u64,
    #[serde(skip_serializing_if = "is_false")]
    pub reset_progress: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileProgress>,
}
impl Event {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            instance_id: None,
            completed: 0,
            total: 0,
            bytes: 0,
            total_bytes: 0,
            reset_progress: false,
            step: None,
            files: Vec::new(),
        }
    }
    pub fn workflow(step: &str, message: &str) -> Self {
        Self {
            step: Some(step.into()),
            ..Self::new("workflow", message)
        }
    }
    pub fn scoped(sink: &EventSink, instance_id: &str, step: Option<&str>) -> EventSink {
        let sink = sink.clone();
        let instance_id = instance_id.to_owned();
        let step = step.map(str::to_owned);
        std::sync::Arc::new(move |mut event| {
            event.instance_id = Some(instance_id.clone());
            if event.step.is_none() {
                event.step.clone_from(&step);
            }
            sink(event);
        })
    }
}
pub type EventSink = std::sync::Arc<dyn Fn(Event) + Send + Sync>;
