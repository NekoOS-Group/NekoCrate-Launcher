use super::*;
use crate::network;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Deserialize)]
pub struct ResourceScope {
    pub id: String,
    pub version: String,
    pub kind: ResourceKind,
    pub platform: Platform,
}
pub(super) struct Provider {
    client: reqwest::Client,
    platform: Platform,
    key: Option<reqwest::header::HeaderValue>,
    cache: PathBuf,
}
pub(super) fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        bail!("无效资源 ID");
    }
    Ok(())
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn mr_type(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Mods => "mod",
        ResourceKind::Resourcepacks => "resourcepack",
        ResourceKind::Datapacks => "datapack",
        ResourceKind::Shaderpacks => "shader",
        ResourceKind::Saves => "",
    }
}
fn loader_filters(target: &ResourceTarget, kind: ResourceKind) -> Vec<String> {
    match kind {
        ResourceKind::Mods => target
            .loader
            .as_ref()
            .map(|l| vec![l.kind.to_lowercase()])
            .unwrap_or_default(),
        ResourceKind::Datapacks => vec!["datapack".into()],
        _ => Vec::new(),
    }
}
fn cf_loader(target: &ResourceTarget) -> Option<u8> {
    match target.loader.as_ref()?.kind.to_ascii_lowercase().as_str() {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}
pub(super) fn compatible(
    version: &ResourceVersion,
    target: &ResourceTarget,
    kind: ResourceKind,
) -> bool {
    version.games.contains(&target.minecraft)
        && (kind != ResourceKind::Mods && kind != ResourceKind::Datapacks
            || kind == ResourceKind::Datapacks && version.loaders.is_empty()
            || loader_filters(target, kind).iter().any(|l| {
                version
                    .loaders
                    .iter()
                    .any(|other| other.eq_ignore_ascii_case(l))
            }))
}
impl Provider {
    pub(super) fn new(launcher: &Launcher, platform: Platform) -> Result<Self> {
        let key = if platform == Platform::Curseforge {
            let value = launcher
                .curseforge_key()?
                .context("需要配置 CurseForge API Key，请前往网络设置")?;
            let mut header = reqwest::header::HeaderValue::from_str(&value)?;
            header.set_sensitive(true);
            Some(header)
        } else {
            None
        };
        if platform == Platform::Local {
            bail!("本地文件不支持在线搜索");
        }
        Ok(Self {
            client: network::client_builder(launcher.root(), &launcher.settings())?
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()?,
            platform,
            key,
            cache: launcher.root().join("metadata/resources"),
        })
    }
    pub(super) async fn get(
        &self,
        path: &str,
        query: &[(&str, String)],
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let base = if self.platform == Platform::Modrinth {
            "https://api.modrinth.com/v2/"
        } else {
            "https://api.curseforge.com/v1/"
        };
        let url = reqwest::Url::parse_with_params(&format!("{base}{path}"), query)?;
        let digest = ring::digest::digest(&ring::digest::SHA256, url.as_str().as_bytes());
        let cached = self
            .cache
            .join(format!("{}.json", hex::encode(digest.as_ref())));
        if fs::metadata(&cached)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(600))
            && let Ok(value) = storage::read_json::<Value>(&cached)
        {
            return Ok(value);
        }
        let mut request = self.client.get(url);
        if let Some(key) = &self.key {
            request = request.header("x-api-key", key);
        }
        let value = tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            result = async {
                let mut response = request.send().await?;
                match response.status().as_u16() {
                    401 | 403 if self.platform == Platform::Curseforge => bail!("CurseForge API Key 无效或没有访问权限"),
                    429 => bail!("资源平台请求过于频繁，请稍后重试"),
                    _ => {},
                }
                response = response.error_for_status()?;
                if response.status().is_redirection() { bail!("资源接口返回了非预期跳转"); }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    if bytes.len() + chunk.len() > 8 * 1024 * 1024 { bail!("资源元数据过大"); }
                    bytes.extend(chunk);
                }
                Ok::<Value, anyhow::Error>(serde_json::from_slice(&bytes)?)
            } => result.map_err(network::explain_error)?,
        };
        // A failed cache write must not prevent discovery.
        let _ = fs::create_dir_all(&self.cache);
        let _ = storage::write_json(&cached, &value);
        Ok(value)
    }
    async fn cf_class(&self, kind: ResourceKind, cancel: &CancellationToken) -> Result<u64> {
        match kind {
            ResourceKind::Mods => Ok(6),
            ResourceKind::Resourcepacks => Ok(12),
            ResourceKind::Saves => Ok(17),
            _ => {
                let value = self
                    .get(
                        "categories",
                        &[("gameId", "432".into()), ("classesOnly", "true".into())],
                        cancel,
                    )
                    .await?;
                let slug = if kind == ResourceKind::Shaderpacks {
                    "shaders"
                } else {
                    "data-packs"
                };
                value["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|v| {
                        v["slug"].as_str().is_some_and(|s| {
                            s == slug || (kind == ResourceKind::Datapacks && s == "datapacks")
                        })
                    })
                    .and_then(|v| v["id"].as_u64())
                    .context("该平台暂无此资源分类")
            }
        }
    }
    async fn project(&self, id: &str, cancel: &CancellationToken) -> Result<Project> {
        identifier(id)?;
        let path = if self.platform == Platform::Modrinth {
            format!("project/{id}")
        } else {
            format!("mods/{id}")
        };
        let value = self.get(&path, &[], cancel).await?;
        let v = if self.platform == Platform::Modrinth {
            &value
        } else {
            &value["data"]
        };
        if self.platform == Platform::Curseforge && v["allowModDistribution"] == false {
            bail!("作者未允许通过第三方启动器下载：{}", text(v, "name"));
        }
        Ok(project(v, self.platform))
    }
    async fn versions(
        &self,
        target: &ResourceTarget,
        kind: ResourceKind,
        project: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResourceVersion>> {
        identifier(project)?;
        let mut query = Vec::new();
        let path = if self.platform == Platform::Modrinth {
            query.push(("game_versions", json!([target.minecraft]).to_string()));
            let loaders = loader_filters(target, kind);
            if !loaders.is_empty() {
                query.push(("loaders", json!(loaders).to_string()));
            }
            format!("project/{project}/version")
        } else {
            query.push(("gameVersion", target.minecraft.clone()));
            query.push(("pageSize", "50".into()));
            if kind == ResourceKind::Mods
                && let Some(loader) = cf_loader(target)
            {
                query.push(("modLoaderType", loader.to_string()));
            }
            format!("mods/{project}/files")
        };
        let value = self.get(&path, &query, cancel).await?;
        let array = if self.platform == Platform::Modrinth {
            &value
        } else {
            &value["data"]
        };
        let mut versions = Vec::new();
        for v in array.as_array().context("资源版本格式无效")? {
            let version = parse_version(v, self.platform)?;
            if compatible(&version, target, kind) {
                versions.push(version);
            }
        }
        Ok(versions)
    }
    pub(super) async fn version(
        &self,
        project: Option<&str>,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<ResourceVersion> {
        identifier(id)?;
        let path = if self.platform == Platform::Modrinth {
            format!("version/{id}")
        } else {
            let project = project.context("缺少 CurseForge 项目 ID")?;
            identifier(project)?;
            format!("mods/{project}/files/{id}")
        };
        let v = self.get(&path, &[], cancel).await?;
        parse_version(
            if self.platform == Platform::Modrinth {
                &v
            } else {
                &v["data"]
            },
            self.platform,
        )
    }
}
pub(super) fn project(v: &Value, platform: Platform) -> Project {
    if platform == Platform::Modrinth {
        let id = v["project_id"]
            .as_str()
            .or_else(|| v["id"].as_str())
            .unwrap_or_default();
        Project {
            id: id.into(),
            name: text(v, "title"),
            description: text(v, "description"),
            downloads: v["downloads"].as_u64().unwrap_or(0),
            url: format!("https://modrinth.com/project/{id}"),
            kind: match v["project_type"].as_str() {
                Some("mod") => Some(ResourceKind::Mods),
                Some("resourcepack") => Some(ResourceKind::Resourcepacks),
                Some("datapack") => Some(ResourceKind::Datapacks),
                Some("shader") => Some(ResourceKind::Shaderpacks),
                _ => None,
            },
        }
    } else {
        Project {
            id: v["id"].to_string(),
            name: text(v, "name"),
            description: text(v, "summary"),
            downloads: v["downloadCount"].as_f64().unwrap_or(0.0) as u64,
            url: text(&v["links"], "websiteUrl"),
            kind: match v["classId"].as_u64() {
                Some(6) => Some(ResourceKind::Mods),
                Some(12) => Some(ResourceKind::Resourcepacks),
                Some(17) => Some(ResourceKind::Saves),
                _ => None,
            },
        }
    }
}
pub(super) fn parse_version(v: &Value, platform: Platform) -> Result<ResourceVersion> {
    let mut result = if platform == Platform::Modrinth {
        ResourceVersion {
            id: text(v, "id"),
            project: text(v, "project_id"),
            name: text(v, "name"),
            number: text(v, "version_number"),
            release_type: text(v, "version_type"),
            games: strings(&v["game_versions"]),
            loaders: strings(&v["loaders"]),
            files: v["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| ResourceFile {
                    name: text(f, "filename"),
                    url: text(f, "url"),
                    size: f["size"].as_u64().unwrap_or(0),
                    sha1: text(&f["hashes"], "sha1"),
                    primary: f["primary"].as_bool().unwrap_or(false),
                })
                .collect(),
            dependencies: v["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| ResourceDependency {
                    project: d["project_id"].as_str().map(str::to_owned),
                    version: d["version_id"].as_str().map(str::to_owned),
                    relation: text(d, "dependency_type"),
                })
                .collect(),
        }
    } else {
        let games = strings(&v["gameVersions"]);
        ResourceVersion {
            id: v["id"].to_string(),
            project: v["modId"].to_string(),
            name: text(v, "displayName"),
            number: text(v, "displayName"),
            release_type: match v["releaseType"].as_u64() {
                Some(1) => "release",
                Some(2) => "beta",
                _ => "alpha",
            }
            .into(),
            loaders: games
                .iter()
                .filter(|g| {
                    ["forge", "neoforge", "fabric", "quilt"]
                        .contains(&g.to_ascii_lowercase().as_str())
                })
                .map(|g| g.to_ascii_lowercase())
                .collect(),
            games,
            files: vec![ResourceFile {
                name: text(v, "fileName"),
                url: text(v, "downloadUrl"),
                size: v["fileLength"].as_u64().unwrap_or(0),
                primary: true,
                sha1: v["hashes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|h| h["algo"] == 1)
                    .map(|h| text(h, "value"))
                    .unwrap_or_default(),
            }],
            dependencies: v["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| ResourceDependency {
                    project: d["modId"].as_u64().map(|i| i.to_string()),
                    version: None,
                    relation: match d["relationType"].as_u64() {
                        Some(3) => "required",
                        Some(5) => "incompatible",
                        Some(1 | 6) => "embedded",
                        _ => "optional",
                    }
                    .into(),
                })
                .collect(),
        }
    };
    identifier(&result.id)?;
    identifier(&result.project)?;
    result.files.retain(|f| !f.name.ends_with(".sig"));
    Ok(result)
}

impl Launcher {
    pub async fn search_resources(
        &self,
        scope: &ResourceScope,
        query: &str,
        offset: usize,
        cancel: &CancellationToken,
    ) -> Result<SearchResult> {
        if query.len() > 256 || offset > 10000 {
            bail!("搜索范围无效");
        }
        let target = self.resource_target(&scope.id, &scope.version)?;
        if scope.kind == ResourceKind::Mods && target.loader.is_none() {
            bail!("此版本尚未安装 Mod 加载器");
        }
        let api = Provider::new(self, scope.platform)?;
        let (path, args) = if scope.platform == Platform::Modrinth {
            if scope.kind == ResourceKind::Saves {
                bail!("地图请通过本地导入添加");
            }
            let mut facets = vec![
                vec![format!("project_type:{}", mr_type(scope.kind))],
                vec![format!("versions:{}", target.minecraft)],
            ];
            if scope.kind == ResourceKind::Mods {
                facets.push(
                    loader_filters(&target, scope.kind)
                        .iter()
                        .map(|l| format!("categories:{l}"))
                        .collect(),
                );
            }
            (
                "search",
                vec![
                    ("query", query.into()),
                    ("offset", offset.to_string()),
                    ("limit", "20".into()),
                    ("facets", json!(facets).to_string()),
                ],
            )
        } else {
            let mut args = vec![
                ("gameId", "432".into()),
                (
                    "classId",
                    api.cf_class(scope.kind, cancel).await?.to_string(),
                ),
                ("searchFilter", query.into()),
                ("gameVersion", target.minecraft.clone()),
                ("index", offset.to_string()),
                ("pageSize", "20".into()),
                ("sortField", "2".into()),
                ("sortOrder", "desc".into()),
            ];
            if scope.kind == ResourceKind::Mods
                && let Some(loader) = cf_loader(&target)
            {
                args.push(("modLoaderType", loader.to_string()));
            }
            ("mods/search", args)
        };
        let value = api.get(path, &args, cancel).await?;
        let (hits, total) = if scope.platform == Platform::Modrinth {
            (&value["hits"], value["total_hits"].as_u64())
        } else {
            (&value["data"], value["pagination"]["totalCount"].as_u64())
        };
        Ok(SearchResult {
            hits: hits
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| project(v, scope.platform))
                .collect(),
            total: total.unwrap_or(0) as usize,
        })
    }
    pub async fn resource_versions(
        &self,
        scope: &ResourceScope,
        project: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResourceVersion>> {
        let target = self.resource_target(&scope.id, &scope.version)?;
        Provider::new(self, scope.platform)?
            .versions(&target, scope.kind, project, cancel)
            .await
    }
    pub async fn prepare_resource(
        &self,
        scope: &ResourceScope,
        project: &str,
        version: &str,
        world: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ResourcePlan> {
        self.idle(&scope.id)?;
        let target = self.resource_target(&scope.id, &scope.version)?;
        let api = Provider::new(self, scope.platform)?;
        let root = api.version(Some(project), version, cancel).await?;
        if root.project != project {
            bail!("项目与资源版本不一致");
        }
        let mut plan = new_plan(target);
        let installed = records(&plan.target.directory)?;
        let mut queue = VecDeque::from([root]);
        let mut selected = HashMap::<String, ResourceVersion>::new();
        while let Some(version) = queue.pop_front() {
            if cancel.is_cancelled() {
                bail!("操作已取消");
            }
            if let Some(previous) = selected.get(&version.project) {
                if previous.id != version.id {
                    plan.conflicts.push(format!(
                        "依赖要求不同版本：{} / {}",
                        previous.name, version.name
                    ));
                }
                continue;
            }
            if selected.len() >= 256 {
                bail!("资源依赖数量过多");
            }
            let detail = api.project(&version.project, cancel).await?;
            // v2 historically exposes datapacks as mods; the selected artifact's loader
            // identifies its real destination. Never put a required JAR in shaderpacks.
            let entry_kind = if version.loaders.iter().any(|l| l == "datapack")
                && !version.files.iter().any(|f| f.name.ends_with(".jar"))
            {
                ResourceKind::Datapacks
            } else if version
                .files
                .iter()
                .any(|f| f.primary && f.name.ends_with(".jar"))
            {
                ResourceKind::Mods
            } else {
                detail.kind.unwrap_or(scope.kind)
            };
            if version.project == project && selected.is_empty() && entry_kind != scope.kind {
                bail!("所选文件的类型与资源分类不符");
            }
            if !compatible(&version, &plan.target, entry_kind) {
                plan.conflicts
                    .push(format!("资源与游戏或加载器版本不兼容：{}", version.name));
                continue;
            }
            let primary: Vec<_> = version.files.iter().filter(|f| f.primary).collect();
            let file = if primary.len() == 1 {
                primary[0]
            } else if version.files.len() == 1 {
                &version.files[0]
            } else {
                bail!("资源包含多个主文件，无法安全选择：{}", detail.name);
            };
            component(&file.name)?;
            let url = reqwest::Url::parse(&file.url)
                .context("作者未提供可用下载地址，请从项目页面下载后拖入")?;
            if url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
                || file.sha1.len() != 40
                || !file.sha1.bytes().all(|c| c.is_ascii_hexdigit())
            {
                bail!("资源缺少安全下载地址或校验信息");
            }
            if entry_kind == ResourceKind::Saves {
                bail!("地图请从项目页面下载后拖入，确认压缩包目录后安装");
            }
            plan.entries.push(PlanEntry {
                kind: entry_kind,
                destination: destination(&plan.target, entry_kind, &file.name, world)?,
                size: file.size,
                origin: ResourceOrigin {
                    platform: scope.platform,
                    project: Some(version.project.clone()),
                    version: Some(version.id.clone()),
                    name: detail.name,
                    dependencies: version.dependencies.clone(),
                },
                input: Input::Remote { file: file.clone() },
                reuse: false,
                content_sha256: None,
            });
            selected.insert(version.project.clone(), version.clone());
            if version.dependencies.len() > 1024 || queue.len() > 2048 {
                bail!("资源依赖数量过多");
            }
            for dependency in version
                .dependencies
                .iter()
                .filter(|d| d.relation == "required")
            {
                let dep = if let Some(id) = &dependency.version {
                    api.version(dependency.project.as_deref(), id, cancel)
                        .await?
                } else if let Some(project) = &dependency.project {
                    if selected.contains_key(project) {
                        continue;
                    }
                    let dependency_kind = api
                        .project(project, cancel)
                        .await?
                        .kind
                        .unwrap_or(entry_kind);
                    let existing = installed.iter().find(|(path, record)| {
                        record.origin.platform == scope.platform
                            && record.origin.project.as_deref() == Some(project)
                            && record.origin.version.is_some()
                            && safe_join(&plan.target.directory, path).is_ok_and(|p| p.is_file())
                    });
                    if let Some((_, record)) = existing {
                        let existing = api
                            .version(
                                Some(project),
                                record.origin.version.as_deref().unwrap(),
                                cancel,
                            )
                            .await?;
                        if compatible(&existing, &plan.target, dependency_kind) {
                            queue.push_back(existing);
                            continue;
                        }
                    }
                    let versions = api
                        .versions(&plan.target, dependency_kind, project, cancel)
                        .await?;
                    versions
                        .iter()
                        .find(|v| v.release_type == "release")
                        .or_else(|| versions.first())
                        .cloned()
                        .with_context(|| format!("必要依赖没有兼容版本：{project}"))?
                } else {
                    bail!("必要依赖缺少可解析的项目或版本 ID");
                };
                if dependency
                    .project
                    .as_ref()
                    .is_some_and(|id| id != &dep.project)
                {
                    bail!("依赖版本与项目不一致");
                }
                queue.push_back(dep);
            }
        }
        for version in selected.values() {
            for dep in version
                .dependencies
                .iter()
                .filter(|d| d.relation == "incompatible")
            {
                let conflicts = |p: &str, v: &str| {
                    dep.project.as_deref().is_none_or(|id| id == p)
                        && dep.version.as_deref().is_none_or(|id| id == v)
                };
                if selected.values().any(|v| conflicts(&v.project, &v.id))
                    || installed.iter().any(|(path, r)| {
                        r.origin.platform == scope.platform
                            && safe_join(&plan.target.directory, path).is_ok_and(|p| p.exists())
                            && conflicts(
                                r.origin.project.as_deref().unwrap_or(""),
                                r.origin.version.as_deref().unwrap_or(""),
                            )
                    })
                {
                    plan.conflicts
                        .push(format!("存在声明不兼容的资源：{}", version.name));
                }
            }
        }
        let token = cancel.clone();
        tokio::task::spawn_blocking(move || {
            install::preflight(&mut plan, &token)?;
            save_plan(&plan)?;
            Ok(plan)
        })
        .await?
    }
}
