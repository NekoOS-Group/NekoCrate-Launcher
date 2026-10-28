use crate::{model::Account, storage::now};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

mod profile_name;
mod session_cache;
pub(crate) use session_cache::SessionCache;

const OAUTH: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";
const DEFAULT_CLIENT_ID: &str = "31dbb1aa-d741-4983-bae9-c1bc13dc71b2";

#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceChallenge {
    #[serde(skip_serializing)]
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(skip)]
    pub started_at: u64,
    #[serde(skip)]
    pub client_id: String,
}
fn default_interval() -> u64 {
    5
}
#[derive(Deserialize)]
struct OAuthToken {
    access_token: String,
    refresh_token: Option<String>,
}
#[derive(Clone)]
pub struct MinecraftSession {
    pub account: Account,
    pub access_token: String,
    pub xuid: String,
    pub refresh_token: String,
    // In-memory only: changing the computer clock must not extend a token's life.
    pub(crate) valid_until: Instant,
}

pub fn explain_error(code: &str, xerr: Option<u64>) -> &'static str {
    match xerr {
        Some(2148916233) => {
            return "此微软账户尚未创建 Xbox 档案。请先登录 xbox.com 完成设置，再重试。";
        }
        Some(2148916235) => return "该地区的 Xbox Live 不可用。请检查账户地区设置。",
        Some(2148916236 | 2148916237) => {
            return "Xbox 要求完成年龄或身份验证。请在官方账户页面完成验证。";
        }
        Some(2148916238) => return "此账户需要监护人在 Microsoft 家庭设置中允许 Xbox 游戏服务。",
        _ => {}
    }
    match code {
        "authorization_declined" | "access_denied" => {
            "登录授权已被拒绝，请重新开始并在微软页面确认授权。"
        }
        "expired_token" => "登录验证码已过期，请重新获取验证码。",
        "invalid_grant" => "登录凭据已过期或被撤销，请移除账户并重新登录。",
        "unauthorized_client" | "invalid_client" => {
            "Client ID 无效或应用未启用公共客户端流程，请检查微软应用注册配置。"
        }
        "invalid_scope" => "微软应用未接受 XboxLive.signin 权限，请检查应用配置。",
        _ => "身份服务未能完成请求，请检查网络、应用权限和 Xbox 账户状态后重试。",
    }
}
async fn response_value(response: reqwest::Response, stage: &str) -> Result<Value> {
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("身份服务返回了无法解析的响应")?;
    if !status.is_success() {
        let code = body["error"].as_str().unwrap_or("unknown");
        let xerr = body["XErr"].as_u64();
        if stage == "Minecraft 登录" && status == 403 {
            bail!(
                "Minecraft 拒绝此应用登录。请确认 Client ID 已获 Minecraft API 访问许可，或稍后重试。（HTTP 403）"
            );
        }
        if stage == "Minecraft 档案" && status == 404 {
            bail!(
                "未找到 Java 版游戏档案，请确认账户拥有 Java 版并在 minecraft.net 创建玩家名称。"
            );
        }
        bail!(
            "{stage}失败（HTTP {status}）：{}",
            explain_error(code, xerr)
        );
    }
    Ok(body)
}
pub async fn begin(client: &reqwest::Client, client_id: &str) -> Result<DeviceChallenge> {
    begin_at(client, client_id, OAUTH).await
}
async fn begin_at(
    client: &reqwest::Client,
    client_id: &str,
    oauth_endpoint: &str,
) -> Result<DeviceChallenge> {
    // Empty overrides in both new and existing settings use the project app.
    let client_id = match client_id.trim() {
        "" => DEFAULT_CLIENT_ID,
        configured => configured,
    };
    uuid::Uuid::parse_str(client_id).context("微软登录应用 ID 配置无效，请联系启动器维护者。")?;
    let response = client
        .post(format!("{oauth_endpoint}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .context("无法连接微软身份服务")?;
    let mut challenge: DeviceChallenge =
        serde_json::from_value(response_value(response, "获取验证码").await?)?;
    if !challenge.verification_uri.starts_with("https://") {
        bail!("微软返回了无效登录地址");
    }
    challenge.started_at = now();
    challenge.client_id = client_id.into();
    Ok(challenge)
}
pub async fn finish(
    client: &reqwest::Client,
    challenge: DeviceChallenge,
    cancel: &CancellationToken,
) -> Result<MinecraftSession> {
    let mut interval = challenge.interval.max(1);
    loop {
        if now() > challenge.started_at + challenge.expires_in {
            bail!("{}", explain_error("expired_token", None));
        }
        tokio::select! { _=cancel.cancelled()=>bail!("登录已取消"), _=tokio::time::sleep(Duration::from_secs(interval))=>{} }
        let response = tokio::select! {
            _=cancel.cancelled()=>bail!("登录已取消"),
            r=client.post(format!("{OAUTH}/token")).form(&[("grant_type","urn:ietf:params:oauth:grant-type:device_code"),("client_id",&challenge.client_id),("device_code",&challenge.device_code)]).send()=>r.context("微软登录轮询失败")?,
        };
        let status = response.status();
        let value: Value = response.json().await?;
        if !status.is_success() {
            match value["error"].as_str().unwrap_or("") {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += 5;
                    continue;
                }
                code => bail!("{}", explain_error(code, None)),
            }
        }
        let oauth: OAuthToken = serde_json::from_value(value)?;
        return tokio::select! { _=cancel.cancelled()=>bail!("登录已取消"), r=exchange(client,oauth,&challenge.client_id)=>r };
    }
}
pub async fn refresh<F>(
    client: &reqwest::Client,
    account: &Account,
    refresh_token: &str,
    persist: impl Fn(String) -> F,
) -> Result<MinecraftSession>
where
    F: std::future::Future<Output = Result<()>>,
{
    let token = refresh_oauth(client, account, refresh_token, OAUTH, persist).await?;
    exchange(client, token, &account.client_id).await
}

async fn refresh_oauth<F>(
    client: &reqwest::Client,
    account: &Account,
    refresh_token: &str,
    endpoint: &str,
    persist: impl Fn(String) -> F,
) -> Result<OAuthToken>
where
    F: std::future::Future<Output = Result<()>>,
{
    let response = client
        .post(format!("{endpoint}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", account.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .context("无法刷新微软登录，请检查网络")?;
    let mut token: OAuthToken =
        serde_json::from_value(response_value(response, "刷新登录").await?)?;
    let rotated = token
        .refresh_token
        .get_or_insert_with(|| refresh_token.to_owned());
    if rotated != refresh_token {
        // Commit rotation before the Xbox/Minecraft exchange, which may fail.
        persist(rotated.clone()).await?;
    }
    Ok(token)
}
async fn exchange(
    client: &reqwest::Client,
    oauth: OAuthToken,
    client_id: &str,
) -> Result<MinecraftSession> {
    exchange_at(
        client,
        oauth,
        client_id,
        [
            "https://user.auth.xboxlive.com/user/authenticate",
            "https://xsts.auth.xboxlive.com/xsts/authorize",
            "https://api.minecraftservices.com/authentication/login_with_xbox",
            "https://api.minecraftservices.com/entitlements/mcstore",
            "https://api.minecraftservices.com/minecraft/profile",
        ],
    )
    .await
}

async fn exchange_at(
    client: &reqwest::Client,
    oauth: OAuthToken,
    client_id: &str,
    endpoints: [&str; 5],
) -> Result<MinecraftSession> {
    let xbl=response_value(client.post(endpoints[0]).json(&json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={}",oauth.access_token)},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})).send().await?,"Xbox 登录").await?;
    let xbox_token = xbl["Token"].as_str().context("Xbox 未返回登录令牌")?;
    let xsts=response_value(client.post(endpoints[1]).json(&json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[xbox_token]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})).send().await?,"Xbox 游戏授权").await?;
    let token = xsts["Token"].as_str().context("Xbox 未返回游戏令牌")?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .context("Xbox 未返回用户标识")?;
    let xuid = xsts["DisplayClaims"]["xui"][0]["xid"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let minecraft=response_value(client.post(endpoints[2]).json(&json!({"identityToken":format!("XBL3.0 x={uhs};{token}"),"ensureLegacyEnabled":true})).send().await?,"Minecraft 登录").await?;
    let access = minecraft["access_token"]
        .as_str()
        .context("Minecraft 未返回访问令牌")?
        .to_owned();
    let lifetime = minecraft["expires_in"].as_u64().unwrap_or(3600);
    let valid_until = Instant::now()
        .checked_add(Duration::from_secs(lifetime.saturating_sub(60)))
        .context("Minecraft 返回的令牌有效期无效")?;
    let entitlements = response_value(
        client.get(endpoints[3]).bearer_auth(&access).send().await?,
        "游戏权益检查",
    )
    .await?;
    if entitlements["items"]
        .as_array()
        .is_none_or(|items| items.is_empty())
    {
        bail!("此微软账户没有可用的 Minecraft Java 版权益，请检查购买账户或 Game Pass 订阅状态。");
    }
    let profile = response_value(
        client.get(endpoints[4]).bearer_auth(&access).send().await?,
        "Minecraft 档案",
    )
    .await?;
    let id = profile["id"].as_str().context("档案缺少 UUID")?.to_owned();
    uuid::Uuid::parse_str(&id).context("Minecraft 档案 UUID 无效")?;
    Ok(MinecraftSession {
        account: Account {
            id,
            username: profile["name"].as_str().context("档案缺少玩家名称")?.into(),
            expires_at: now().saturating_add(lifetime),
            client_id: client_id.into(),
            owned: true,
            skin: crate::skin::profile_skin(&profile),
        },
        access_token: access,
        xuid,
        refresh_token: oauth
            .refresh_token
            .context("微软未返回刷新令牌，请确认 offline_access 权限")?,
        valid_until,
    })
}

fn secret_account(root: &std::path::Path, id: &str) -> String {
    use ring::digest::{SHA256, digest};
    let namespace = hex::encode(digest(&SHA256, root.to_string_lossy().as_bytes()));
    format!("{}-{id}", &namespace[..16])
}
pub fn secret_entry(root: &std::path::Path, id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new("NekoCrate Launcher", &secret_account(root, id))
        .context("系统凭据存储不可用")
}
pub fn save_secret(root: &std::path::Path, id: &str, token: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let result = save_macos_secret(&secret_account(root, id), token);
    #[cfg(not(target_os = "macos"))]
    let result = secret_entry(root, id)?
        .set_password(token)
        .map_err(anyhow::Error::from);
    result.context("无法写入系统钥匙串；登录信息未保存。请解锁钥匙串或启用 Secret Service")
}

#[cfg(target_os = "macos")]
fn save_macos_secret(account: &str, token: &str) -> Result<()> {
    use security_framework::{
        item::{ItemClass, ItemSearchOptions, Reference, SearchResult},
        os::macos::keychain::{SecKeychain, SecPreferencesDomain},
    };
    let keychain = SecKeychain::default_for_domain(SecPreferencesDomain::User)?;
    // keyring's setter reads the old password before replacing it. Request only
    // its item reference, preserving the exact keychain/service/account and ACL.
    let result = ItemSearchOptions::new()
        .keychains(std::slice::from_ref(&keychain))
        .class(ItemClass::generic_password())
        .service("NekoCrate Launcher")
        .account(account)
        .load_refs(true)
        .limit(1)
        .search();
    match result {
        Ok(items) => match items.into_iter().next() {
            Some(SearchResult::Ref(Reference::KeychainItem(mut item))) => {
                item.set_password(token.as_bytes())?;
            }
            _ => bail!("系统钥匙串返回了无效凭据引用"),
        },
        Err(error) if error.code() == -25300 => {
            // errSecItemNotFound; authorization errors must never create a copy.
            keychain.add_generic_password("NekoCrate Launcher", account, token.as_bytes())?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
pub fn load_secret(root: &std::path::Path, id: &str) -> Result<String> {
    secret_entry(root, id)?
        .get_password()
        .context("系统钥匙串中没有可用凭据，请重新登录")
}
pub fn remove_secret(root: &std::path::Path, id: &str) -> Result<()> {
    match secret_entry(root, id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("无法移除系统钥匙串凭据"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn refresh_only_persists_changed_tokens_and_propagates_storage_failure() {
        use std::sync::Mutex;
        let account = Account {
            id: "account-fixture".into(),
            username: "Fixture".into(),
            client_id: "client-fixture".into(),
            expires_at: 0,
            owned: true,
            skin: None,
        };
        for returned in [None, Some("old-fixture"), Some("rotated-fixture")] {
            let (url, server) = crate::test_http::serve(vec![(
                200,
                json!({"access_token":"ms-fixture", "refresh_token":returned}).to_string(),
            )])
            .await;
            let saved = Mutex::new(Vec::new());
            let token = refresh_oauth(
                &crate::test_http::client(),
                &account,
                "old-fixture",
                &url,
                |token| {
                    saved.lock().unwrap().push(token);
                    std::future::ready(Ok(()))
                },
            )
            .await
            .unwrap();
            assert_eq!(
                token.refresh_token.as_deref(),
                Some(returned.unwrap_or("old-fixture"))
            );
            assert_eq!(
                saved.lock().unwrap().len(),
                usize::from(returned == Some("rotated-fixture"))
            );
            let requests = server.await.unwrap();
            assert!(requests[0].contains("refresh_token=old-fixture"));
        }
        let (url, server) = crate::test_http::serve(vec![(
            200,
            json!({"access_token":"ms-fixture", "refresh_token":"rotated-fixture"}).to_string(),
        )])
        .await;
        let result = refresh_oauth(
            &crate::test_http::client(),
            &account,
            "old-fixture",
            &url,
            |_| std::future::ready(Err(anyhow::anyhow!("storage fixture failure"))),
        )
        .await;
        assert_eq!(result.err().unwrap().to_string(), "storage fixture failure");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn device_login_defaults_to_project_app_and_preserves_explicit_overrides() {
        for configured in ["", " \t", " 2bc9d1a1-9cbc-49a2-992c-6ab525827e22 "] {
            let expected = if configured.trim().is_empty() {
                DEFAULT_CLIENT_ID
            } else {
                configured.trim()
            };
            let (url, server) = crate::test_http::serve(vec![(
                200,
                json!({
                    "device_code": "device-fixture",
                    "user_code": "TEST-CODE",
                    "verification_uri": "https://www.microsoft.com/link",
                    "expires_in": 900,
                    "interval": 5,
                })
                .to_string(),
            )])
            .await;
            let challenge = begin_at(&crate::test_http::client(), configured, &url)
                .await
                .unwrap();
            let requests = server.await.unwrap();
            assert!(requests[0].contains(&format!("client_id={expected}")));
            assert!(requests[0].contains("scope=XboxLive.signin+offline_access"));
            assert_eq!(challenge.client_id, expected);
            assert_eq!(challenge.user_code, "TEST-CODE");
        }
    }
    #[tokio::test]
    async fn exchange_checks_entitlement_and_binds_profile() {
        for owned in [true, false] {
            let mut replies = vec![
                (200, json!({"Token":"xbl-fixture"}).to_string()),
                (200, json!({"Token":"xsts-fixture","DisplayClaims":{"xui":[{"uhs":"hash-fixture","xid":"1234"}]}}).to_string()),
                (200, json!({"access_token":"mc-fixture","expires_in":3600}).to_string()),
                (200, if owned {json!({"items":[{"name":"game_minecraft"}]})} else {json!({"items":[]})}.to_string()),
            ];
            if owned {
                replies.push((
                    200,
                    json!({"id":"00112233445566778899aabbccddeeff","name":"NekoTest",
                        "skins":[{"state":"ACTIVE","variant":"SLIM","url":"https://textures.minecraft.net/texture/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}).to_string(),
                ));
            }
            let (url, server) = crate::test_http::serve(replies).await;
            let result = exchange_at(
                &crate::test_http::client(),
                OAuthToken {
                    access_token: "ms-fixture".into(),
                    refresh_token: Some("refresh-fixture".into()),
                },
                "client-fixture",
                [&url; 5],
            )
            .await;
            let requests = server.await.unwrap();
            assert!(requests[0].contains("d=ms-fixture"));
            assert!(requests[1].contains("xbl-fixture"));
            assert!(requests[2].contains("XBL3.0 x=hash-fixture;xsts-fixture"));
            assert!(requests[3].contains("Bearer mc-fixture"));
            if owned {
                let session = result.unwrap();
                assert_eq!(session.account.username, "NekoTest");
                assert_eq!(session.xuid, "1234");
                assert!(session.account.owned);
                assert!(session.account.skin.as_ref().unwrap().slim);
                assert_eq!(requests.len(), 5);
            } else {
                assert!(result.err().unwrap().to_string().contains("没有可用"));
                assert_eq!(requests.len(), 4);
            }
        }
    }
    #[test]
    fn device_code_is_not_exposed_to_frontend() {
        let c = DeviceChallenge {
            device_code: "SECRET".into(),
            user_code: "ABCD".into(),
            verification_uri: "https://microsoft.com/link".into(),
            expires_in: 900,
            interval: 5,
            started_at: 0,
            client_id: "PRIVATE".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("SECRET"));
        assert!(!json.contains("PRIVATE"));
        assert!(json.contains("ABCD"));
    }
    #[test]
    fn explains_xbox_child_and_oauth_errors() {
        assert!(explain_error("", Some(2148916238)).contains("监护人"));
        assert!(explain_error("invalid_grant", None).contains("重新登录"));
    }
}
