use crate::{Launcher, model::Account};
use anyhow::Result;

impl Launcher {
    /// Background refresh never changes the selected player or joins the download job.
    pub async fn sync_account(&self, id: &str) -> Result<Option<Account>> {
        // Give foreground login/profile changes priority; the next check can retry.
        let Ok(_guard) = self.0.account_operation.try_lock() else {
            return Ok(None);
        };
        let Some(account) = self
            .snapshot()
            .database
            .accounts
            .into_iter()
            .find(|a| a.id == id)
        else {
            return Ok(None);
        };
        if !self.0.sessions.background_refresh_due(&account) {
            return Ok(Some(account));
        }
        // Refreshing credentials already fetches and validates the profile.
        Ok(Some(self.session_for_account(id, false).await?.account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MinecraftSession;
    use std::time::{Duration, Instant};

    const PLAYER: &str = "22222222222242228222222222222222";
    fn account() -> Account {
        Account {
            id: PLAYER.into(),
            username: "Original".into(),
            client_id: "fixture".into(),
            expires_at: crate::storage::now() + 3600,
            owned: true,
            skin: None,
        }
    }

    #[tokio::test]
    async fn fresh_persisted_login_needs_neither_memory_tokens_nor_keychain_reads() {
        let temp = tempfile::tempdir().unwrap();
        {
            let launcher = Launcher::open(Some(temp.path().into())).unwrap();
            launcher.save_account(account(), true).unwrap();
        }
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        // A restarted process has no credentials at all in this fixture. Checks
        // must still succeed without reading a secret or requesting a new token.
        assert!(launcher.0.sessions.session(&account()).is_none());
        assert!(launcher.0.sessions.refresh_token(&account()).is_none());
        for _ in 0..3 {
            let cached = launcher.sync_account(PLAYER).await.unwrap().unwrap();
            assert_eq!(cached.username, "Original");
            assert_eq!(
                cached.expires_at,
                launcher.snapshot().database.accounts[0].expires_at
            );
        }
        assert!(launcher.0.sessions.refresh_token(&account()).is_none());
    }

    #[tokio::test]
    async fn background_checks_preserve_selection_tokens_and_foreground_operations() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let account = account();
        launcher.save_account(account.clone(), true).unwrap();
        launcher.0.sessions.remember(MinecraftSession {
            account: account.clone(),
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: Instant::now() + Duration::from_secs(300),
        });
        launcher.select_guest().unwrap();
        let downloading = launcher.0.operation.lock().await;
        let cached = launcher.sync_account(PLAYER).await.unwrap().unwrap();
        assert_eq!(cached.username, "Original");
        assert!(launcher.snapshot().database.selected_account.is_none());
        let session = launcher.0.sessions.session(&account).unwrap();
        assert_eq!(session.access_token, "access-fixture");
        assert_eq!(session.refresh_token, "refresh-fixture");
        assert!(
            launcher
                .sync_account("removed-account")
                .await
                .unwrap()
                .is_none()
        );
        let foreground = launcher.0.account_operation.lock().await;
        assert!(launcher.sync_account(PLAYER).await.unwrap().is_none());
        drop(foreground);
        drop(downloading);
    }
}
