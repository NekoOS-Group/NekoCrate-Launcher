use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub microsoft_client_id: String,
    pub download_source: String,
    pub concurrency: usize,
    pub retries: usize,
    pub auto_java: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            microsoft_client_id: String::new(),
            download_source: "official".into(),
            concurrency: 8,
            retries: 2,
            auto_java: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<u8>,
    pub versions: Vec<InstanceVersion>,
    pub selected_version: Option<String>,
    pub memory_mb: u32,
    pub java_path: Option<String>,
    pub created_at: u64,
    pub last_played: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceVersion {
    pub id: String,
    pub installed: bool,
}
impl Instance {
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
            schema: 2,
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
    pub url: String,
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
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "logging")]
    pub logging: Option<serde_json::Value>,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
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
pub struct Event {
    pub kind: String,
    pub message: String,
    pub instance_id: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub bytes: u64,
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
        }
    }
}
pub type EventSink = std::sync::Arc<dyn Fn(Event) + Send + Sync>;
