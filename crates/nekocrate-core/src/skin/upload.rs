use super::{profile_skin, texture_url, validate_png};
use crate::{
    Launcher,
    model::{Account, AccountSkin},
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{
    Client, StatusCode,
    multipart::{Form, Part},
};
use std::time::Duration;

const ENDPOINT: &str = "https://api.minecraftservices.com/minecraft/profile/skins";

fn validate_upload(bytes: &[u8]) -> Result<()> {
    validate_png(bytes)?;
    // Higher-resolution textures can be previewed locally, but the official
    // Java profile service only accepts the original skin dimensions.
    ensure!(
        u32::from_be_bytes(bytes[16..20].try_into()?) == 64,
        "正版账户皮肤须为 64×64 或 64×32 的 PNG"
    );
    Ok(())
}

enum UploadResult {
    Applied(AccountSkin),
    Unauthorized,
}

async fn upload_at(
    client: &Client,
    endpoint: &str,
    account: &str,
    token: &str,
    bytes: &[u8],
    slim: bool,
) -> Result<UploadResult> {
    validate_upload(bytes)?;
    let form = Form::new()
        .text("variant", if slim { "slim" } else { "classic" })
        .part(
            "file",
            Part::bytes(bytes.to_vec())
                .file_name("skin.png")
                .mime_str("image/png")?,
        );
    let mut response = client
        .post(endpoint)
        .bearer_auth(token)
        .multipart(form)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("无法上传皮肤，请检查网络后重试")?;
    match response.status() {
        StatusCode::UNAUTHORIZED => return Ok(UploadResult::Unauthorized),
        StatusCode::BAD_REQUEST => bail!("Minecraft 未接受此皮肤，请检查 PNG 文件和手臂模型"),
        StatusCode::FORBIDDEN => bail!("Minecraft 拒绝更换皮肤，请检查账户权限或重新登录"),
        StatusCode::TOO_MANY_REQUESTS => bail!("皮肤切换过于频繁，请稍后重试"),
        status if !status.is_success() => {
            bail!("皮肤上传失败（HTTP {}），请稍后重试", status.as_u16())
        }
        _ => {}
    }
    // Do not echo the service body or send credentials to a texture host.
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(body.len() + chunk.len() <= 64 * 1024, "皮肤服务响应过大");
        body.extend_from_slice(&chunk);
    }
    let profile: serde_json::Value =
        serde_json::from_slice(&body).context("皮肤服务返回了无法解析的响应")?;
    let id = profile["id"].as_str().context("皮肤服务未返回账户信息")?;
    ensure!(
        uuid::Uuid::parse_str(id)? == uuid::Uuid::parse_str(account)?,
        "皮肤服务返回的账户不匹配"
    );
    let skin = profile_skin(&profile).context("皮肤服务未返回已应用的皮肤")?;
    texture_url(&skin.url)?;
    Ok(UploadResult::Applied(skin))
}

impl Launcher {
    /// Apply to an explicit account, never whichever account happens to be
    /// selected when an awaited request completes. Tokens stay in the core.
    pub async fn apply_account_skin(
        &self,
        account: &str,
        bytes: &[u8],
        slim: bool,
    ) -> Result<Account> {
        validate_upload(bytes)?;
        let client = crate::network::client_builder(self.root(), &self.settings())?
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        self.apply_account_skin_at(&client, ENDPOINT, account, bytes, slim)
            .await
    }

    async fn apply_account_skin_at(
        &self,
        client: &Client,
        endpoint: &str,
        account: &str,
        bytes: &[u8],
        slim: bool,
    ) -> Result<Account> {
        validate_upload(bytes)?;
        // Independent of the download lock; serialize with login/refresh/removal
        // so stale sessions cannot overwrite the newly returned profile.
        let _guard = self.0.account_operation.lock().await;
        let mut session = self.session_for_account(account, false).await?;
        let skin = match upload_at(
            client,
            endpoint,
            account,
            &session.access_token,
            bytes,
            slim,
        )
        .await?
        {
            UploadResult::Applied(skin) => skin,
            UploadResult::Unauthorized => {
                session = self.session_for_account(account, true).await?;
                match upload_at(
                    client,
                    endpoint,
                    account,
                    &session.access_token,
                    bytes,
                    slim,
                )
                .await?
                {
                    UploadResult::Applied(skin) => skin,
                    UploadResult::Unauthorized => {
                        self.0.sessions.invalidate_session(account);
                        bail!("登录凭据已失效，请重新登录后更换皮肤");
                    }
                }
            }
        };
        session.account.skin = Some(skin);
        let updated = session.account.clone();
        self.save_account(updated.clone(), false)?;
        self.0.sessions.remember(session);
        if let Err(error) = self.remember_uploaded_skin(bytes, &updated.skin.as_ref().unwrap().url)
        {
            // The profile has already changed remotely; a local association
            // failure must not incorrectly report that the upload failed.
            let _ = crate::diagnostics::record_error(self.root(), "skin_library", &error);
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PLAYER: &str = "22222222222242228222222222222222";
    const OTHER: &str = "11111111111141118111111111111111";
    const PNG: &[u8] = include_bytes!("../../../../app/src/assets/steve.png");
    fn profile(id: &str, variant: &str) -> String {
        json!({"id": id, "name":"Fixture", "skins":[{
            "state":"ACTIVE", "variant": variant,
            "url": format!("https://textures.minecraft.net/texture/{}", "a".repeat(64))
        }]})
        .to_string()
    }
    fn remember(launcher: &Launcher, id: &str) -> Account {
        let account = Account {
            id: id.into(),
            username: "Fixture".into(),
            client_id: "fixture".into(),
            expires_at: crate::storage::now() + 3600,
            owned: true,
            skin: None,
        };
        launcher.save_account(account.clone(), true).unwrap();
        launcher.0.sessions.remember(crate::auth::MinecraftSession {
            account: account.clone(),
            access_token: format!("access-{id}"),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: std::time::Instant::now() + Duration::from_secs(300),
        });
        account
    }

    #[tokio::test]
    async fn multipart_upload_preserves_png_and_model_and_never_sends_token_in_body() {
        for slim in [false, true] {
            let (url, server) = crate::test_http::serve_bytes_with_headers(vec![(
                200,
                String::new(),
                profile(PLAYER, if slim { "SLIM" } else { "CLASSIC" }),
            )])
            .await;
            let result = upload_at(
                &crate::test_http::client(),
                &url,
                PLAYER,
                "access-fixture",
                PNG,
                slim,
            )
            .await
            .unwrap();
            let UploadResult::Applied(skin) = result else {
                panic!("expected applied skin")
            };
            assert_eq!(skin.slim, slim);
            let requests = server.await.unwrap();
            let request = &requests[0];
            let split = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let headers = String::from_utf8_lossy(&request[..split]).to_lowercase();
            let body = &request[split..];
            let text = String::from_utf8_lossy(body);
            assert!(headers.starts_with("post / "));
            assert!(headers.contains("authorization: bearer access-fixture"));
            assert!(headers.contains("multipart/form-data; boundary="));
            assert!(text.contains(&format!(
                "name=\"variant\"\r\n\r\n{}",
                if slim { "slim" } else { "classic" }
            )));
            assert!(text.contains("name=\"file\"; filename=\"skin.png\""));
            assert!(body.windows(PNG.len()).any(|bytes| bytes == PNG));
            assert!(!text.contains("access-fixture"));
        }
    }

    #[tokio::test]
    async fn upload_updates_only_requested_account_and_cached_session_without_download_lock_or_keychain()
     {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let player = remember(&launcher, PLAYER);
        remember(&launcher, OTHER); // Selection changed before the queued request.
        let saved = launcher.save_skin_bytes("Imported", PNG, true).unwrap();
        let (url, server) = crate::test_http::serve_bytes_with_headers(vec![(
            200,
            String::new(),
            profile(PLAYER, "SLIM"),
        )])
        .await;
        let downloading = launcher.0.operation.lock().await;
        let updated = tokio::time::timeout(
            Duration::from_secs(2),
            launcher.apply_account_skin_at(&crate::test_http::client(), &url, PLAYER, PNG, true),
        )
        .await
        .unwrap()
        .unwrap();
        drop(downloading);
        let requests = server.await.unwrap();
        let headers = String::from_utf8_lossy(&requests[0]);
        assert!(headers.contains(&format!("Bearer access-{PLAYER}")));
        assert!(!headers.contains(&format!("access-{OTHER}")));
        assert!(updated.skin.as_ref().unwrap().slim);
        let db = launcher.snapshot().database;
        assert_eq!(db.selected_account.as_deref(), Some(OTHER));
        assert_eq!(db.guest_skin, "builtin:steve");
        assert!(
            db.accounts
                .iter()
                .find(|a| a.id == OTHER)
                .unwrap()
                .skin
                .is_none()
        );
        let cached = launcher.0.sessions.session(&player).unwrap();
        assert_eq!(
            cached.account.skin.unwrap().url,
            updated.skin.as_ref().unwrap().url
        );
        assert_eq!(cached.access_token, format!("access-{PLAYER}"));
        let metadata = launcher.saved_skins().unwrap().remove(0);
        assert_eq!(metadata.id, saved.id);
        assert_eq!(
            metadata.texture_url.as_deref(),
            Some(updated.skin.as_ref().unwrap().url.as_str())
        );
        launcher
            .edit_saved_skin(&saved.id, "Renamed", false)
            .unwrap();
        assert_eq!(
            launcher.saved_skins().unwrap()[0].texture_url,
            metadata.texture_url
        );
        let public = std::fs::read_to_string(temp.path().join("database.json")).unwrap();
        assert!(!public.contains("access-") && !public.contains("refresh-fixture"));
        drop(launcher);
        let reopened = Launcher::open(Some(temp.path().into())).unwrap();
        assert!(
            reopened
                .snapshot()
                .database
                .accounts
                .iter()
                .find(|a| a.id == PLAYER)
                .unwrap()
                .skin
                .as_ref()
                .unwrap()
                .slim
        );
        assert_eq!(
            reopened.saved_skins().unwrap()[0].texture_url,
            metadata.texture_url
        );
    }

    #[tokio::test]
    async fn rejected_uploads_preserve_profile_and_cache_and_do_not_echo_service_body() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = remember(&launcher, PLAYER);
        for (status, response, expected) in [
            (400, "sensitive-service-body".into(), "PNG"),
            (403, "sensitive-service-body".into(), "权限"),
            (429, "sensitive-service-body".into(), "频繁"),
            (503, "sensitive-service-body".into(), "503"),
            (200, profile(OTHER, "SLIM"), "账户不匹配"),
            (
                200,
                json!({"id":PLAYER, "skins":[]}).to_string(),
                "已应用的皮肤",
            ),
        ] {
            let (url, server) =
                crate::test_http::serve_bytes_with_headers(vec![(status, String::new(), response)])
                    .await;
            let error = launcher
                .apply_account_skin_at(&crate::test_http::client(), &url, PLAYER, PNG, true)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            assert!(!format!("{error:#}").contains("sensitive-service-body"));
            assert!(launcher.snapshot().database.accounts[0].skin.is_none());
            assert!(
                launcher
                    .0
                    .sessions
                    .session(&account)
                    .unwrap()
                    .account
                    .skin
                    .is_none()
            );
            server.await.unwrap();
        }
        let (url, server) =
            crate::test_http::serve_bytes_with_headers(vec![(401, String::new(), String::new())])
                .await;
        assert!(matches!(
            upload_at(
                &crate::test_http::client(),
                &url,
                PLAYER,
                "expired-fixture",
                PNG,
                false
            )
            .await
            .unwrap(),
            UploadResult::Unauthorized
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn invalid_uploads_do_not_access_credentials_and_redirects_do_not_forward_them() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let mut large = PNG.to_vec();
        large[16..20].copy_from_slice(&128u32.to_be_bytes());
        assert!(
            launcher
                .apply_account_skin("missing", &large, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("64×64")
        );
        assert!(
            launcher
                .apply_account_skin("missing", b"invalid", false)
                .await
                .unwrap_err()
                .to_string()
                .contains("PNG")
        );
        remember(&launcher, PLAYER);
        let (url, server) = crate::test_http::serve_bytes_with_headers(vec![(
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
            launcher
                .apply_account_skin_at(&client, &url, PLAYER, PNG, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("307")
        );
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
