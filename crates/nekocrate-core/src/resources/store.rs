//! Download-only catalogs stay separate from resources installed into a client.
use super::{
    provider::{self, Provider},
    *,
};
use anyhow::ensure;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
pub mod skins;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Plugins,
    Maps,
    Servers,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Scope {
    pub kind: Kind,
    pub platform: Platform,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    pub category: String,
}
impl Scope {
    fn validate(&self) -> Result<()> {
        ensure!(self.platform != Platform::Local, "无效资源平台");
        ensure!(
            self.kind != Kind::Maps || self.platform == Platform::Curseforge,
            "地图下载源为 CurseForge"
        );
        ensure!(
            self.kind != Kind::Servers || self.platform == Platform::Modrinth,
            "服务器列表来源为 Modrinth"
        );
        if !self.minecraft.is_empty() {
            storage::safe_version(&self.minecraft)?;
        }
        if !self.category.is_empty() {
            provider::identifier(&self.category)?;
            if self.platform == Platform::Curseforge {
                ensure!(
                    self.category.parse::<u32>().is_ok_and(|id| id > 0),
                    "无效资源类型"
                );
            }
        }
        ensure!(
            matches!(
                self.loader.as_str(),
                "" | "bukkit"
                    | "spigot"
                    | "paper"
                    | "purpur"
                    | "folia"
                    | "sponge"
                    | "velocity"
                    | "waterfall"
                    | "bungeecord"
            ),
            "无效插件加载器"
        );
        Ok(())
    }
    fn compatible(&self, version: &ResourceVersion) -> bool {
        (self.minecraft.is_empty() || version.games.contains(&self.minecraft))
            && (self.kind != Kind::Plugins
                || self.platform != Platform::Modrinth
                || version.loaders.iter().any(|loader| {
                    if self.loader.is_empty() {
                        PLUGIN_LOADERS.contains(&loader.as_str())
                    } else {
                        loader.eq_ignore_ascii_case(&self.loader)
                    }
                }))
    }
}
const PLUGIN_LOADERS: &[&str] = &[
    "bukkit",
    "spigot",
    "paper",
    "purpur",
    "folia",
    "sponge",
    "velocity",
    "waterfall",
    "bungeecord",
];

#[derive(Serialize)]
pub struct Server {
    pub project: Project,
    pub address: Option<String>,
    pub online: Option<u64>,
    pub details: super::presentation::ProjectDetails,
}

fn server_project(value: &Value) -> Value {
    let mut normalized = Value::Object(value.as_object().cloned().unwrap_or_default());
    normalized["body"] = value["description"].clone();
    normalized["title"] = value["name"].clone();
    normalized["description"] = value["summary"].clone();
    for (field, link) in [
        ("source_url", "source"),
        ("wiki_url", "wiki"),
        ("issues_url", "issues"),
    ] {
        normalized[field] = value["link_urls"][link]["url"].clone();
    }
    normalized
}

fn search_args(scope: &Scope, query: &str, offset: usize) -> Vec<(&'static str, String)> {
    if scope.platform == Platform::Curseforge {
        let mut args = vec![
            ("gameId", "432".into()),
            (
                "classId",
                if scope.kind == Kind::Maps { "17" } else { "5" }.into(),
            ),
            ("searchFilter", query.into()),
            ("index", offset.to_string()),
            ("pageSize", "20".into()),
            ("sortField", "2".into()),
            ("sortOrder", "desc".into()),
        ];
        if !scope.minecraft.is_empty() {
            args.push(("gameVersion", scope.minecraft.clone()));
        }
        if !scope.category.is_empty() {
            args.push(("categoryId", scope.category.clone()));
        }
        return args;
    }
    let mut args = vec![
        ("query", query.into()),
        ("offset", offset.to_string()),
        ("limit", "20".into()),
    ];
    if scope.kind == Kind::Servers {
        let mut filter = "project_types = minecraft_java_server".to_owned();
        if !scope.minecraft.is_empty() {
            filter.push_str(&format!(" AND game_versions = {}", json!(scope.minecraft)));
        }
        if !scope.category.is_empty() {
            filter.push_str(&format!(" AND categories = {}", json!(scope.category)));
        }
        args.push(("new_filters", filter));
    } else {
        let mut facets = vec![vec!["project_type:plugin".to_owned()]];
        if !scope.minecraft.is_empty() {
            facets.push(vec![format!("versions:{}", scope.minecraft)]);
        }
        if !scope.loader.is_empty() {
            facets.push(vec![format!("categories:{}", scope.loader)]);
        }
        if !scope.category.is_empty() {
            facets.push(vec![format!("categories:{}", scope.category)]);
        }
        args.push(("facets", json!(facets).to_string()));
    }
    args
}

impl Launcher {
    pub async fn store_categories(
        &self,
        scope: &Scope,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModpackCategory>> {
        scope.validate()?;
        let api = Provider::new(self, scope.platform)?;
        let (project_type, class_id) = match scope.kind {
            Kind::Servers => ("minecraft_java_server", 0),
            Kind::Maps => ("", 17),
            Kind::Plugins => ("plugin", 5),
        };
        let value = if scope.kind == Kind::Servers {
            api.modrinth_v3("tag/category", &[], cancel).await?
        } else if scope.platform == Platform::Curseforge {
            api.get_display(&format!("categories?gameId=432&classId={class_id}"), cancel)
                .await?
        } else {
            api.get_display("tag/category", cancel).await?
        };
        super::modpacks::parse_project_categories(&value, scope.platform, project_type, class_id)
    }

    pub async fn store_search(
        &self,
        scope: &Scope,
        query: &str,
        offset: usize,
        cancel: &CancellationToken,
    ) -> Result<SearchResult> {
        scope.validate()?;
        ensure!(query.len() <= 256 && offset <= 9980, "搜索范围无效");
        let api = Provider::new(self, scope.platform)?;
        let args = search_args(scope, query, offset);
        let value = if scope.kind == Kind::Servers {
            api.modrinth_v3("search", &args, cancel).await?
        } else {
            api.get(
                if scope.platform == Platform::Modrinth {
                    "search"
                } else {
                    "mods/search"
                },
                &args,
                cancel,
            )
            .await?
        };
        let mr = scope.platform == Platform::Modrinth;
        Ok(SearchResult {
            hits: value[if mr { "hits" } else { "data" }]
                .as_array()
                .context("资源列表格式无效")?
                .iter()
                .map(|v| {
                    provider::project(
                        &if scope.kind == Kind::Servers {
                            server_project(v)
                        } else {
                            v.clone()
                        },
                        scope.platform,
                    )
                })
                .collect(),
            total: if mr {
                value["total_hits"].as_u64()
            } else {
                value["pagination"]["totalCount"].as_u64()
            }
            .unwrap_or(0) as usize,
        })
    }

    pub async fn store_versions(
        &self,
        scope: &Scope,
        project: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResourceVersion>> {
        scope.validate()?;
        ensure!(scope.kind != Kind::Servers, "服务器无需下载资源文件");
        provider::identifier(project)?;
        let api = Provider::new(self, scope.platform)?;
        let mut args = Vec::new();
        let path = if scope.platform == Platform::Modrinth {
            if !scope.minecraft.is_empty() {
                args.push(("game_versions", json!([scope.minecraft]).to_string()));
            }
            let loaders = if scope.loader.is_empty() {
                PLUGIN_LOADERS.to_vec()
            } else {
                vec![scope.loader.as_str()]
            };
            args.push(("loaders", json!(loaders).to_string()));
            format!("project/{project}/version")
        } else {
            if !scope.minecraft.is_empty() {
                args.push(("gameVersion", scope.minecraft.clone()));
            }
            args.push(("pageSize", "50".into()));
            format!("mods/{project}/files")
        };
        let value = api.get(&path, &args, cancel).await?;
        let array = if scope.platform == Platform::Modrinth {
            &value
        } else {
            &value["data"]
        };
        array
            .as_array()
            .context("资源版本格式无效")?
            .iter()
            .map(|v| provider::parse_version(v, scope.platform))
            .filter(|v| match v {
                Ok(v) => scope.compatible(v),
                Err(_) => true,
            })
            .collect()
    }

    pub async fn store_server(&self, project: &str, cancel: &CancellationToken) -> Result<Server> {
        provider::identifier(project)?;
        let api = Provider::new(self, Platform::Modrinth)?;
        let value = server_project(
            &api.modrinth_v3(&format!("project/{project}"), &[], cancel)
                .await?,
        );
        ensure!(
            value["project_types"]
                .as_array()
                .is_some_and(|v| v.iter().any(|t| t == "minecraft_java_server")),
            "此项目不是游戏服务器"
        );
        let address = value["minecraft_java_server"]["address"]
            .as_str()
            .map(str::to_owned);
        let online = value["minecraft_java_server"]["ping"]["data"]["players_online"].as_u64();
        let body = value["body"].as_str().unwrap_or_default().to_owned();
        let mut details = super::presentation::details(&value, Platform::Modrinth, body, vec![]);
        for (key, label) in [("site", "主页"), ("discord", "Discord")] {
            if let Some(url) = value["link_urls"][key]["url"]
                .as_str()
                .and_then(super::presentation::web_url)
            {
                details
                    .links
                    .push(super::presentation::ProjectLink { label, url });
            }
        }
        Ok(Server {
            project: provider::project(&value, Platform::Modrinth),
            address,
            online,
            details,
        })
    }

    pub async fn store_download(
        &self,
        scope: &Scope,
        project: &str,
        version: &str,
        output: &Path,
        cancel: &CancellationToken,
        sink: &crate::model::EventSink,
    ) -> Result<()> {
        scope.validate()?;
        ensure!(scope.kind != Kind::Servers, "服务器无需下载资源文件");
        provider::identifier(project)?;
        provider::identifier(version)?;
        ensure!(
            output.is_absolute() && !output.exists(),
            "目标文件已存在，请选择其他文件名"
        );
        let _guard = self
            .0
            .operation
            .try_lock()
            .context("请先结束当前下载或安装任务")?;
        let api = Provider::new(self, scope.platform)?;
        let selected = api.version(Some(project), version, cancel).await?;
        ensure!(
            selected.project == project && scope.compatible(&selected),
            "此资源没有兼容版本"
        );
        // Also enforces the author's third-party distribution setting.
        let detail = api.project(project, cancel).await?;
        if scope.kind == Kind::Plugins && scope.platform == Platform::Curseforge {
            let raw = api.get(&format!("mods/{project}"), &[], cancel).await?;
            ensure!(raw["data"]["classId"] == 5, "此项目不是服务器插件");
        }
        ensure!(
            scope.kind != Kind::Maps || detail.kind == Some(ResourceKind::Saves),
            "此项目不是地图"
        );
        let file = provider::primary_file(&selected)?;
        let extension = if scope.kind == Kind::Maps {
            "zip"
        } else {
            "jar"
        };
        ensure!(
            Path::new(&file.name)
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case(extension))
                && output
                    .extension()
                    .is_some_and(|v| v.eq_ignore_ascii_case(extension)),
            "下载文件类型不匹配"
        );
        let root = self.root().join("cache/store-downloads").join(&file.sha1);
        fs::create_dir_all(&root)?;
        let mut downloader = crate::download::Downloader::new(self.root(), self.settings())?
            .with_cache(root.join("objects"));
        if scope.platform == Platform::Curseforge {
            downloader = downloader.with_curseforge_key(
                self.root(),
                &self.settings(),
                crate::curseforge::key()?,
            )?;
        }
        let spec = crate::download::FileSpec {
            url: file.url.clone(),
            path: root.join(&file.name),
            hash: file.sha1.clone(),
            sha256: false,
            size: file.size,
        };
        downloader.batch(vec![spec.clone()], cancel, sink).await?;
        let src = spec.path.clone();
        let dst = output.to_owned();
        let token = cancel.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            crate::download::check_cancel(&token)?;
            let mut source = fs::File::open(src)?;
            let mut dest = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst)?;
            let result = std::io::copy(&mut source, &mut dest).and_then(|_| dest.sync_all());
            if result.is_err() {
                drop(dest);
                let _ = fs::remove_file(&dst);
            }
            result?;
            Ok(())
        })
        .await??;
        // The exported file becomes a reusable local source, without retaining
        // an extra completed copy under the launcher's cache.
        let _ = downloader.index_local(output).await;
        let _ = downloader.discard_installer(&spec).await;
        let _ = fs::remove_dir_all(root);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sink() -> crate::model::EventSink {
        std::sync::Arc::new(|_| {})
    }
    fn cache(launcher: &Launcher, url: reqwest::Url, value: Value) {
        let hash = hex::encode(ring::digest::digest(
            &ring::digest::SHA256,
            url.as_str().as_bytes(),
        ));
        storage::write_json(
            &launcher
                .root()
                .join(format!("metadata/resources/{hash}.json")),
            &value,
        )
        .unwrap();
    }
    #[tokio::test]
    async fn server_v3_names_descriptions_and_addresses_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let scope = Scope {
            kind: Kind::Servers,
            platform: Platform::Modrinth,
            minecraft: "".into(),
            loader: "".into(),
            category: "skyblock".into(),
        };
        let value = json!({"id":"example", "project_id":"example", "name":"Example server", "summary":"Short introduction", "description":"# Long introduction", "project_types":["minecraft_java_server"], "minecraft_java_server":{"address":"play.example.test:25565","ping":{"data":{"players_online":12}}}, "link_urls":{"site":{"url":"https://example.test"}}});
        cache(
            &launcher,
            reqwest::Url::parse_with_params(
                "https://api.modrinth.com/v3/search",
                search_args(&scope, "", 20),
            )
            .unwrap(),
            json!({"hits":[value],"total_hits":40}),
        );
        cache(
            &launcher,
            reqwest::Url::parse("https://api.modrinth.com/v3/project/example").unwrap(),
            value,
        );
        let result = launcher
            .store_search(&scope, "", 20, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result.hits[0].name, "Example server");
        assert_eq!(result.hits[0].description, "Short introduction");
        assert_eq!(result.total, 40);
        let detail = launcher
            .store_server("example", &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(detail.details.body, "# Long introduction");
        assert_eq!(detail.address.as_deref(), Some("play.example.test:25565"));
        assert_eq!(detail.online, Some(12));
        assert_eq!(detail.details.links[0].url, "https://example.test/");
        cache(
            &launcher,
            reqwest::Url::parse("https://api.modrinth.com/v3/tag/category").unwrap(),
            json!([
                {"name":"skyblock","project_type":"minecraft_java_server"},
                {"name":"optimization","project_type":"modpack"},
                {"name":"pvp","project_type":"minecraft_java_server"}
            ]),
        );
        let tags = launcher
            .store_categories(&scope, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            tags.iter().map(|tag| tag.id.as_str()).collect::<Vec<_>>(),
            ["pvp", "skyblock"]
        );
    }

    #[tokio::test]
    async fn plugin_versions_exclude_client_mod_variants_and_download_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let scope = Scope {
            kind: Kind::Plugins,
            platform: Platform::Modrinth,
            minecraft: "1.21.1".into(),
            loader: "paper".into(),
            category: "".into(),
        };
        let make = |id: &str, game: &str, loader: &str| json!({"id":id, "project_id":"example", "name":id, "version_number":id, "version_type":"release", "game_versions":[game],"loaders":[loader],"files":[],"dependencies":[]});
        let url = reqwest::Url::parse_with_params(
            "https://api.modrinth.com/v2/project/example/version",
            [
                ("game_versions", "[\"1.21.1\"]"),
                ("loaders", "[\"paper\"]"),
            ],
        )
        .unwrap();
        cache(
            &launcher,
            url,
            json!([
                make("paper", "1.21.1", "paper"),
                make("fabric", "1.21.1", "fabric"),
                make("old", "1.20.1", "paper")
            ]),
        );
        let versions = launcher
            .store_versions(&scope, "example", &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "paper");
        let output = dir.path().join("existing.jar");
        fs::write(&output, b"keep").unwrap();
        assert!(
            launcher
                .store_download(
                    &scope,
                    "example",
                    "paper",
                    &output,
                    &CancellationToken::new(),
                    &sink()
                )
                .await
                .is_err()
        );
        assert_eq!(fs::read(output).unwrap(), b"keep");
    }

    #[tokio::test]
    #[ignore = "Live public-provider integration check; writes only to a temporary directory"]
    async fn live_store_catalogs_download() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let token = CancellationToken::new();
        let mut scope = Scope {
            kind: Kind::Servers,
            platform: Platform::Modrinth,
            minecraft: "".into(),
            loader: "".into(),
            category: "".into(),
        };
        let servers = launcher.store_search(&scope, "", 0, &token).await.unwrap();
        assert!(!servers.hits.is_empty() && !servers.hits[0].name.is_empty());
        let server = launcher
            .store_server(&servers.hits[0].id, &token)
            .await
            .unwrap();
        assert!(server.address.is_some() && !server.details.body.is_empty());
        scope.kind = Kind::Plugins;
        scope.loader = "paper".into();
        let plugins = launcher
            .store_search(&scope, "luckperms", 0, &token)
            .await
            .unwrap();
        assert!(!plugins.hits.is_empty());
        let project = &plugins.hits[0].id;
        let versions = launcher
            .store_versions(&scope, project, &token)
            .await
            .unwrap();
        let version = versions.first().unwrap();
        let file = provider::primary_file(version).unwrap();
        assert!(file.size < 32 * 1024 * 1024);
        let output = dir.path().join(&file.name);
        launcher
            .store_download(&scope, project, &version.id, &output, &token, &sink())
            .await
            .unwrap();
        assert_eq!(fs::metadata(&output).unwrap().len(), file.size);
        assert!(
            !launcher
                .root()
                .join("cache/store-downloads")
                .join(&file.sha1)
                .exists()
        );
    }
    #[test]
    fn catalogs_use_real_types_and_do_not_inherit_client_loaders() {
        let mut scope = Scope {
            kind: Kind::Plugins,
            platform: Platform::Modrinth,
            minecraft: "1.21.1".into(),
            loader: "paper".into(),
            category: "".into(),
        };
        scope.validate().unwrap();
        let args = search_args(&scope, "", 20);
        let facets = args.iter().find(|(k, _)| *k == "facets").unwrap().1.clone();
        assert!(facets.contains("project_type:plugin") && facets.contains("categories:paper"));
        scope.kind = Kind::Servers;
        scope.loader.clear();
        scope.category = "skyblock".into();
        assert!(
            search_args(&scope, "", 0)
                .iter()
                .any(|(k, v)| *k == "new_filters" && v == "project_types = minecraft_java_server AND game_versions = \"1.21.1\" AND categories = \"skyblock\"")
        );
        scope.kind = Kind::Maps;
        assert!(scope.validate().is_err());
        scope.platform = Platform::Curseforge;
        // Display labels must never be sent to CurseForge instead of category IDs.
        assert!(scope.validate().is_err());
        scope.category = "250".into();
        scope.validate().unwrap();
        assert!(search_args(&scope, "", 0).contains(&("classId", "17".into())));
        assert!(search_args(&scope, "", 20).contains(&("categoryId", "250".into())));
        scope.category = "0".into();
        assert!(scope.validate().is_err());
        scope.platform = Platform::Modrinth;
        scope.kind = Kind::Servers;
        scope.category = "skyblock\" OR project_types = mod".into();
        assert!(scope.validate().is_err());
        scope.category.clear();
        scope.loader = "fabric".into();
        assert!(scope.validate().is_err());
    }

    #[test]
    fn map_categories_exclude_other_project_types_and_invalid_ids() {
        let tags = super::super::modpacks::parse_project_categories(
            &json!({"data":[
                {"id":17,"name":"Worlds","isClass":true},
                {"id":250,"name":"Adventure","classId":17},
                {"id":251,"name":"Survival","parentCategoryId":17},
                {"id":252,"name":"Mods","classId":6},
                {"id":"invalid","name":"Malformed","classId":17}
            ]}),
            Platform::Curseforge,
            "",
            17,
        )
        .unwrap();
        assert_eq!(
            tags.iter().map(|tag| tag.id.as_str()).collect::<Vec<_>>(),
            ["250", "251"]
        );
    }
}
