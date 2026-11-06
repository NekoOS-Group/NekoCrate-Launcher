//! System-browser authorization code flow for a public desktop client.
//! No credentials or callback query strings are logged or returned to the UI.
use super::{MinecraftSession, OAUTH, OAuthToken, SCOPE};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

const LIFETIME: Duration = Duration::from_secs(900);
const REQUEST_LIMIT: usize = 16 * 1024;

#[derive(Serialize)]
pub struct BrowserChallenge {
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(skip)]
    listener: TcpListener,
    #[serde(skip)]
    redirect_uri: String,
    #[serde(skip)]
    client_id: String,
    #[serde(skip)]
    verifier: String,
    #[serde(skip)]
    state: String,
    #[serde(skip)]
    deadline: Instant,
}

fn random_secret() -> Result<String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("无法生成微软登录验证信息"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ))
}

impl BrowserChallenge {
    pub async fn begin(id: &str) -> Result<Self> {
        let client_id = super::client_id(id)?.to_owned();
        // Never bind on all interfaces. localhost's dynamic port is accepted
        // against a registered http://localhost native-app redirect URI.
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .context("无法创建本机登录回调，请重试")?;
        let redirect_uri = format!("http://localhost:{}/", listener.local_addr()?.port());
        let verifier = random_secret()?;
        let state = random_secret()?;
        let mut url = reqwest::Url::parse(&format!("{OAUTH}/authorize"))?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_mode", "query"),
            ("scope", SCOPE),
            ("prompt", "select_account"),
            ("state", state.as_str()),
            ("code_challenge", challenge(&verifier).as_str()),
            ("code_challenge_method", "S256"),
        ]);
        Ok(Self {
            verification_uri: url.into(),
            expires_in: LIFETIME.as_secs(),
            listener,
            redirect_uri,
            client_id,
            verifier,
            state,
            deadline: Instant::now() + LIFETIME,
        })
    }

    pub(crate) async fn finish(
        self,
        client: &reqwest::Client,
        cancel: &CancellationToken,
    ) -> Result<MinecraftSession> {
        let id = self.client_id.clone();
        let oauth = self
            .redeem(client, cancel, &format!("{OAUTH}/token"))
            .await?;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("登录已取消"),
            result = super::exchange(client, oauth, &id) => result,
        }
    }

    async fn redeem(
        self,
        client: &reqwest::Client,
        cancel: &CancellationToken,
        token_endpoint: &str,
    ) -> Result<OAuthToken> {
        let code = tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("登录已取消"),
            _ = tokio::time::sleep_until(self.deadline.into()) => bail!("浏览器登录已超时，请重新登录"),
            result = self.wait_for_code() => result?,
        };
        // No more callbacks are accepted once the one-use code is received.
        drop(self.listener);
        let exchange = async {
            let response = client
                .post(token_endpoint)
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("client_id", self.client_id.as_str()),
                    ("redirect_uri", self.redirect_uri.as_str()),
                    ("code", code.as_str()),
                    ("code_verifier", self.verifier.as_str()),
                    ("scope", SCOPE),
                ])
                .send()
                .await
                .context("无法完成微软浏览器登录")?;
            serde_json::from_value(super::response_value(response, "微软登录").await?)
                .context("微软未返回有效的登录令牌")
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("登录已取消"),
            result = tokio::time::timeout(Duration::from_secs(30), exchange) =>
                result.context("微软登录请求超时，请重试")?,
        }
    }

    async fn wait_for_code(&self) -> Result<String> {
        let host = format!("localhost:{}", self.listener.local_addr()?.port());
        loop {
            let (mut stream, peer) = self
                .listener
                .accept()
                .await
                .context("无法接收本机登录回调")?;
            if !peer.ip().is_loopback() {
                continue;
            }
            // Ignore unrelated tabs, preconnects and malformed requests rather
            // than letting them consume or cancel the pending authorization.
            let result = tokio::time::timeout(Duration::from_secs(5), async {
                let bytes = read_headers(&mut stream).await?;
                let callback = parse_callback(&bytes, &host, &self.state);
                let status = if callback.is_some() { "200 OK" } else { "400 Bad Request" };
                // Never interpolate request parameters into the browser page.
                let body = if callback.is_some() {
                    "<!doctype html><meta charset=utf-8><title>NekoCrate Launcher</title><p>请返回猫咪盒子启动器查看登录结果。</p><p>Return to NekoCrate Launcher to continue.</p>"
                } else {
                    "<!doctype html><meta charset=utf-8><title>NekoCrate Launcher</title><p>无效或已过期的登录回调。请从启动器重新打开登录页。</p>"
                };
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}", body.len());
                // A closed browser tab must not discard a valid authorization.
                let _ = tokio::time::timeout(Duration::from_millis(250), stream.write_all(response.as_bytes())).await;
                Ok::<_, anyhow::Error>(callback)
            }).await;
            if let Ok(Ok(Some(callback))) = result {
                return match callback {
                    Callback::Code(code) => Ok(code),
                    Callback::Denied(error) => bail!("{}", super::explain_error(&error, None)),
                };
            }
        }
    }
}

async fn read_headers(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        ensure!(read > 0, "登录回调连接已关闭");
        bytes.extend_from_slice(&buffer[..read]);
        ensure!(bytes.len() <= REQUEST_LIMIT, "登录回调请求过大");
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}

#[derive(Debug, PartialEq)]
enum Callback {
    Code(String),
    Denied(String),
}

fn parse_callback(bytes: &[u8], host: &str, state: &str) -> Option<Callback> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    if !request.parse(bytes).ok()?.is_complete() || request.method != Some("GET") {
        return None;
    }
    let mut hosts = request
        .headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("host"));
    if hosts.next()?.value != host.as_bytes() || hosts.next().is_some() {
        return None;
    }
    let path = request.path?;
    if !path.starts_with("/?") || path.contains('#') {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://{host}{path}")).ok()?;
    let mut returned_state = None;
    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        let slot = match key.as_ref() {
            "state" => &mut returned_state,
            "code" => &mut code,
            "error" => &mut error,
            _ => continue,
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    if returned_state.as_deref() != Some(state) {
        return None;
    }
    match (code, error) {
        (Some(code), None) if !code.is_empty() && code.len() <= 8192 => Some(Callback::Code(code)),
        (None, Some(error)) if !error.is_empty() => Some(Callback::Denied(error)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn request(host: &str, query: &str) -> String {
        format!("GET /?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n")
    }
    async fn send(address: std::net::SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .unwrap()
            .unwrap();
        response
    }
    fn params(url: &str) -> BTreeMap<String, String> {
        reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[tokio::test]
    async fn native_login_uses_s256_account_selection_and_keeps_verifier_private() {
        // RFC 7636 Appendix B: independently specified PKCE vector.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let first = BrowserChallenge::begin("").await.unwrap();
        let second = BrowserChallenge::begin("").await.unwrap();
        assert_ne!(first.state, second.state);
        assert_ne!(first.verifier, second.verifier);
        assert!(first.listener.local_addr().unwrap().ip().is_loopback());
        let query = params(&first.verification_uri);
        assert_eq!(query["client_id"], super::super::DEFAULT_CLIENT_ID);
        assert_eq!(query["prompt"], "select_account");
        assert_eq!(query["scope"], SCOPE);
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["code_challenge"], challenge(&first.verifier));
        assert_eq!(query["redirect_uri"], first.redirect_uri);
        assert!(!query.contains_key("login_hint"));
        let verifier = first.verifier.clone();
        let value = serde_json::to_value(first).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert!(value.get("verification_uri").is_some());
        assert_eq!(value["expires_in"], 900);
        assert!(!value.to_string().contains(&verifier));
    }

    #[tokio::test]
    async fn login_defaults_to_project_app_and_preserves_explicit_overrides() {
        for configured in ["", " \t", " 2bc9d1a1-9cbc-49a2-992c-6ab525827e22 "] {
            let expected = if configured.trim().is_empty() {
                super::super::DEFAULT_CLIENT_ID
            } else {
                configured.trim()
            };
            let login = BrowserChallenge::begin(configured).await.unwrap();
            assert_eq!(login.client_id, expected);
            assert_eq!(params(&login.verification_uri)["client_id"], expected);
        }
        let error = BrowserChallenge::begin("invalid-client-id")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("应用 ID 配置无效"));
    }

    #[test]
    fn callback_requires_expected_host_path_state_and_one_unambiguous_result() {
        let parse = |query: &str| {
            parse_callback(
                request("localhost:1234", query).as_bytes(),
                "localhost:1234",
                "state-fixture",
            )
        };
        assert_eq!(
            parse("state=state-fixture&code=a%2Bb"),
            Some(Callback::Code("a+b".into()))
        );
        assert_eq!(
            parse("error=access_denied&state=state-fixture"),
            Some(Callback::Denied("access_denied".into()))
        );
        for query in [
            "code=stolen",
            "state=wrong&code=stolen",
            "state=state-fixture",
            "state=state-fixture&state=state-fixture&code=x",
            "state=state-fixture&code=x&code=y",
            "state=state-fixture&code=",
            "state=state-fixture&code=x&error=access_denied",
            "state=state-fixture&error=access_denied&error=other",
            "error=access_denied",
            "state=state-fixture&code=x#fragment",
        ] {
            assert_eq!(parse(query), None, "{query}");
        }
        let valid = request("localhost:1234", "state=state-fixture&code=x");
        for invalid in [
            valid.replace("GET", "POST"),
            valid.replace("/?", "/other?"),
            valid.replace("Host: localhost:1234", "Host: attacker.example"),
            valid.replace(
                "Host: localhost:1234",
                "Host: localhost:1234\r\nHost: localhost:1234",
            ),
            valid.replace("GET /?", "GET http://localhost:1234/?"),
        ] {
            assert_eq!(
                parse_callback(invalid.as_bytes(), "localhost:1234", "state-fixture"),
                None
            );
        }
    }

    #[tokio::test]
    async fn loopback_ignores_unrelated_requests_and_redeems_once_with_the_original_verifier() {
        let browser = BrowserChallenge::begin("").await.unwrap();
        let address = browser.listener.local_addr().unwrap();
        let host = format!("localhost:{}", address.port());
        let state = browser.state.clone();
        let verifier = browser.verifier.clone();
        let redirect = browser.redirect_uri.clone();
        let (endpoint, server) = crate::test_http::serve(vec![(
            200,
            r#"{"access_token":"fixture-access","refresh_token":"fixture-refresh"}"#.into(),
        )])
        .await;
        let waiting = tokio::spawn(async move {
            browser
                .redeem(
                    &crate::test_http::client(),
                    &CancellationToken::new(),
                    &endpoint,
                )
                .await
        });
        let invalid = send(address, &request(&host, "error=access_denied&state=wrong")).await;
        assert!(invalid.starts_with("HTTP/1.1 400"));
        assert!(!waiting.is_finished());
        let response = send(
            address,
            &request(&host, &format!("state={state}&code=secret-fixture")),
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("Cache-Control: no-store"));
        assert!(response.contains("Referrer-Policy: no-referrer"));
        assert!(!response.contains("secret-fixture"));
        assert!(!response.contains(&state));
        let token = waiting.await.unwrap().unwrap();
        assert_eq!(token.access_token, "fixture-access");
        assert_eq!(token.refresh_token.as_deref(), Some("fixture-refresh"));
        assert!(TcpStream::connect(address).await.is_err());
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = requests[0].split_once("\r\n\r\n").unwrap().1;
        let fields = params(&format!("http://fixture/?{body}"));
        assert_eq!(fields["grant_type"], "authorization_code");
        assert_eq!(fields["code_verifier"], verifier);
        assert_eq!(fields["redirect_uri"], redirect);
        assert_eq!(fields["code"], "secret-fixture");
        assert!(!fields.contains_key("client_secret"));
    }

    #[tokio::test]
    async fn cancellation_and_expiry_close_the_callback_socket_even_during_partial_http() {
        let browser = BrowserChallenge::begin("").await.unwrap();
        let address = browser.listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let waiting = tokio::spawn(async move {
            browser
                .redeem(&crate::test_http::client(), &token, "http://127.0.0.1:1")
                .await
        });
        let _stalled = TcpStream::connect(address).await.unwrap();
        tokio::task::yield_now().await;
        cancel.cancel();
        let error = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap()
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "登录已取消");
        assert!(TcpStream::connect(address).await.is_err());
        let mut browser = BrowserChallenge::begin("").await.unwrap();
        browser.deadline = Instant::now();
        let address = browser.listener.local_addr().unwrap();
        let error = browser
            .redeem(
                &crate::test_http::client(),
                &CancellationToken::new(),
                "http://127.0.0.1:1",
            )
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("超时"));
        assert!(TcpStream::connect(address).await.is_err());
    }

    #[tokio::test]
    async fn denied_authorization_does_not_exchange_tokens_or_echo_provider_text() {
        let browser = BrowserChallenge::begin("").await.unwrap();
        let address = browser.listener.local_addr().unwrap();
        let host = format!("localhost:{}", address.port());
        let query = format!(
            "state={}&error=access_denied&error_description=secret-fixture",
            browser.state
        );
        let waiting = tokio::spawn(async move {
            browser
                .redeem(
                    &crate::test_http::client(),
                    &CancellationToken::new(),
                    "http://127.0.0.1:1",
                )
                .await
        });
        let response = send(address, &request(&host, &query)).await;
        assert!(!response.contains("secret-fixture"));
        let error = waiting.await.unwrap().err().unwrap();
        assert!(error.to_string().contains("拒绝"));
        assert!(!error.to_string().contains("secret-fixture"));
    }
}
