//! Display-only platform metadata and bounded, cached project artwork.
use super::*;
use crate::network;
use provider::{Provider, identifier, project};
use serde_json::Value;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct ProjectDetails {
    pub refresh_required: bool,
    pub project: Project,
    pub body: String,
    pub body_format: &'static str,
    pub authors: Vec<String>,
    pub categories: Vec<String>,
    pub updated: Option<String>,
    pub license: Option<String>,
    pub license_url: Option<String>,
    pub links: Vec<ProjectLink>,
    pub gallery: Vec<ProjectImage>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectLink {
    pub label: &'static str,
    pub url: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectImage {
    pub url: String,
    pub title: String,
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().into()
}
fn array(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}
fn strings(v: &Value) -> Vec<String> {
    array(v)
        .filter_map(Value::as_str)
        .take(32)
        .map(str::to_owned)
        .collect()
}
fn web_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.into())
}
pub(super) fn image_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    (value.len() <= 2048
        && url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "cdn.modrinth.com"
                    | "media.forgecdn.net"
                    | "mediafilez.forgecdn.net"
                    | "raw.githubusercontent.com"
                    | "user-images.githubusercontent.com"
                    | "i.imgur.com"
            )
        ))
    .then(|| url.into())
}
fn details(v: &Value, platform: Platform, body: String, authors: Vec<String>) -> ProjectDetails {
    let mr = platform == Platform::Modrinth;
    let mut links = Vec::new();
    for (label, value) in if mr {
        [
            ("源码", &v["source_url"]),
            ("文档", &v["wiki_url"]),
            ("反馈问题", &v["issues_url"]),
        ]
    } else {
        [
            ("源码", &v["links"]["sourceUrl"]),
            ("文档", &v["links"]["wikiUrl"]),
            ("反馈问题", &v["links"]["issuesUrl"]),
        ]
    } {
        if let Some(url) = value.as_str().and_then(web_url) {
            links.push(ProjectLink { label, url });
        }
    }
    let gallery = array(&v[if mr { "gallery" } else { "screenshots" }])
        .take(12)
        .filter_map(|item| {
            Some(ProjectImage {
                url: image_url(
                    item[if mr { "url" } else { "thumbnailUrl" }]
                        .as_str()
                        .or(item["url"].as_str())?,
                )?,
                title: text(item, "title"),
            })
        })
        .collect();
    ProjectDetails {
        refresh_required: false,
        project: project(v, platform),
        // Detailed descriptions are display data, not an unbounded document viewer.
        body: body.chars().take(256_000).collect(),
        body_format: if mr { "markdown" } else { "html" },
        authors,
        categories: if mr {
            strings(&v["categories"])
        } else {
            array(&v["categories"])
                .take(32)
                .map(|c| text(c, "name"))
                .filter(|s| !s.is_empty())
                .collect()
        },
        updated: v[if mr { "updated" } else { "dateModified" }]
            .as_str()
            .map(str::to_owned),
        license: v["license"]["name"]
            .as_str()
            .or(v["license"]["id"].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        license_url: v["license"]["url"].as_str().and_then(web_url),
        links,
        gallery,
    }
}
async fn curseforge_id(
    api: &Provider,
    id: &str,
    refresh: bool,
    cancel: &CancellationToken,
) -> Result<String> {
    if id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(id.into());
    }
    // Match the declared URL's exact slug and class, never a fuzzy filename/name search.
    let found = api
        .get_presentation(
            &format!("mods/search?gameId=432&classId=6&slug={id}"),
            refresh,
            None,
            cancel,
        )
        .await?
        .0;
    exact_curseforge_id(&found, id).context("未找到对应的资源项目")
}
fn exact_curseforge_id(found: &Value, slug: &str) -> Option<String> {
    array(&found["data"])
        .find(|v| {
            v["slug"].as_str() == Some(slug)
                && v["gameId"].as_u64() == Some(432)
                && v["classId"].as_u64() == Some(6)
        })
        .and_then(|v| v["id"].as_u64())
        .map(|id| id.to_string())
}
impl Launcher {
    pub async fn resource_project_summary(
        &self,
        platform: Platform,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<Project> {
        identifier(id)?;
        let api = Provider::new(self, platform)?;
        let path = if platform == Platform::Modrinth {
            format!("project/{id}")
        } else {
            let id = curseforge_id(&api, id, false, cancel).await?;
            format!("mods/{id}")
        };
        let (mut value, stale) = api.get_presentation(&path, false, None, cancel).await?;
        // Icon fallback shares the project endpoint (and its long description).
        // Keep its revalidation interval aligned with the presentation cache.
        if stale {
            value = api.get_presentation(&path, true, None, cancel).await?.0;
        }
        let value = if platform == Platform::Modrinth {
            &value
        } else {
            &value["data"]
        };
        Ok(project(value, platform))
    }
    pub async fn resource_project(
        &self,
        platform: Platform,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<ProjectDetails> {
        self.resource_project_with_refresh(platform, id, false, cancel)
            .await
    }
    pub async fn resource_project_with_refresh(
        &self,
        platform: Platform,
        id: &str,
        refresh: bool,
        cancel: &CancellationToken,
    ) -> Result<ProjectDetails> {
        identifier(id)?;
        if cancel.is_cancelled() {
            bail!("操作已取消");
        }
        let api = Provider::new(self, platform)?;
        if platform == Platform::Modrinth {
            // Author lookup is optional; an unavailable team endpoint must not hide the body.
            let path = format!("project/{id}");
            let members = format!("project/{id}/members");
            let cached = (!refresh).then(|| api.cached_presentation(&path)).flatten();
            let ((v, stale), (team, team_stale)) = if let Some(cached) = cached {
                // A missing optional author list must not delay an already cached body.
                (
                    cached,
                    api.cached_presentation(&members)
                        .unwrap_or((Value::Null, true)),
                )
            } else {
                let (v, team) = tokio::join!(
                    api.get_presentation(&path, refresh, None, cancel),
                    tokio::time::timeout(
                        Duration::from_secs(2),
                        api.get_presentation(&members, refresh, None, cancel)
                    )
                );
                (
                    v?,
                    team.ok()
                        .and_then(Result::ok)
                        .or_else(|| api.cached_presentation(&members))
                        .unwrap_or((Value::Null, false)),
                )
            };
            let authors = array(&team)
                .take(16)
                .filter_map(|a| {
                    a["user"]["name"]
                        .as_str()
                        .or(a["user"]["username"].as_str())
                })
                .map(str::to_owned)
                .collect();
            let mut result = details(&v, platform, text(&v, "body"), authors);
            result.refresh_required = stale || team_stale;
            Ok(result)
        } else {
            let id = curseforge_id(&api, id, refresh, cancel).await?;
            // Distribution restrictions affect downloads, not reading a project's page.
            let path = format!("mods/{id}");
            let description = format!("mods/{id}/description");
            let (v, stale) = api.get_presentation(&path, refresh, None, cancel).await?;
            let revision = serde_json::json!([v["data"]["dateModified"], v["data"]["mainFileId"]]);
            let (body, body_stale) = api
                .get_presentation(&description, refresh, Some(&revision), cancel)
                .await?;
            let authors = array(&v["data"]["authors"])
                .take(16)
                .map(|a| text(a, "name"))
                .collect();
            let mut result = details(&v["data"], platform, text(&body, "data"), authors);
            result.refresh_required = stale || body_stale;
            Ok(result)
        }
    }
    pub async fn resource_image(
        &self,
        source: &str,
        cancel: &CancellationToken,
    ) -> Result<(Vec<u8>, &'static str)> {
        let url = image_url(source).context("不支持的项目图片地址")?;
        let hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, url.as_bytes()));
        let path = self.root().join("metadata/resources/images").join(hash);
        if let Ok(meta) = fs::metadata(&path)
            && meta.len() <= MAX_IMAGE_BYTES as u64
            && let Ok(bytes) = fs::read(&path)
            && let Ok(mime) = image_type(&bytes)
        {
            return Ok((bytes, mime));
        }
        // Never attach platform API keys or account credentials to image requests.
        let client = network::client_builder(self.root(), &self.settings())?
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 3 && image_url(attempt.url().as_str()).is_some() {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .timeout(Duration::from_secs(15))
            .build()?;
        let bytes = tokio::select! {
            _ = cancel.cancelled() => bail!("操作已取消"),
            result = async {
                let mut response = client.get(&url).send().await?.error_for_status()?;
                anyhow::ensure!(!response.status().is_redirection(), "项目图片跳转无效");
                anyhow::ensure!(response.content_length().is_none_or(|n| n <= MAX_IMAGE_BYTES as u64), "项目图片过大");
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    anyhow::ensure!(bytes.len() + chunk.len() <= MAX_IMAGE_BYTES, "项目图片过大");
                    bytes.extend(chunk);
                }
                Ok::<_, anyhow::Error>(bytes)
            } => result.map_err(network::explain_error)?,
        };
        let mime = image_type(&bytes)?;
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = storage::atomic_write(&path, &bytes);
        Ok((bytes, mime))
    }
}
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) fn image_type(bytes: &[u8]) -> Result<&'static str> {
    anyhow::ensure!(bytes.len() <= MAX_IMAGE_BYTES, "项目图片过大");
    let dimension = |w: u32, h: u32| -> Result<()> {
        anyhow::ensure!(
            w > 0 && h > 0 && w <= 8192 && h <= 8192 && u64::from(w) * u64::from(h) <= 16_777_216,
            "项目图片尺寸不受支持"
        );
        Ok(())
    };
    if bytes.len() >= 33 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        dimension(
            u32::from_be_bytes(bytes[16..20].try_into()?),
            u32::from_be_bytes(bytes[20..24].try_into()?),
        )?;
        return Ok("image/png");
    }
    if bytes.len() >= 13 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        dimension(
            u16::from_le_bytes(bytes[6..8].try_into()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into()?) as u32,
        )?;
        return Ok("image/gif");
    }
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        let le24 = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], 0]);
        let (w, h) = match &bytes[12..16] {
            b"VP8X" => (le24(&bytes[24..27]) + 1, le24(&bytes[27..30]) + 1),
            b"VP8L" if bytes[20] == 0x2f => {
                let b = u32::from_le_bytes(bytes[21..25].try_into()?);
                ((b & 0x3fff) + 1, ((b >> 14) & 0x3fff) + 1)
            }
            b"VP8 " if bytes[23..26] == [0x9d, 0x01, 0x2a] => (
                (u16::from_le_bytes(bytes[26..28].try_into()?) & 0x3fff) as u32,
                (u16::from_le_bytes(bytes[28..30].try_into()?) & 0x3fff) as u32,
            ),
            _ => bail!("项目图片格式无效"),
        };
        dimension(w, h)?;
        return Ok("image/webp");
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut offset = 2;
        while offset + 4 <= bytes.len() {
            if bytes[offset] != 0xff {
                break;
            }
            let marker = bytes[offset + 1];
            if marker == 0xff {
                offset += 1;
                continue;
            }
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let length = u16::from_be_bytes(bytes[offset + 2..offset + 4].try_into()?) as usize;
            if length < 2 || offset + 2 + length > bytes.len() {
                break;
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
                && length >= 7
            {
                dimension(
                    u16::from_be_bytes(bytes[offset + 7..offset + 9].try_into()?) as u32,
                    u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into()?) as u32,
                )?;
                return Ok("image/jpeg");
            }
            offset += length + 2;
        }
    }
    bail!("项目图片格式不受支持")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn curseforge_slug_resolution_requires_exact_game_class_and_slug() {
        let value = json!({"data":[
            {"id":1,"gameId":432,"classId":4471,"slug":"cat"},
            {"id":2,"gameId":1,"classId":6,"slug":"cat"},
            {"id":3,"gameId":432,"classId":6,"slug":"other"},
            {"id":4,"gameId":432,"classId":6,"slug":"cat"}
        ]});
        assert_eq!(exact_curseforge_id(&value, "cat").as_deref(), Some("4"));
        assert!(exact_curseforge_id(&value, "missing").is_none());
    }

    #[tokio::test]
    #[ignore = "requires public Modrinth APIs; no real account or game directory is used"]
    async fn live_project_description_and_icon() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let cancel = CancellationToken::new();
        let details = launcher
            .resource_project(Platform::Modrinth, "sodium", &cancel)
            .await
            .unwrap();
        assert_eq!(details.project.name, "Sodium");
        assert!(!details.body.is_empty());
        let url = details.project.icon_url.unwrap();
        let (image, mime) = launcher.resource_image(&url, &cancel).await.unwrap();
        assert!(mime.starts_with("image/"));
        assert!(!image.is_empty());
        let cached = launcher.resource_image(&url, &cancel).await.unwrap();
        assert_eq!(cached.0, image);
    }

    #[test]
    fn platform_details_preserve_rich_body_and_extract_available_metadata() {
        let mr = details(
            &json!({
                "id":"cat", "title":"Cat", "project_type":"mod", "body":"# Cat",
                "icon_url":"https://cdn.modrinth.com/cat.png", "categories":["adventure"],
                "license":{"id":"MIT", "name":"MIT License", "url":"https://example.org/license"}, "updated":"2026-11-03T00:00:00Z",
                "source_url":"https://github.com/example/cat", "wiki_url":"file:///tmp/secret",
                "gallery":[{"url":"https://cdn.modrinth.com/cat.webp", "title":"Cat world"}, {"url":"http://localhost/private"}]
            }),
            Platform::Modrinth,
            "# Cat".into(),
            vec!["Neko".into()],
        );
        assert_eq!(mr.body_format, "markdown");
        assert_eq!(mr.body, "# Cat");
        assert_eq!(mr.authors, ["Neko"]);
        assert_eq!(mr.categories, ["adventure"]);
        assert_eq!(mr.license.as_deref(), Some("MIT License"));
        assert_eq!(
            mr.license_url.as_deref(),
            Some("https://example.org/license")
        );
        assert_eq!(mr.links.len(), 1);
        assert_eq!(mr.gallery.len(), 1);
        assert!(mr.project.icon_url.is_some());
        let cf = details(
            &json!({
                "id":123, "name":"Cat pack", "allowModDistribution":false,
                "logo":{"thumbnailUrl":"https://media.forgecdn.net/cat.png"},
                "categories":[{"name":"Adventure"}], "links":{"websiteUrl":"https://www.curseforge.com/minecraft/modpacks/cat"},
                "screenshots":[{"url":"https://mediafilez.forgecdn.net/cat.png", "title":"Preview"}]
            }),
            Platform::Curseforge,
            "<p>Cat pack</p>".into(),
            vec!["Neko".into()],
        );
        assert_eq!(cf.project.id, "123");
        assert_eq!(cf.body_format, "html");
        assert_eq!(cf.categories, ["Adventure"]);
        assert_eq!(cf.gallery.len(), 1);
        assert!(cf.license_url.is_none());
        assert!(cf.project.icon_url.is_some());
        // Existing catalog caches from before icons remain readable.
        let old: Project = serde_json::from_value(
            json!({"id":"cat", "name":"Cat", "description":"", "downloads":0, "url":""}),
        )
        .unwrap();
        assert!(old.icon_url.is_none());
    }

    #[test]
    fn images_reject_untrusted_hosts_active_formats_and_unbounded_dimensions() {
        for url in [
            "http://cdn.modrinth.com/a.png",
            "https://cdn.modrinth.com.evil.test/a.png",
            "https://cdn.modrinth.com:8443/a.png",
            "https://key@cdn.modrinth.com/a.png",
            "file:///tmp/a",
            "http://127.0.0.1/a",
        ] {
            assert!(image_url(url).is_none(), "{url}");
        }
        assert!(image_url("https://media.forgecdn.net/avatars/1/2.png").is_some());
        assert!(image_type(b"<svg onload='alert(1)'/>").is_err());
        assert!(image_type(b"<html><img src='http://localhost'/></html>").is_err());
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&64u32.to_be_bytes());
        png[20..24].copy_from_slice(&64u32.to_be_bytes());
        assert_eq!(image_type(&png).unwrap(), "image/png");
        png[16..20].copy_from_slice(&9999u32.to_be_bytes());
        assert!(image_type(&png).is_err());
        assert!(image_type(&png[..24]).is_err());
        assert_eq!(
            image_type(b"GIF89a\x40\x00\x40\x00\x00\x00\x00").unwrap(),
            "image/gif"
        );
        let jpeg = [0xff, 0xd8, 0xff, 0xc0, 0, 7, 8, 0, 64, 0, 64];
        assert_eq!(image_type(&jpeg).unwrap(), "image/jpeg");
        let mut webp = vec![0; 30];
        webp[..4].copy_from_slice(b"RIFF");
        webp[8..16].copy_from_slice(b"WEBPVP8X");
        webp[24] = 63;
        webp[27] = 63;
        assert_eq!(image_type(&webp).unwrap(), "image/webp");
    }

    #[tokio::test]
    async fn artwork_cache_is_validated_and_reused_without_network() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let url = "https://cdn.modrinth.com/test/cached.gif";
        let hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, url.as_bytes()));
        let path = launcher.root().join("metadata/resources/images").join(hash);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = b"GIF89a\x40\x00\x40\x00\x00\x00\x00";
        fs::write(&path, bytes).unwrap();
        let (image, mime) = launcher
            .resource_image(url, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(image, bytes);
        assert_eq!(mime, "image/gif");
    }
}
