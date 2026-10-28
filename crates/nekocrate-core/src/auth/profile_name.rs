use crate::{Launcher, model::Account};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use std::time::Duration;

const PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        (3..=16).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "Java 版玩家名称须为 3–16 位英文字母、数字或下划线"
    );
    Ok(())
}

// None means a definitive 401, the only response that permits a refresh/retry.
// In particular, a timed-out PUT may have succeeded and must not be replayed.
async fn request(client: &Client, method: Method, url: &str, token: &str) -> Result<Option<Value>> {
    let changing = method == Method::PUT;
    let mut response = client
        .request(method, url)
        .bearer_auth(token)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context(if changing {
            "改名请求未完成，请刷新账户确认结果后再试"
        } else {
            "无法检查玩家名称，请检查网络后重试"
        })?;
    match response.status() {
        StatusCode::UNAUTHORIZED => return Ok(None),
        StatusCode::BAD_REQUEST => {
            bail!("Minecraft 未接受此名称，请检查名称是否可用及账户改名限制")
        }
        StatusCode::FORBIDDEN => bail!("此账户暂时无法改名，请检查官方改名冷却期或账户权限"),
        StatusCode::CONFLICT => bail!("此玩家名称已被使用，请换一个名称"),
        StatusCode::TOO_MANY_REQUESTS => bail!("改名请求过于频繁，请稍后重试"),
        status if !status.is_success() => bail!("玩家改名服务请求失败（HTTP {}）", status.as_u16()),
        _ => {}
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= 64 * 1024,
            "玩家改名服务响应过大"
        );
        body.extend_from_slice(&chunk);
    }
    // Service bodies can contain arbitrary text. Parse known fields only;
    // never include the raw body or credentials in UI errors/diagnostics.
    Ok(Some(
        serde_json::from_slice(&body).context("玩家改名服务返回了无法解析的响应")?,
    ))
}

async fn rename_at(
    client: &Client,
    endpoint: &str,
    account: &Account,
    token: &str,
    name: &str,
) -> Result<Option<String>> {
    let Some(info) = request(
        client,
        Method::GET,
        &format!("{endpoint}/namechange"),
        token,
    )
    .await?
    else {
        return Ok(None);
    };
    match info["nameChangeAllowed"].as_bool() {
        Some(true) => {}
        Some(false) => bail!("此账户尚未到可改名时间，Java 版每 30 天可改名一次"),
        None => bail!("玩家改名服务未返回改名资格"),
    }
    // Availability can report one's own name as DUPLICATE for a case-only edit.
    // The authenticated PUT still enforces the official cooldown and rules.
    if !account.username.eq_ignore_ascii_case(name) {
        let Some(available) = request(
            client,
            Method::GET,
            &format!("{endpoint}/name/{name}/available"),
            token,
        )
        .await?
        else {
            return Ok(None);
        };
        match available["status"].as_str() {
            Some("AVAILABLE") => {}
            Some("DUPLICATE") => bail!("此玩家名称已被使用，请换一个名称"),
            Some("NOT_ALLOWED") => bail!("Minecraft 不允许使用此玩家名称，请换一个名称"),
            _ => bail!("无法确认此玩家名称是否可用，请稍后重试"),
        }
    }
    let Some(profile) = request(
        client,
        Method::PUT,
        &format!("{endpoint}/name/{name}"),
        token,
    )
    .await?
    else {
        return Ok(None);
    };
    let id = profile["id"]
        .as_str()
        .context("玩家改名服务未返回账户信息")?;
    ensure!(
        uuid::Uuid::parse_str(id)? == uuid::Uuid::parse_str(&account.id)?,
        "玩家改名服务返回的账户不匹配"
    );
    let changed = profile["name"]
        .as_str()
        .context("玩家改名服务未返回新名称")?;
    ensure!(
        changed.eq_ignore_ascii_case(name),
        "玩家改名服务返回的名称不匹配，请刷新账户确认结果"
    );
    validate_name(changed)?;
    Ok(Some(changed.into()))
}

impl Launcher {
    pub async fn rename_account(&self, id: &str, name: &str) -> Result<Account> {
        let client = crate::network::client_builder(self.root(), &self.settings())?
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        self.rename_account_at(&client, PROFILE, id, name).await
    }

    async fn rename_account_at(
        &self,
        client: &Client,
        endpoint: &str,
        id: &str,
        name: &str,
    ) -> Result<Account> {
        let name = name.trim();
        validate_name(name)?;
        let _guard = self.0.account_operation.lock().await;
        let account = self
            .snapshot()
            .database
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .context("账户不存在")?;
        if account.username == name {
            return Ok(account);
        }
        for force_refresh in [false, true] {
            let mut session = self.session_for_account(id, force_refresh).await?;
            if session.account.username == name {
                return Ok(session.account);
            }
            if let Some(changed) = rename_at(
                client,
                endpoint,
                &session.account,
                &session.access_token,
                name,
            )
            .await?
            {
                session.account.username = changed;
                let updated = session.account.clone();
                self.save_account(updated.clone(), false)?;
                self.0.sessions.remember(session);
                return Ok(updated);
            }
        }
        self.0.sessions.invalidate_session(id);
        bail!("登录凭据已失效，请重新登录后修改玩家名称")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::MinecraftSession, model::AccountSkin, test_http};
    use serde_json::json;

    const PLAYER: &str = "22222222222242228222222222222222";
    const OTHER: &str = "11111111111141118111111111111111";
    fn remember(launcher: &Launcher, id: &str) -> Account {
        let account = Account {
            id: id.into(),
            username: "Original".into(),
            client_id: "fixture".into(),
            expires_at: crate::storage::now() + 3600,
            owned: true,
            skin: Some(AccountSkin {
                url: "https://textures.minecraft.net/texture/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
                slim: true,
            }),
        };
        launcher.save_account(account.clone(), true).unwrap();
        launcher.0.sessions.remember(MinecraftSession {
            account: account.clone(),
            access_token: format!("access-{id}"),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: std::time::Instant::now() + Duration::from_secs(300),
        });
        account
    }
    fn allowed() -> (u16, String) {
        (200, json!({"nameChangeAllowed":true}).to_string())
    }
    fn available(status: &str) -> (u16, String) {
        (200, json!({"status":status}).to_string())
    }
    fn renamed(id: &str, name: &str) -> (u16, String) {
        (200, json!({"id":id,"name":name}).to_string())
    }

    #[tokio::test]
    async fn rename_is_account_bound_persists_and_updates_cached_launch_name_without_keychain_or_download_lock()
     {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = remember(&launcher, PLAYER);
        remember(&launcher, OTHER);
        let downloading = launcher.0.operation.lock().await;
        let (url, server) = test_http::serve(vec![
            allowed(),
            available("AVAILABLE"),
            renamed(PLAYER, "New_Name"),
        ])
        .await;
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            launcher.rename_account_at(&test_http::client(), &url, PLAYER, " New_Name "),
        )
        .await
        .unwrap()
        .unwrap();
        drop(downloading);
        assert_eq!(result.id, PLAYER);
        assert_eq!(result.username, "New_Name");
        assert_eq!(result.skin.unwrap().url, account.skin.as_ref().unwrap().url);
        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("GET /namechange "));
        assert!(requests[1].starts_with("GET /name/New_Name/available "));
        assert!(requests[2].starts_with("PUT /name/New_Name "));
        assert!(
            requests
                .iter()
                .all(|r| r.contains(&format!("Bearer access-{PLAYER}"))
                    && !r.contains(&format!("access-{OTHER}")))
        );
        let db = launcher.snapshot().database;
        assert_eq!(db.selected_account.as_deref(), Some(OTHER));
        assert_eq!(db.guest_name, "Player");
        assert_eq!(
            db.accounts.iter().find(|a| a.id == OTHER).unwrap().username,
            "Original"
        );
        let cached = launcher.0.sessions.session(&account).unwrap();
        assert_eq!(cached.account.username, "New_Name");
        assert_eq!(cached.access_token, format!("access-{PLAYER}"));
        launcher.select_account(PLAYER).unwrap();
        let sink: crate::model::EventSink = std::sync::Arc::new(|_| {});
        let launched = launcher.launch_session(&sink).await.unwrap();
        assert_eq!(launched.username, "New_Name");
        assert_eq!(launched.uuid, PLAYER);
        let saved = std::fs::read_to_string(temp.path().join("database.json")).unwrap();
        assert!(!saved.contains("access-") && !saved.contains("refresh-fixture"));
        drop(launcher);
        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        assert_eq!(
            reopened
                .snapshot()
                .database
                .accounts
                .iter()
                .find(|a| a.id == PLAYER)
                .unwrap()
                .username,
            "New_Name"
        );
    }

    #[tokio::test]
    async fn invalid_names_and_unchanged_names_never_access_network_or_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = remember(&launcher, PLAYER);
        launcher.0.sessions.remove(PLAYER); // No keychain item exists.
        for name in [
            "",
            "ab",
            "中文名字",
            "a b",
            "a/b",
            "A?b",
            "abcdefghijklmnopq",
        ] {
            assert!(
                launcher
                    .rename_account_at(&test_http::client(), "http://127.0.0.1:1", PLAYER, name)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("3–16")
            );
        }
        let unchanged = launcher
            .rename_account_at(
                &test_http::client(),
                "http://127.0.0.1:1",
                PLAYER,
                &account.username,
            )
            .await
            .unwrap();
        assert_eq!(unchanged.username, "Original");
        assert_eq!(
            launcher.snapshot().database.accounts[0].username,
            "Original"
        );
    }

    #[tokio::test]
    async fn cooldown_availability_and_service_failures_never_overwrite_the_existing_profile() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = remember(&launcher, PLAYER);
        for (replies, expected) in [
            (
                vec![(200, json!({"nameChangeAllowed":false}).to_string())],
                "30 天",
            ),
            (vec![allowed(), available("DUPLICATE")], "已被使用"),
            (vec![allowed(), available("NOT_ALLOWED")], "不允许"),
            (vec![allowed(), available("UNKNOWN")], "无法确认"),
            (vec![(200, "{}".into())], "资格"),
            (
                vec![
                    allowed(),
                    available("AVAILABLE"),
                    (403, "sensitive service text".into()),
                ],
                "冷却期",
            ),
            (
                vec![
                    allowed(),
                    available("AVAILABLE"),
                    (409, "sensitive service text".into()),
                ],
                "已被使用",
            ),
            (
                vec![
                    allowed(),
                    available("AVAILABLE"),
                    (429, "sensitive service text".into()),
                ],
                "频繁",
            ),
            (
                vec![
                    allowed(),
                    available("AVAILABLE"),
                    renamed(OTHER, "New_Name"),
                ],
                "账户不匹配",
            ),
            (
                vec![
                    allowed(),
                    available("AVAILABLE"),
                    renamed(PLAYER, "Unexpected"),
                ],
                "名称不匹配",
            ),
        ] {
            let count = replies.len();
            let (url, server) = test_http::serve(replies).await;
            let error = launcher
                .rename_account_at(&test_http::client(), &url, PLAYER, "New_Name")
                .await
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            assert!(!format!("{error:#}").contains("sensitive service text"));
            assert_eq!(server.await.unwrap().len(), count);
            assert_eq!(
                launcher.snapshot().database.accounts[0].username,
                "Original"
            );
            assert_eq!(
                launcher
                    .0
                    .sessions
                    .session(&account)
                    .unwrap()
                    .account
                    .username,
                "Original"
            );
        }
    }

    #[tokio::test]
    async fn case_only_changes_skip_own_name_availability_but_obey_cooldown() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        remember(&launcher, PLAYER);
        let (url, server) = test_http::serve(vec![allowed(), renamed(PLAYER, "original")]).await;
        assert_eq!(
            launcher
                .rename_account_at(&test_http::client(), &url, PLAYER, "original")
                .await
                .unwrap()
                .username,
            "original"
        );
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("PUT /name/original "));
    }

    #[tokio::test]
    async fn only_explicit_unauthorized_replies_request_token_refresh_and_redirects_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = remember(&launcher, PLAYER);
        for mut prefix in [
            vec![],
            vec![allowed()],
            vec![allowed(), available("AVAILABLE")],
        ] {
            prefix.push((401, "invalid credentials fixture".into()));
            let count = prefix.len();
            let (url, server) = test_http::serve(prefix).await;
            assert!(
                rename_at(
                    &test_http::client(),
                    &url,
                    &account,
                    "token-fixture",
                    "New_Name"
                )
                .await
                .unwrap()
                .is_none()
            );
            assert_eq!(server.await.unwrap().len(), count);
        }
        let (url, server) = test_http::serve_with_headers(vec![(
            307,
            "Location: http://127.0.0.1:1/never\r\n".into(),
            String::new(),
        )])
        .await;
        let client = crate::network::client_builder(
            temp.path(),
            &crate::model::Settings {
                proxy_mode: "none".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
        assert!(
            rename_at(&client, &url, &account, "token-fixture", "New_Name")
                .await
                .unwrap_err()
                .to_string()
                .contains("307")
        );
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
