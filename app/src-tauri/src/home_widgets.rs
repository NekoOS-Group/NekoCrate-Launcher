use anyhow::{Context, Result, ensure};
use nekocrate_core::{Launcher, network};
use reqwest::Url;
use serde_json::{Value, json};
use std::time::Duration;

const LIMIT: usize = 600_000;
fn feed_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("请输入有效的订阅地址")?;
    ensure!(
        value.len() <= 2048
            && matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "订阅仅支持不含账户密码的 HTTP 或 HTTPS 地址"
    );
    Ok(url)
}

pub async fn fetch(launcher: &Launcher, url: &str) -> Result<Value> {
    let url = feed_url(url)?;
    let client = network::client_builder(launcher.root(), &launcher.settings())?
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("订阅重定向过多");
            }
            if feed_url(attempt.url().as_str()).is_err() {
                return attempt.error("订阅重定向地址无效");
            }
            attempt.follow()
        }))
        .build()?;
    let mut response = client.get(url)
        .header(reqwest::header::ACCEPT, "application/atom+xml, application/rss+xml, application/xml, application/json, text/xml;q=0.9, */*;q=0.1")
        .send().await?.error_for_status()?;
    ensure!(
        response.content_length().unwrap_or(0) <= LIMIT as u64,
        "订阅内容过大"
    );
    let url = response.url().to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len() + chunk.len() <= LIMIT, "订阅内容过大");
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes).context("订阅源需要使用 UTF-8 编码")?;
    Ok(json!({ "text": text, "url": url }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_web_feeds_only_without_embedded_credentials() {
        for url in [
            "https://v1.hitokoto.cn/?encode=json",
            "https://example.org/feed.xml",
            "http://localhost:8080/rss",
        ] {
            assert!(feed_url(url).is_ok());
        }
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/xml,test",
            "https://user:secret@example.org/rss",
            "not a URL",
        ] {
            assert!(feed_url(url).is_err(), "{url}");
        }
    }
}
