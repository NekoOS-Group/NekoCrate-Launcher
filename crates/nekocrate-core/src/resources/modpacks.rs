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
        range(query, offset)?;
        let api = Provider::new(self, platform)?;
        let (path, args) = if platform == Platform::Modrinth {
            (
                "search",
                vec![
                    ("query", query.into()),
                    ("offset", offset.to_string()),
                    ("limit", "20".into()),
                    ("facets", json!([["project_type:modpack"]]).to_string()),
                ],
            )
        } else {
            (
                "mods/search",
                vec![
                    ("gameId", "432".into()),
                    ("classId", "4471".into()),
                    ("searchFilter", query.into()),
                    ("index", offset.to_string()),
                    ("pageSize", "20".into()),
                    ("sortField", "2".into()),
                    ("sortOrder", "desc".into()),
                ],
            )
        };
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
        if platform == Platform::Curseforge
            && let Some(key) = self.curseforge_key()?
        {
            downloader = downloader.with_curseforge_key(self.root(), &self.settings(), &key)?;
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
