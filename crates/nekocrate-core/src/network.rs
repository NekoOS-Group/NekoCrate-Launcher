use crate::model::Settings;
use crate::proxy_credentials::{ProxyVault, SystemVault};
use anyhow::{Result, bail};
use reqwest::{Client, ClientBuilder, Proxy, Url};
use std::path::Path;

/// Inspect typed TLS errors, including those wrapped inside an IO error by
/// hyper. Never infer clock problems from arbitrary server response text.
fn certificate_error<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a rustls::CertificateError> {
    if let Some(rustls::Error::InvalidCertificate(reason)) = error.downcast_ref() {
        return Some(reason);
    }
    if let Some(io) = error.downcast_ref::<std::io::Error>()
        && let Some(inner) = io.get_ref()
        && let Some(reason) = certificate_error(inner)
    {
        return Some(reason);
    }
    error.source().and_then(certificate_error)
}

pub fn is_certificate_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| certificate_error(cause).is_some())
}

pub fn explain_error(error: anyhow::Error) -> anyhow::Error {
    use rustls::CertificateError::*;
    let reason = error.chain().find_map(certificate_error);
    let hint = match reason {
        Some(Expired | ExpiredContext { .. } | NotValidYet | NotValidYetContext { .. }) => {
            "HTTPS 证书不在有效期内，请检查系统日期、时间和时区并开启自动同步；若时间正确，可能是服务器证书异常"
        }
        Some(_) => "HTTPS 证书验证失败，请检查代理或网络的证书配置",
        None => return error,
    };
    // Several layers may enrich the same error. Keep one actionable hint.
    if error.chain().any(|cause| cause.to_string() == hint) {
        error
    } else {
        error.context(hint)
    }
}

pub fn validate(settings: &Settings) -> Result<()> {
    if !["auto", "official", "bmclapi"].contains(&settings.download_source.as_str())
        || !["auto", "official", "modrinth", "curseforge", "bbsmc"]
            .contains(&settings.mod_download_source.as_str())
    {
        bail!("未知下载源");
    }
    if !["system", "none", "http", "socks", "custom"].contains(&settings.proxy_mode.as_str()) {
        bail!("未知代理模式");
    }
    if settings.proxy_username.len() > 255
        || settings
            .proxy_password
            .as_ref()
            .is_some_and(|p| p.0.len() > 255)
    {
        bail!("代理账号和密码不能超过 255 字节");
    }
    if matches!(settings.proxy_mode.as_str(), "http" | "socks" | "custom")
        || !settings.proxy_url.is_empty()
    {
        let valid = Url::parse(&settings.proxy_url).is_ok_and(|url| {
            ["http", "https", "socks5", "socks5h"].contains(&url.scheme())
                && match settings.proxy_mode.as_str() {
                    "http" => ["http", "https"].contains(&url.scheme()),
                    "socks" => ["socks5", "socks5h"].contains(&url.scheme()),
                    _ => true,
                }
                && url.host_str().is_some()
                && url.port_or_known_default().is_some_and(|port| port > 0)
                && url.username().is_empty()
                && url.password().is_none()
                && ["", "/"].contains(&url.path())
                && url.query().is_none()
                && url.fragment().is_none()
        });
        if !valid {
            // Do not echo user input: it may contain a mistakenly pasted password.
            bail!(
                "代理地址须为 HTTP、HTTPS 或 SOCKS5 地址，包含有效端口且不含账号密码、路径或参数"
            );
        }
    }
    Ok(())
}

/// All launcher traffic uses the same policy, including authentication and skins.
pub fn client_builder(root: &Path, settings: &Settings) -> Result<ClientBuilder> {
    validate(settings)?;
    let builder = Client::builder().user_agent(concat!(
        "NekoCrate/",
        env!("CARGO_PKG_VERSION"),
        " (https://github.com/NekoOS-Group/NekoCrate-Launcher)"
    ));
    Ok(match settings.proxy_mode.as_str() {
        "none" => builder.no_proxy(),
        "http" | "socks" | "custom" => {
            let mut proxy = Proxy::all(&settings.proxy_url)?;
            // Empty fields disable authentication. Credentials are never part of
            // the configured URL, origin headers, snapshots, or application logs.
            if !settings.proxy_username.is_empty() {
                let password = match &settings.proxy_password {
                    Some(value) => value.0.clone(),
                    None => settings
                        .proxy_password_key
                        .as_deref()
                        .map_or_else(|| Ok(String::new()), |key| SystemVault(root).read(key))?,
                };
                if !password.is_empty() {
                    proxy = proxy.basic_auth(&settings.proxy_username, &password);
                }
            }
            builder.no_proxy().proxy(proxy)
        }
        _ => builder, // reqwest system-proxy: OS / environment configuration.
    })
}

/// Only rewrite documented public game hosts. Login URLs and credentials never
/// go through a download mirror. Mod URLs use the independent source setting.
pub fn urls(settings: &Settings, input: &str) -> Vec<String> {
    let original = || vec![input.to_owned()];
    let Ok(url) = Url::parse(input) else {
        return original();
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return original();
    }
    let prefix = match url.host_str().unwrap_or_default() {
        "api.modrinth.com"
        | "cdn.modrinth.com"
        | "api.curseforge.com"
        | "edge.forgecdn.net"
        | "mediafilez.forgecdn.net" => {
            // Catalog selection cannot rewrite IDs or files across platforms.
            // Preserve the publisher's file URL regardless of the game mirror.
            return original();
        }
        "piston-meta.mojang.com"
        | "piston-data.mojang.com"
        | "launchermeta.mojang.com"
        | "launcher.mojang.com" => "",
        "resources.download.minecraft.net" => "/assets",
        "libraries.minecraft.net" => "/maven",
        _ => return original(),
    };
    let mirror = format!("https://bmclapi2.bangbang93.com{prefix}{}", url.path());
    match settings.download_source.as_str() {
        "auto" => vec![input.to_owned(), mirror],
        "bmclapi" => vec![mirror],
        _ => original(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_hint_uses_typed_certificate_errors_and_preserves_the_cause() {
        for reason in [
            rustls::CertificateError::Expired,
            rustls::CertificateError::NotValidYet,
        ] {
            let error = anyhow::Error::new(std::io::Error::other(
                rustls::Error::InvalidCertificate(reason),
            ))
            .context("request failed");
            assert!(is_certificate_error(&error));
            let error = explain_error(error);
            assert!(error.to_string().contains("系统日期"));
            assert!(format!("{error:#}").contains("request failed"));
            assert_eq!(
                format!("{:#}", explain_error(error))
                    .matches("系统日期")
                    .count(),
                1
            );
        }
        let unknown = anyhow::Error::new(rustls::Error::InvalidCertificate(
            rustls::CertificateError::UnknownIssuer,
        ));
        assert!(!explain_error(unknown).to_string().contains("系统日期"));
        let arbitrary = anyhow::anyhow!("server said: certificate expired");
        assert!(!is_certificate_error(&arbitrary));
        assert_eq!(
            explain_error(arbitrary).to_string(),
            "server said: certificate expired"
        );
    }

    #[test]
    fn mod_catalog_choices_persist_independently_and_never_rewrite_other_platforms() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = crate::Launcher::open(Some(dir.path().into())).unwrap();
        for source in ["auto", "modrinth", "curseforge", "bbsmc", "official"] {
            let mut settings = launcher.settings();
            settings.download_source = "bmclapi".into();
            settings.mod_download_source = source.into();
            launcher.save_settings(settings).unwrap();
            let saved = launcher.settings();
            assert_eq!(
                saved.mod_download_source,
                if source == "official" { "auto" } else { source }
            );
            assert_eq!(saved.download_source, "bmclapi");
            for url in [
                "https://cdn.modrinth.com/data/a/b/mod.jar",
                "https://edge.forgecdn.net/files/1/2/mod.jar",
                "https://bbsmc.net/mod/example",
            ] {
                assert_eq!(urls(&saved, url), [url]);
            }
        }
    }

    #[tokio::test]
    async fn http_proxy_auth_is_only_sent_with_both_fields_and_never_as_origin_auth() {
        use crate::model::ProxyPassword;
        for (username, password, authenticated) in [
            ("user", "pass", true),
            ("", "pass", false),
            ("user", "", false),
            ("", "", false),
        ] {
            let (address, proxy) = crate::test_http::serve(vec![(200, "ok".into())]).await;
            let settings = Settings {
                proxy_mode: "http".into(),
                proxy_url: address,
                proxy_username: username.into(),
                proxy_password: Some(ProxyPassword(password.into())),
                ..Settings::default()
            };
            client_builder(Path::new("/unused-test-vault"), &settings)
                .unwrap()
                .build()
                .unwrap()
                .get("http://launcher.invalid/auth")
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
            let requests = proxy.await.unwrap();
            let headers = requests[0].to_ascii_lowercase();
            assert_eq!(
                headers.contains("\r\nproxy-authorization: basic dxnlcjpwyxnz\r\n"),
                authenticated
            );
            assert!(!headers.contains("\r\nauthorization:"));
        }
    }

    #[test]
    fn existing_preferences_and_source_boundaries_are_preserved() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"download_source":"official"}"#).unwrap();
        assert_eq!(settings.mod_download_source, "auto");
        assert_eq!(settings.proxy_mode, "system");
        let game = "https://libraries.minecraft.net/org/example.jar";
        assert_eq!(urls(&settings, game), [game]);
        settings.download_source = "bmclapi".into();
        assert_eq!(
            urls(&settings, game),
            ["https://bmclapi2.bangbang93.com/maven/org/example.jar"]
        );
        settings.download_source = "auto".into();
        assert_eq!(urls(&settings, game).len(), 2);
        for private in [
            "https://libraries.minecraft.net.attacker.test/data",
            "https://libraries.minecraft.net/path?token=secret",
            "https://user:password@libraries.minecraft.net/path",
            "https://login.microsoftonline.com/path",
            "https://cdn.modrinth.com/data/project/version/mod.jar",
        ] {
            assert_eq!(urls(&settings, private), [private]);
        }
    }

    #[test]
    fn validates_proxy_without_echoing_secrets() {
        let mut settings = Settings {
            proxy_mode: "custom".into(),
            ..Settings::default()
        };
        for url in [
            "",
            "file:///tmp/proxy",
            "http://localhost:0",
            "http://localhost/path",
            "http://name:secret@localhost:8080",
            "socks5://localhost",
        ] {
            settings.proxy_url = url.into();
            let error = validate(&settings).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
        for url in [
            "http://127.0.0.1:7890",
            "https://proxy.example",
            "socks5://[::1]:1080",
            "socks5h://localhost:1080",
        ] {
            settings.proxy_url = url.into();
            validate(&settings).unwrap();
        }
    }

    #[test]
    fn proxy_protocol_must_match_the_selected_type() {
        let mut settings = Settings {
            proxy_mode: "http".into(),
            proxy_url: "socks5h://127.0.0.1:1080".into(),
            ..Settings::default()
        };
        assert!(validate(&settings).is_err());
        settings.proxy_mode = "socks".into();
        validate(&settings).unwrap();
        settings.proxy_url = "https://localhost:7890".into();
        assert!(validate(&settings).is_err());
        settings.proxy_mode = "http".into();
        validate(&settings).unwrap();
        settings.proxy_mode = "none".into();
        validate(&settings).unwrap();
        let legacy: Settings = serde_json::from_str(
            r#"{"concurrency":12,"proxy_mode":"custom","proxy_url":"socks5://localhost:1080"}"#,
        )
        .unwrap();
        assert!(!legacy.adaptive_concurrency);
        assert_eq!(legacy.concurrency, 12);
        validate(&legacy).unwrap();
    }

    #[tokio::test]
    async fn socks_proxy_receives_the_target_hostname_and_serves_the_request() {
        for authenticated in [false, true] {
            exercise_socks_proxy(authenticated).await;
        }
    }
    async fn exercise_socks_proxy(authenticated: bool) {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let settings = Settings {
            proxy_mode: "socks".into(),
            proxy_url: format!("socks5h://{}", listener.local_addr().unwrap()),
            proxy_username: if authenticated {
                "user".into()
            } else {
                String::new()
            },
            proxy_password: authenticated.then(|| crate::model::ProxyPassword("pass".into())),
            ..Settings::default()
        };
        let proxy = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut greeting = [0; 2];
            stream.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting[0], 5);
            let mut methods = vec![0; greeting[1] as usize];
            stream.read_exact(&mut methods).await.unwrap();
            let method = if authenticated { 2 } else { 0 };
            assert!(methods.contains(&method));
            stream.write_all(&[5, method]).await.unwrap();
            if authenticated {
                let mut credentials = [0; 11];
                stream.read_exact(&mut credentials).await.unwrap();
                assert_eq!(&credentials, b"\x01\x04user\x04pass");
                stream.write_all(&[1, 0]).await.unwrap();
            }
            let mut request = [0; 5];
            stream.read_exact(&mut request).await.unwrap();
            assert_eq!(&request[..4], &[5, 1, 0, 3]); // CONNECT, domain name.
            let mut domain = vec![0; request[4] as usize];
            stream.read_exact(&mut domain).await.unwrap();
            assert_eq!(domain, b"launcher.invalid");
            let mut port = [0; 2];
            stream.read_exact(&mut port).await.unwrap();
            assert_eq!(u16::from_be_bytes(port), 80);
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                .await
                .unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                headers.push(stream.read_u8().await.unwrap());
            }
            assert!(headers.starts_with(b"GET /socks HTTP/1.1"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nsocks",
                )
                .await
                .unwrap();
        });
        let response = client_builder(Path::new("/unused-test-vault"), &settings)
            .unwrap()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap()
            .get("http://launcher.invalid/socks")
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "socks");
        proxy.await.unwrap();
    }

    #[tokio::test]
    async fn requests_reach_custom_proxy_and_direct_mode_bypasses_it() {
        let (address, proxy) = crate::test_http::serve(vec![(200, "proxied".into())]).await;
        let mut settings = Settings {
            proxy_mode: "http".into(),
            proxy_url: address,
            ..Settings::default()
        };
        let response = client_builder(Path::new("/unused-test-vault"), &settings)
            .unwrap()
            .build()
            .unwrap()
            .get("http://launcher.invalid/resource")
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "proxied");
        assert!(
            proxy.await.unwrap()[0].starts_with("GET http://launcher.invalid/resource HTTP/1.1")
        );
        let (address, direct) = crate::test_http::serve(vec![(200, "direct".into())]).await;
        settings.proxy_mode = "none".into();
        let response = client_builder(Path::new("/unused-test-vault"), &settings)
            .unwrap()
            .build()
            .unwrap()
            .get(address)
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "direct");
        direct.await.unwrap();
    }
}
