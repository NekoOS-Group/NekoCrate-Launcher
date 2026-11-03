//! Online archives enter the same reviewed, isolated installation flow as local packs.
use super::{
    provider::{self, Provider},
    *,
};
use crate::{
    download::{Downloader, FileSpec},
    model::{Event, EventSink},
    modpack,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize)]
pub struct ModpackVersions {
    pub versions: Vec<ResourceVersion>,
    pub more: bool,
}
#[derive(Serialize)]
pub struct ModpackArchive {
    pub path: PathBuf,
    pub preview: modpack::Preview,
}
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModpackSort {
    #[default]
    Relevance,
    Downloads,
    Updated,
    Newest,
}
#[derive(Debug, Default, Deserialize)]
pub struct ModpackFilters {
    #[serde(default)]
    pub sort: ModpackSort,
    #[serde(default)]
    pub category: String,
}
#[derive(Debug, Serialize)]
pub struct ModpackCategory {
    pub id: String,
    pub name: String,
}

fn search_parameters(
    platform: Platform,
    query: &str,
    offset: usize,
    filters: &ModpackFilters,
) -> Result<(&'static str, Vec<(&'static str, String)>)> {
    range(query, offset)?;
    if !filters.category.is_empty() {
        provider::identifier(&filters.category)?;
        if platform == Platform::Curseforge {
            anyhow::ensure!(
                filters.category.parse::<u32>().is_ok_and(|id| id > 0),
                "无效资源类型"
            );
        }
    }
    match platform {
        Platform::Modrinth => {
            let mut facets = vec![vec!["project_type:modpack".to_owned()]];
            if !filters.category.is_empty() {
                facets.push(vec![format!("categories:{}", filters.category)]);
            }
            let mut args = vec![
                ("query", query.into()),
                ("offset", offset.to_string()),
                ("limit", "20".into()),
                ("facets", json!(facets).to_string()),
            ];
            // Keep the default query/cache key compatible with existing catalogs.
            if !matches!(filters.sort, ModpackSort::Relevance) {
                args.push((
                    "index",
                    match filters.sort {
                        ModpackSort::Downloads => "downloads",
                        ModpackSort::Updated => "updated",
                        ModpackSort::Newest => "newest",
                        ModpackSort::Relevance => "relevance",
                    }
                    .into(),
                ));
            }
            Ok(("search", args))
        }
        Platform::Curseforge => {
            let mut args = vec![
                ("gameId", "432".into()),
                ("classId", "4471".into()),
                ("searchFilter", query.into()),
                ("index", offset.to_string()),
                ("pageSize", "20".into()),
                (
                    "sortField",
                    match filters.sort {
                        ModpackSort::Relevance => "2",
                        ModpackSort::Downloads => "6",
                        ModpackSort::Updated => "3",
                        ModpackSort::Newest => "11",
                    }
                    .into(),
                ),
                ("sortOrder", "desc".into()),
            ];
            if !filters.category.is_empty() {
                args.push(("categoryId", filters.category.clone()));
            }
            Ok(("mods/search", args))
        }
        Platform::Local => bail!("本地文件不支持在线搜索"),
    }
}
fn parse_categories(value: &serde_json::Value, platform: Platform) -> Result<Vec<ModpackCategory>> {
    let entries = if platform == Platform::Modrinth {
        value
    } else {
        &value["data"]
    };
    let mut categories: Vec<_> = entries
        .as_array()
        .context("资源类型列表格式无效")?
        .iter()
        .filter_map(|entry| {
            let id = if platform == Platform::Modrinth {
                if entry["project_type"] != "modpack" {
                    return None;
                }
                entry["name"].as_str()?.to_owned()
            } else {
                if entry["isClass"] == true
                    || (entry["classId"] != 4471 && entry["parentCategoryId"] != 4471)
                {
                    return None;
                }
                entry["id"].as_u64()?.to_string()
            };
            if provider::identifier(&id).is_err() {
                return None;
            }
            Some(ModpackCategory {
                id,
                name: entry["name"].as_str()?.to_owned(),
            })
        })
        .collect();
    categories.sort_by_cached_key(|c| c.name.to_lowercase());
    categories.dedup_by(|a, b| a.id == b.id);
    Ok(categories)
}

fn range(query: &str, offset: usize) -> Result<()> {
    if query.len() > 256 || offset > 9900 {
        bail!("搜索范围无效");
    }
    Ok(())
}

async fn check_project(
    api: &Provider,
    platform: Platform,
    id: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    provider::identifier(id)?;
    let path = if platform == Platform::Modrinth {
        format!("project/{id}")
    } else {
        format!("mods/{id}")
    };
    let value = api.get(&path, &[], cancel).await?;
    let pack = if platform == Platform::Modrinth {
        value["project_type"] == "modpack"
    } else {
        value["data"]["classId"] == 4471
    };
    if !pack {
        bail!("此项目不是整合包");
    }
    if platform == Platform::Curseforge && value["data"]["allowModDistribution"] == false {
        bail!("作者未允许通过第三方启动器下载，请在项目主页下载后导入");
    }
    Ok(())
}
fn archive_file(version: &ResourceVersion, platform: Platform) -> Result<&ResourceFile> {
    let extension = if platform == Platform::Modrinth {
        ".mrpack"
    } else {
        ".zip"
    };
    let candidates: Vec<_> = version
        .files
        .iter()
        .filter(|f| f.name.to_ascii_lowercase().ends_with(extension))
        .collect();
    let primary: Vec<_> = candidates.iter().copied().filter(|f| f.primary).collect();
    let file = match (primary.as_slice(), candidates.as_slice()) {
        ([file], _) | ([], [file]) => *file,
        _ => bail!("此版本没有唯一的整合包文件"),
    };
    let url =
        reqwest::Url::parse(&file.url).context("此文件未提供下载地址，请在项目主页下载后导入")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("整合包下载地址无效");
    }
    if file.size == 0
        || file.size > 1024 * 1024 * 1024
        || file.sha1.len() != 40
        || !file.sha1.bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("整合包文件大小或校验信息无效");
    }
    Ok(file)
}

impl Launcher {
    pub async fn search_modpacks(
        &self,
        platform: Platform,
        query: &str,
        offset: usize,
        cancel: &CancellationToken,
    ) -> Result<SearchResult> {
        self.search_modpacks_filtered(platform, query, offset, &ModpackFilters::default(), cancel)
            .await
    }

    pub async fn modpack_categories(
        &self,
        platform: Platform,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModpackCategory>> {
        let api = Provider::new(self, platform)?;
        let value = if platform == Platform::Modrinth {
            api.get_display("tag/category", cancel).await?
        } else {
            api.get_display("categories?gameId=432&classId=4471", cancel)
                .await?
        };
        parse_categories(&value, platform)
    }

    pub async fn search_modpacks_filtered(
        &self,
        platform: Platform,
        query: &str,
        offset: usize,
        filters: &ModpackFilters,
        cancel: &CancellationToken,
    ) -> Result<SearchResult> {
        let (path, args) = search_parameters(platform, query, offset, filters)?;
        let api = Provider::new(self, platform)?;
        let value = api.get(path, &args, cancel).await?;
        let (hits, total) = if platform == Platform::Modrinth {
            (&value["hits"], value["total_hits"].as_u64())
        } else {
            (&value["data"], value["pagination"]["totalCount"].as_u64())
        };
        Ok(SearchResult {
            hits: hits
                .as_array()
                .context("资源搜索结果格式无效")?
                .iter()
                .map(|v| provider::project(v, platform))
                .collect(),
            total: total.unwrap_or(0) as usize,
        })
    }

    /// Fetch just the latest file identified by the catalog, not the version list.
    pub async fn modpack_version(
        &self,
        platform: Platform,
        project: &str,
        version: &str,
        cancel: &CancellationToken,
    ) -> Result<ResourceVersion> {
        provider::identifier(project)?;
        let api = Provider::new(self, platform)?;
        let selected = api.version(Some(project), version, cancel).await?;
        anyhow::ensure!(selected.project == project, "整合包版本不属于所选项目");
        Ok(selected)
    }

    pub async fn modpack_versions(
        &self,
        platform: Platform,
        project: &str,
        offset: usize,
        cancel: &CancellationToken,
    ) -> Result<ModpackVersions> {
        range("", offset)?;
        let api = Provider::new(self, platform)?;
        check_project(&api, platform, project, cancel).await?;
        // Packs define their own Minecraft and loader versions. Never constrain
        // them by the currently selected profile in the destination directory.
        let (path, query) = if platform == Platform::Modrinth {
            (format!("project/{project}/version"), Vec::new())
        } else {
            (
                format!("mods/{project}/files"),
                vec![("index", offset.to_string()), ("pageSize", "50".into())],
            )
        };
        let value = api.get(&path, &query, cancel).await?;
        let array = if platform == Platform::Modrinth {
            &value
        } else {
            &value["data"]
        }
        .as_array()
        .context("资源版本格式无效")?;
        let skip = if platform == Platform::Modrinth {
            offset
        } else {
            0
        };
        let total = if platform == Platform::Modrinth {
            array.len()
        } else {
            value["pagination"]["totalCount"]
                .as_u64()
                .unwrap_or(array.len() as u64) as usize
        };
        let versions = array
            .iter()
            .skip(skip)
            .take(50)
            .filter(|v| v["isServerPack"] != true && v["status"] != "draft")
            .map(|v| provider::parse_version(v, platform))
            .collect::<Result<Vec<_>>>()?;
        Ok(ModpackVersions {
            versions,
            more: offset + 50 < total,
        })
    }

    pub async fn download_modpack(
        &self,
        platform: Platform,
        project: &str,
        version: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<ModpackArchive> {
        let _operation = self.0.operation.lock().await;
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        sink(Event::workflow("pack_archive_resolve", "解析整合包文件"));
        let api = Provider::new(self, platform)?;
        check_project(&api, platform, project, cancel).await?;
        let selected = api.version(Some(project), version, cancel).await?;
        if selected.project != project {
            bail!("整合包版本不属于所选项目");
        }
        let file = archive_file(&selected, platform)?;
        let extension = if platform == Platform::Modrinth {
            "mrpack"
        } else {
            "zip"
        };
        let hash = file.sha1.to_ascii_lowercase();
        let path = self
            .root()
            .join("cache/modpacks")
            .join(format!("{hash}.{extension}"));
        let mut downloader = Downloader::new(self.root(), self.settings())?;
        if platform == Platform::Curseforge {
            downloader = downloader.with_curseforge_key(
                self.root(),
                &self.settings(),
                crate::curseforge::key()?,
            )?;
        }
        sink(Event::workflow("pack_archive_files", "下载整合包文件"));
        downloader
            .batch(
                vec![FileSpec {
                    url: file.url.clone(),
                    path: path.clone(),
                    hash: hash.clone(),
                    sha256: false,
                    size: file.size,
                }],
                cancel,
                sink,
            )
            .await?;
        sink(Event::workflow("pack_archive_preview", "读取整合包信息"));
        let launcher = self.clone();
        let archive = path.clone();
        let preview =
            tokio::task::spawn_blocking(move || launcher.preview_modpack(&archive)).await??;
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        if preview.pack.format
            != if platform == Platform::Modrinth {
                "modrinth"
            } else {
                "curseforge"
            }
        {
            bail!("整合包内容与下载平台不符");
        }
        Ok(ModpackArchive { path, preview })
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    #[test]
    fn curseforge_categories_exclude_classes_and_other_resource_types() {
        let categories = parse_categories(
            &json!({"data":[
                {"id":4471,"name":"Modpacks","isClass":true},
                {"id":4476,"name":"Magic","classId":4471,"parentCategoryId":4471},
                {"id":4477,"name":"Adventure and RPG","parentCategoryId":4471},
                {"id":412,"name":"Technology","classId":6},
                {"id":4476,"name":"Magic","classId":4471}
            ]}),
            Platform::Curseforge,
        )
        .unwrap();
        assert_eq!(
            categories
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()))
                .collect::<Vec<_>>(),
            [("4477", "Adventure and RPG"), ("4476", "Magic")]
        );
    }
    #[test]
    fn curseforge_sort_and_category_match_the_official_search_contract() {
        for (sort, expected) in [
            (ModpackSort::Relevance, "2"),
            (ModpackSort::Downloads, "6"),
            (ModpackSort::Updated, "3"),
            (ModpackSort::Newest, "11"),
        ] {
            let (path, args) = search_parameters(
                Platform::Curseforge,
                "ATM",
                20,
                &ModpackFilters {
                    sort,
                    category: "4476".into(),
                },
            )
            .unwrap();
            assert_eq!(path, "mods/search");
            let args: std::collections::HashMap<_, _> = args.into_iter().collect();
            assert_eq!(args["classId"], "4471");
            assert_eq!(args["categoryId"], "4476");
            assert_eq!(args["sortField"], expected);
            assert_eq!(args["index"], "20");
        }
        assert!(
            search_parameters(
                Platform::Curseforge,
                "",
                0,
                &ModpackFilters {
                    category: "magic".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
