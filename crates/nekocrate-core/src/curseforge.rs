//! Application credential supplied by the release environment, never by the UI.
use anyhow::{Context, Result};

pub(crate) const UNAVAILABLE: &str = "此构建暂不支持 CurseForge，请使用官方发行版";
pub(crate) const ACCESS_DENIED: &str = "CurseForge 服务暂不可用，请稍后重试或更新启动器";

pub(crate) fn key() -> Result<&'static str> {
    // Rust tracks option_env! inputs, so restoring a Cargo cache cannot retain
    // an old credential after the build secret is changed or removed.
    build_key(option_env!("CURSEFORGE_API_KEY"))
}

fn build_key(value: Option<&str>) -> Result<&str> {
    value
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4096
                && value.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .context(UNAVAILABLE)
}

/// The two official file CDNs can disagree about an otherwise available file.
/// Keep the API's URL first; only try the other host after that source fails.
pub(crate) fn download_fallback(value: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = match url.host_str()? {
        "edge.forgecdn.net" => "mediafilez.forgecdn.net",
        "mediafilez.forgecdn.net" => "edge.forgecdn.net",
        _ => return None,
    };
    let parts: Vec<_> = url.path_segments()?.collect();
    if parts.len() != 4
        || parts[0] != "files"
        || parts[1..3]
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
        || parts[3].is_empty()
    {
        return None;
    }
    url.set_host(Some(host)).ok()?;
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_cdn_fallback_keeps_the_exact_path_and_rejects_unrelated_or_signed_urls() {
        let path = "/files/9409/902/Verity%20Craft%2B15.3.0.zip";
        let edge = format!("https://edge.forgecdn.net{path}");
        let media = format!("https://mediafilez.forgecdn.net{path}");
        assert_eq!(download_fallback(&edge).as_deref(), Some(media.as_str()));
        assert_eq!(download_fallback(&media).as_deref(), Some(edge.as_str()));
        for url in [
            format!("{edge}?signature=private"),
            format!("{edge}#fragment"),
            format!("http://edge.forgecdn.net{path}"),
            format!("https://edge.forgecdn.net:8443{path}"),
            format!("https://user:private@edge.forgecdn.net{path}"),
            format!("https://edge.forgecdn.net.example.org{path}"),
            format!("https://cdn.modrinth.com{path}"),
            "https://edge.forgecdn.net/attachments/9409/902/icon.png".into(),
            "https://edge.forgecdn.net/files/a/902/pack.zip".into(),
            "https://edge.forgecdn.net/files/9409/902/".into(),
        ] {
            assert_eq!(download_fallback(&url), None, "{url}");
        }
    }

    #[test]
    fn build_credential_is_validated_without_disclosing_it_in_errors() {
        assert_eq!(
            build_key(Some("  test-app-credential  ")).unwrap(),
            "test-app-credential"
        );
        for value in [
            None,
            Some(""),
            Some("   "),
            Some("private\ncredential"),
            Some("non-ascii-密钥"),
        ] {
            assert_eq!(build_key(value).unwrap_err().to_string(), UNAVAILABLE);
        }
        assert_eq!(
            build_key(Some(&"x".repeat(4097))).unwrap_err().to_string(),
            UNAVAILABLE
        );
    }
}
