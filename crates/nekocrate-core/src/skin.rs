use crate::{model::AccountSkin, storage};
use anyhow::{Context, Result, ensure};
use reqwest::Url;
use std::{path::Path, time::Duration};

const MAX_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn profile_skin(profile: &serde_json::Value) -> Option<AccountSkin> {
    let active = profile["skins"]
        .as_array()?
        .iter()
        .find(|skin| skin["state"] == "ACTIVE")?;
    Some(AccountSkin {
        url: active["url"].as_str()?.to_owned(),
        slim: active["variant"] == "SLIM",
    })
}

fn texture_url(value: &str) -> Result<(Url, String)> {
    let mut url = Url::parse(value).context("皮肤地址无效")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some("textures.minecraft.net")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "皮肤地址不是 Minecraft 纹理服务"
    );
    let hash = url
        .path()
        .strip_prefix("/texture/")
        .context("皮肤纹理路径无效")?
        .to_owned();
    ensure!(
        (32..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "皮肤纹理标识无效"
    );
    url.set_scheme("https")
        .map_err(|_| anyhow::anyhow!("无法升级皮肤地址到 HTTPS"))?;
    Ok((url, hash))
}

fn validate_png(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 33
            && bytes.len() <= MAX_BYTES
            && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            && &bytes[12..16] == b"IHDR",
        "皮肤文件不是有效的 PNG"
    );
    let width = u32::from_be_bytes(bytes[16..20].try_into()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into()?);
    ensure!(
        (64..=1024).contains(&width)
            && width.is_power_of_two()
            && (height == width || height == width / 2),
        "皮肤尺寸不受支持"
    );
    Ok(())
}

pub async fn load(
    root: &Path,
    skin: &AccountSkin,
    settings: &crate::model::Settings,
) -> Result<Vec<u8>> {
    let (url, hash) = texture_url(&skin.url)?;
    let cache = root.join("cache/skins").join(format!("{hash}.png"));
    if let Ok(bytes) = tokio::fs::read(&cache).await
        && validate_png(&bytes).is_ok()
    {
        return Ok(bytes);
    }
    // Texture requests carry no account credentials and cannot redirect to another host.
    let client = crate::network::client_builder(root, settings)?
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    ensure!(
        response.status().is_success(),
        "皮肤服务返回重定向，未加载纹理"
    );
    ensure!(
        response.content_length().unwrap_or(0) <= MAX_BYTES as u64,
        "皮肤文件过大"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len() + chunk.len() <= MAX_BYTES, "皮肤文件过大");
        bytes.extend_from_slice(&chunk);
    }
    validate_png(&bytes)?;
    storage::atomic_write(&cache, &bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_active_skin_and_preserves_slim_model() {
        let profile = serde_json::json!({"skins":[
            {"state":"INACTIVE","url":"old","variant":"CLASSIC"},
            {"state":"ACTIVE","url":"current","variant":"SLIM"}
        ]});
        let skin = profile_skin(&profile).unwrap();
        assert_eq!(skin.url, "current");
        assert!(skin.slim);
        assert!(profile_skin(&serde_json::json!({"name":"old account"})).is_none());
    }
    #[test]
    fn only_fetches_official_texture_paths_over_https() {
        let hash = "a".repeat(64);
        let valid = format!("http://textures.minecraft.net/texture/{hash}");
        assert_eq!(texture_url(&valid).unwrap().0.scheme(), "https");
        for url in [
            valid.replace("textures.minecraft.net", "localhost"),
            valid.replace("textures.minecraft.net", "textures.minecraft.net.evil.test"),
            valid.replace("textures.minecraft.net", "user@textures.minecraft.net"),
            valid.replace("textures.minecraft.net", "textures.minecraft.net:8080"),
            format!("{valid}?redirect=1"),
            valid.replace(&hash, "../../database.json"),
        ] {
            assert!(texture_url(&url).is_err(), "accepted {url}");
        }
    }
    #[test]
    fn rejects_non_images_and_unbounded_dimensions() {
        assert!(validate_png(b"not a png").is_err());
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&64u32.to_be_bytes());
        png[20..24].copy_from_slice(&32u32.to_be_bytes());
        assert!(validate_png(&png).is_ok());
        png[20..24].copy_from_slice(&2048u32.to_be_bytes());
        assert!(validate_png(&png).is_err());
    }
    #[test]
    fn old_accounts_remain_readable_without_skin_metadata() {
        let account: crate::model::Account = serde_json::from_value(serde_json::json!({
            "id":"legacy","username":"Player","expires_at":0,"client_id":"client","owned":true
        }))
        .unwrap();
        assert!(account.skin.is_none());
    }
}
