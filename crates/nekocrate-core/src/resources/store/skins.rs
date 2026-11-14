//! Anonymous, read-only skin catalogs. Account and skin-upload APIs are not used.
use crate::{Launcher, network, storage};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, fs, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Littleskin,
    Elyby,
    Minecraftskins,
    Mineskin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skin {
    pub source: Source,
    pub id: String,
    pub name: String,
    pub author: String,
    pub slim: Option<bool>,
    // Only immutable, provider-owned texture URLs; never a general-purpose proxy.
    pub texture: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Page {
    pub skins: Vec<Skin>,
    pub next: Option<String>,
}

fn hex_id(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn numeric_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 12 && value.bytes().all(|c| c.is_ascii_digit())
}
fn slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn page_number(cursor: &str) -> Result<usize> {
    let page = if cursor.is_empty() {
        1
    } else {
        cursor.parse().context("页码无效")?
    };
    ensure!((1..=10000).contains(&page), "页码无效");
    Ok(page)
}
fn catalog_url(source: Source, query: &str, cursor: &str) -> Result<reqwest::Url> {
    ensure!(
        query.chars().count() <= 100 && !query.chars().any(char::is_control),
        "搜索内容过长或包含无效字符"
    );
    let mut url = reqwest::Url::parse(match source {
        Source::Littleskin => "https://littleskin.cn/skinlib/list",
        Source::Elyby => "https://ely.by/api/legacy/skins",
        Source::Minecraftskins => "https://www.minecraftskins.net/",
        Source::Mineskin => "https://api.mineskin.org/v2/skins",
    })?;
    if source == Source::Mineskin {
        ensure!(cursor.is_empty() || hex_id(cursor, 32), "皮肤分页游标无效");
        let mut params = url.query_pairs_mut();
        params.append_pair("size", "24");
        if !cursor.is_empty() {
            params.append_pair("after", cursor);
        }
        if !query.is_empty() {
            params.append_pair("filter", query);
        }
    } else {
        let page = page_number(cursor)?.to_string();
        match source {
            Source::Littleskin => {
                url.query_pairs_mut()
                    .append_pair("filter", "skin")
                    .append_pair("sort", "time")
                    .append_pair("page", &page)
                    .append_pair("keyword", query);
            }
            Source::Elyby => {
                let mut params = url.query_pairs_mut();
                params.append_pair("page", &page);
                // Ely searches tags, not names. An array is required by its API.
                if !query.is_empty() {
                    params.append_pair("tags[]", query);
                }
            }
            Source::Minecraftskins => {
                let mut path = url.path_segments_mut().expect("HTTP base URL");
                path.clear();
                if !query.is_empty() {
                    path.push("search").push(query);
                    if page != "1" {
                        path.push(&page);
                    }
                } else if page != "1" {
                    path.push("page").push(&page);
                }
            }
            Source::Mineskin => unreachable!(),
        }
    }
    Ok(url)
}

impl Skin {
    fn validate(&self) -> Result<()> {
        ensure!(
            match self.source {
                Source::Littleskin | Source::Elyby => numeric_id(&self.id),
                Source::Minecraftskins => slug(&self.id),
                Source::Mineskin => hex_id(&self.id, 32),
            },
            "皮肤标识无效"
        );
        ensure!(
            self.name.len() <= 1024 && self.author.len() <= 1024,
            "皮肤信息过长"
        );
        if matches!(self.source, Source::Elyby | Source::Mineskin) {
            self.texture_url()?;
        } else {
            ensure!(self.texture.is_none(), "皮肤下载地址无效");
        }
        Ok(())
    }
    fn texture_url(&self) -> Result<reqwest::Url> {
        let url = reqwest::Url::parse(self.texture.as_deref().context("皮肤缺少下载地址")?)?;
        ensure!(
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "皮肤下载地址无效"
        );
        let valid = match (self.source, url.host_str()) {
            (Source::Elyby, Some("ely.by")) => url
                .path()
                .strip_prefix("/storage/skins/")
                .and_then(|s| s.strip_suffix(".png"))
                .is_some_and(|s| hex_id(s, 32)),
            (Source::Mineskin, Some("textures.minecraft.net")) => {
                url.path().strip_prefix("/texture/").is_some_and(|s| {
                    (32..=64).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_hexdigit())
                })
            }
            _ => false,
        };
        ensure!(valid, "皮肤下载地址无效");
        Ok(url)
    }
}
fn string(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(200)
        .collect()
}
fn parse_json(source: Source, body: &[u8], cursor: &str) -> Result<Page> {
    let value: Value = serde_json::from_slice(body).context("无法读取皮肤目录")?;
    let key = match source {
        Source::Littleskin => "data",
        Source::Elyby => "items",
        _ => "skins",
    };
    let rows = value[key].as_array().context("无法读取皮肤目录")?;
    let mut skins = Vec::new();
    for row in rows.iter().take(128) {
        let skin = match source {
            Source::Littleskin => {
                if row["public"] != true
                    || row["hd"] == true
                    || row["protected"] == true
                    || !matches!(row["type"].as_str(), Some("steve" | "alex"))
                {
                    continue;
                }
                Skin {
                    source,
                    id: row["tid"].as_u64().context("皮肤标识无效")?.to_string(),
                    name: string(row, "name"),
                    author: string(row, "nickname"),
                    slim: Some(row["type"] == "alex"),
                    texture: None,
                }
            }
            Source::Elyby => {
                let id = row["id"].as_u64().context("皮肤标识无效")?.to_string();
                let tags = row["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" · ");
                Skin {
                    source,
                    name: if tags.is_empty() {
                        format!("Ely.by · {id}")
                    } else {
                        tags.chars().take(200).collect()
                    },
                    id,
                    author: String::new(),
                    slim: row["is_slim"].as_bool(),
                    texture: Some(string(row, "skin_url")),
                }
            }
            Source::Mineskin => {
                let id = string(row, "uuid");
                let name = string(row, "name");
                Skin {
                    source,
                    name: if name.is_empty() {
                        format!("MineSkin · {}", string(row, "shortId"))
                    } else {
                        name
                    },
                    id,
                    author: String::new(),
                    slim: None,
                    texture: Some(format!(
                        "https://textures.minecraft.net/texture/{}",
                        string(row, "texture")
                    )),
                }
            }
            Source::Minecraftskins => unreachable!(),
        };
        skin.validate()?;
        skins.push(skin);
    }
    let next = match source {
        Source::Littleskin => value["next_page_url"]
            .as_str()
            .map(|_| (page_number(cursor).unwrap_or(1) + 1).to_string()),
        Source::Elyby => value["next"]
            .as_u64()
            .filter(|n| *n > value["current"].as_u64().unwrap_or(1))
            .map(|n| n.to_string()),
        Source::Mineskin => value
            .pointer("/pagination/next/after")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Source::Minecraftskins => None,
    };
    // Validate next tokens, but never follow a URL returned by a provider.
    if let Some(next) = &next {
        catalog_url(source, "", next)?;
    }
    Ok(Page { skins, next })
}

fn html_text(value: &str) -> String {
    let entities = regex::Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|amp|quot|apos|lt|gt|nbsp);")
        .expect("entity regex");
    entities
        .replace_all(value, |c: &regex::Captures<'_>| {
            let ch = match &c[1] {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "nbsp" => Some(' '),
                s if s.starts_with("#x") => u32::from_str_radix(&s[2..], 16)
                    .ok()
                    .and_then(char::from_u32),
                s => s[1..].parse::<u32>().ok().and_then(char::from_u32),
            };
            ch.map(|c| c.to_string()).unwrap_or_else(|| c[0].to_owned())
        })
        .trim()
        .chars()
        .take(200)
        .collect()
}
fn parse_html(html: &str, cursor: &str) -> Result<Page> {
    ensure!(
        html.contains("hero-title") && !html.contains("cf-chl-"),
        "无法读取皮肤目录，请重试或切换来源"
    );
    let title = regex::Regex::new(r#"class="card-title">([^<]+)</h2>"#)?;
    let link = regex::Regex::new(r#"href="/([A-Za-z0-9_-]+)(?:\?[^"<>]*)?""#)?;
    let author = regex::Regex::new(r#"Designed by ([^<]+)"#)?;
    let mut skins = Vec::new();
    let mut seen = HashSet::new();
    for card in html.split(r#"<div class="card">"#).skip(1).take(128) {
        let Some(name) = title.captures(card) else {
            continue;
        };
        let Some(link) = link.captures(card) else {
            continue;
        };
        let id = link[1].to_owned();
        if !seen.insert(id.clone()) {
            continue;
        }
        let skin = Skin {
            source: Source::Minecraftskins,
            id: id.clone(),
            name: html_text(&name[1]),
            author: author
                .captures(card)
                .map(|c| html_text(&c[1]))
                .unwrap_or_default(),
            slim: if card.contains(&format!("/skineditor#{id}/3px")) {
                Some(true)
            } else if card.contains(&format!("/skineditor#{id}/4px")) {
                Some(false)
            } else {
                None
            },
            texture: None,
        };
        skin.validate()?;
        skins.push(skin);
    }
    let next = html
        .contains(r#"class="next-page" href=""#)
        .then(|| (page_number(cursor).unwrap_or(1) + 1).to_string());
    Ok(Page { skins, next })
}

fn fresh(path: &Path, ttl: u64) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(ttl))
}
fn cache_key(url: &str) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, url.as_bytes()))
}
async fn fetch(
    launcher: &Launcher,
    url: &str,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    let client = network::client_builder(launcher.root(), &launcher.settings())?
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()?;
    tokio::select! {
        _ = cancel.cancelled() => bail!("操作已取消"),
        result = async {
            let mut response = client.get(url).send().await?;
            ensure!(response.status().is_success(), "皮肤来源暂时无法访问（HTTP {}）", response.status());
            ensure!(response.content_length().unwrap_or(0) <= limit as u64, "皮肤来源响应过大");
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(bytes.len() + chunk.len() <= limit, "皮肤来源响应过大");
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        } => result.map_err(network::explain_error),
    }
}

impl Launcher {
    pub async fn store_skins(
        &self,
        source: Source,
        query: &str,
        cursor: &str,
        cancel: &CancellationToken,
    ) -> Result<Page> {
        let url = catalog_url(source, query.trim(), cursor)?;
        let cache = self.root().join(format!(
            "metadata/skin-catalogs/{}.json",
            cache_key(url.as_str())
        ));
        if fresh(&cache, 600)
            && let Ok(page) = storage::read_json::<Page>(&cache)
            && page
                .skins
                .iter()
                .all(|s| s.source == source && s.validate().is_ok())
        {
            return Ok(page);
        }
        let bytes = fetch(self, url.as_str(), 4 * 1024 * 1024, cancel).await?;
        let result = if source == Source::Minecraftskins {
            parse_html(std::str::from_utf8(&bytes)?, cursor)?
        } else {
            parse_json(source, &bytes, cursor)?
        };
        let _ = storage::write_json(&cache, &result);
        Ok(result)
    }
    async fn little_skin_url(&self, id: &str, cancel: &CancellationToken) -> Result<String> {
        let cache = self
            .root()
            .join(format!("metadata/skin-catalogs/little-{id}.json"));
        let cached = fresh(&cache, 600)
            .then(|| storage::read_json::<Value>(&cache).ok())
            .flatten();
        let info = if let Some(info) = cached {
            info
        } else {
            let bytes = fetch(
                self,
                &format!("https://littleskin.cn/skinlib/info/{id}"),
                64 * 1024,
                cancel,
            )
            .await?;
            let info: Value = serde_json::from_slice(&bytes)?;
            let _ = storage::write_json(&cache, &info);
            info
        };
        ensure!(
            info["public"] == true
                && info["protected"] != true
                && info["hd"] != true
                && matches!(info["type"].as_str(), Some("steve" | "alex")),
            "此皮肤暂不支持下载"
        );
        let hash = info["hash"].as_str().context("皮肤缺少下载地址")?;
        ensure!(hex_id(hash, 64), "皮肤下载地址无效");
        Ok(format!("https://littleskin.cn/textures/{hash}"))
    }
    pub async fn store_skin_bytes(
        &self,
        input: &Skin,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>> {
        input.validate()?;
        let url = match input.source {
            Source::Littleskin => self.little_skin_url(&input.id, cancel).await?,
            Source::Minecraftskins => {
                format!("https://www.minecraftskins.net/{}/download", input.id)
            }
            _ => input.texture_url()?.to_string(),
        };
        let cache = self
            .root()
            .join(format!("cache/skins/store/{}.png", cache_key(&url)));
        // Hash-addressed textures are immutable. A named MinecraftSkins.net skin can change.
        if (input.source != Source::Minecraftskins || fresh(&cache, 86400))
            && fs::metadata(&cache).is_ok_and(|m| m.len() <= 2 * 1024 * 1024)
            && let Ok(bytes) = fs::read(&cache)
            && crate::skin::validate_png(&bytes).is_ok()
        {
            return Ok(bytes);
        }
        let bytes = fetch(self, &url, 2 * 1024 * 1024, cancel).await?;
        crate::skin::validate_png(&bytes)?;
        storage::atomic_write(&cache, &bytes)?;
        Ok(bytes)
    }
    pub async fn store_save_skin(
        &self,
        input: &Skin,
        slim: bool,
        cancel: &CancellationToken,
    ) -> Result<crate::resource_manager::SavedSkin> {
        let bytes = self.store_skin_bytes(input, cancel).await?;
        self.save_skin_bytes(&input.name, &bytes, input.slim.unwrap_or(slim))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_requests_encode_search_and_validate_cursors() {
        let url = catalog_url(Source::Littleskin, "猫 & Steve", "2").unwrap();
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "keyword" && v == "猫 & Steve")
        );
        assert!(
            catalog_url(Source::Elyby, "cat", "1")
                .unwrap()
                .query_pairs()
                .any(|(k, v)| k == "tags[]" && v == "cat")
        );
        assert_eq!(
            catalog_url(Source::Minecraftskins, "Steve", "2")
                .unwrap()
                .as_str(),
            "https://www.minecraftskins.net/search/Steve/2"
        );
        assert!(catalog_url(Source::Mineskin, "", "../../secret").is_err());
        assert!(catalog_url(Source::Littleskin, "", "0").is_err());
        assert!(catalog_url(Source::Littleskin, "", "10001").is_err());
    }
    #[test]
    fn texture_downloads_cannot_escape_provider_hosts_or_paths() {
        let mut skin = Skin {
            source: Source::Elyby,
            id: "123".into(),
            name: "Test".into(),
            author: String::new(),
            slim: Some(true),
            texture: None,
        };
        skin.texture = Some(format!(
            "https://ely.by/storage/skins/{}.png",
            "a".repeat(32)
        ));
        assert!(skin.validate().is_ok());
        for url in [
            "https://ely.by.evil.test/a.png",
            "http://127.0.0.1/a",
            "https://ely.by/private",
            "https://user@ely.by/storage/skins/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
            "https://ely.by/storage/skins/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png?q=1",
        ] {
            skin.texture = Some(url.into());
            assert!(skin.validate().is_err(), "{url}");
        }
        skin.source = Source::Minecraftskins;
        skin.texture = None;
        for id in ["../secrets", "a/b", "a?token=secret", "a%2Fb", ""] {
            skin.id = id.into();
            assert!(skin.validate().is_err());
        }
    }
    #[test]
    fn json_catalogs_preserve_models_and_use_provider_pagination() {
        let little = json!({"data":[{"tid":1,"name":"Cat","nickname":"Author","public":true,"type":"alex","hd":false},{"tid":2,"public":true,"type":"cape"},{"tid":3,"public":true,"type":"steve","hd":true}],"next_page_url":"https://untrusted.invalid"});
        let page = parse_json(
            Source::Littleskin,
            &serde_json::to_vec(&little).unwrap(),
            "1",
        )
        .unwrap();
        assert_eq!(page.skins.len(), 1);
        assert_eq!(page.skins[0].slim, Some(true));
        assert_eq!(page.next.as_deref(), Some("2"));
        let ely = json!({"items":[{"id":1,"skin_url":format!("https://ely.by/storage/skins/{}.png","a".repeat(32)),"is_slim":false,"tags":["Cat"]}],"current":2,"next":2});
        let page = parse_json(Source::Elyby, &serde_json::to_vec(&ely).unwrap(), "2").unwrap();
        assert_eq!(page.skins[0].name, "Cat");
        assert!(page.next.is_none());
        let mine = json!({"skins":[{"uuid":"a".repeat(32),"shortId":"12345678","name":null,"texture":"b".repeat(64)}],"pagination":{"next":{"after":"c".repeat(32)}}});
        let page = parse_json(Source::Mineskin, &serde_json::to_vec(&mine).unwrap(), "").unwrap();
        assert_eq!(page.skins[0].name, "MineSkin · 12345678");
        assert_eq!(page.next, Some("c".repeat(32)));
        assert!(parse_json(Source::Mineskin, br#"{"success":false}"#, "").is_err());
    }
    #[test]
    fn html_catalog_handles_home_search_and_empty_results() {
        let html = r#"<h1 class="hero-title">Skins</h1><div class="card"><a href="/cat"></a><h2 class="card-title">Cat &amp; Kitten</h2><h3>Designed by Me</h3><a href="/skineditor#cat/3px">Edit</a><div class="card"><a class="panel-link" href="/steve?order=search"></a><h2 class="card-title">Steve</h2><a class="next-page" href="/page/2">Next</a>"#;
        let page = parse_html(html, "1").unwrap();
        assert_eq!(page.skins.len(), 2);
        assert_eq!(page.skins[0].name, "Cat & Kitten");
        assert_eq!(page.skins[0].author, "Me");
        assert_eq!(page.skins[0].slim, Some(true));
        assert_eq!(page.skins[1].id, "steve");
        assert_eq!(page.next.as_deref(), Some("2"));
        assert!(
            parse_html(
                r#"<h1 class="hero-title">Search results</h1>No results"#,
                "1"
            )
            .unwrap()
            .skins
            .is_empty()
        );
        assert!(parse_html("<title>Just a moment...</title>", "1").is_err());
    }
    #[tokio::test]
    #[ignore = "Live anonymous skin-provider check; writes only to a temporary directory"]
    async fn live_skin_catalogs_search_paginate_preview_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().join("launcher"))).unwrap();
        let cancel = CancellationToken::new();
        for source in [
            Source::Littleskin,
            Source::Elyby,
            Source::Minecraftskins,
            Source::Mineskin,
        ] {
            let page = launcher.store_skins(source, "", "", &cancel).await.unwrap();
            let skin = page.skins.first().expect("nonempty public catalog");
            let bytes = launcher.store_skin_bytes(skin, &cancel).await.unwrap();
            crate::skin::validate_png(&bytes).unwrap();
            let saved = launcher.store_save_skin(skin, true, &cancel).await.unwrap();
            assert_eq!(launcher.saved_skin_bytes(&saved.id).unwrap(), bytes);
            assert_eq!(
                launcher
                    .store_save_skin(skin, true, &cancel)
                    .await
                    .unwrap()
                    .id,
                saved.id
            );
            let next = launcher
                .store_skins(
                    source,
                    "",
                    page.next.as_deref().expect("next page"),
                    &cancel,
                )
                .await
                .unwrap();
            assert!(!next.skins.is_empty());
            assert_ne!(page.skins[0].id, next.skins[0].id);
            let search = launcher
                .store_skins(source, "Steve", "", &cancel)
                .await
                .unwrap();
            assert!(!search.skins.is_empty());
            assert_ne!(page.skins[0].id, search.skins[0].id);
            println!(
                "{source:?}: catalog {}, next {}, search {}, PNG {} bytes, saved and deduplicated",
                page.skins.len(),
                next.skins.len(),
                search.skins.len(),
                bytes.len()
            );
        }
    }
}
