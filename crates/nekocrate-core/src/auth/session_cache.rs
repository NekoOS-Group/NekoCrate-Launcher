//! Account-bound credentials for this process only; never serialized or logged.
use super::MinecraftSession;
use crate::model::Account;
use std::{collections::HashMap, sync::Mutex, time::Instant};

struct Credentials {
    client_id: String,
    refresh_token: String,
    session: Option<MinecraftSession>,
}

fn session_is_valid(session: &MinecraftSession) -> bool {
    session.account.owned
        // Wall time notices sleep; monotonic time resists clock rollback.
        && crate::storage::now() < session.account.expires_at.saturating_sub(60)
        && Instant::now() < session.valid_until
}

#[derive(Default)]
pub(crate) struct SessionCache(Mutex<HashMap<String, Credentials>>);

impl SessionCache {
    pub fn session(&self, account: &Account) -> Option<MinecraftSession> {
        let entries = self.0.lock().unwrap();
        let credentials = entries.get(&account.id)?;
        let session = credentials.session.as_ref()?;
        (credentials.client_id == account.client_id && session_is_valid(session))
            .then(|| session.clone())
    }

    pub fn refresh_token(&self, account: &Account) -> Option<String> {
        let entries = self.0.lock().unwrap();
        let credentials = entries.get(&account.id)?;
        (credentials.client_id == account.client_id).then(|| credentials.refresh_token.clone())
    }

    /// Restarting loses access tokens, but must not cause background Keychain
    /// reads while the persisted login is still fresh. Launch can obtain a
    /// session on demand; a known in-memory expiry remains authoritative.
    pub fn background_refresh_due(&self, account: &Account) -> bool {
        let entries = self.0.lock().unwrap();
        match entries
            .get(&account.id)
            .filter(|credentials| credentials.client_id == account.client_id)
            .and_then(|credentials| credentials.session.as_ref())
        {
            Some(session) => !session_is_valid(session),
            None => crate::storage::now() >= account.expires_at.saturating_sub(60),
        }
    }

    pub fn remember_refresh(&self, account: &Account, token: String) {
        let mut entries = self.0.lock().unwrap();
        let credentials = entries
            .entry(account.id.clone())
            .or_insert_with(|| Credentials {
                client_id: account.client_id.clone(),
                refresh_token: String::new(),
                session: None,
            });
        if credentials.client_id != account.client_id {
            credentials.client_id.clone_from(&account.client_id);
            credentials.session = None;
        }
        credentials.refresh_token = token;
    }

    pub fn remember(&self, session: MinecraftSession) {
        self.0.lock().unwrap().insert(
            session.account.id.clone(),
            Credentials {
                client_id: session.account.client_id.clone(),
                refresh_token: session.refresh_token.clone(),
                session: Some(session),
            },
        );
    }

    pub fn invalidate_session(&self, id: &str) {
        if let Some(credentials) = self.0.lock().unwrap().get_mut(id) {
            credentials.session = None;
        }
    }

    pub fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn sessions_are_account_bound_expire_and_keep_refresh_credentials_until_removed() {
        let cache = SessionCache::default();
        let account = Account {
            id: "account-a".into(),
            username: "A".into(),
            client_id: "app-a".into(),
            expires_at: crate::storage::now() + 3600,
            owned: true,
            skin: None,
        };
        let session = MinecraftSession {
            account: account.clone(),
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: Instant::now() + Duration::from_secs(300),
        };
        cache.remember(session.clone());
        assert!(cache.session(&account).is_some());
        assert!(!cache.background_refresh_due(&account));
        let mut other = account.clone();
        other.id = "account-b".into();
        assert!(cache.session(&other).is_none());
        assert!(cache.refresh_token(&other).is_none());
        other = account.clone();
        other.client_id = "app-b".into();
        assert!(cache.session(&other).is_none());
        assert!(cache.refresh_token(&other).is_none());

        let mut slept = session.clone();
        slept.account.expires_at = 0;
        cache.remember(slept);
        assert!(
            cache.session(&account).is_none(),
            "sleep cannot keep an expired token alive"
        );
        assert!(cache.background_refresh_due(&account));
        cache.remember(MinecraftSession {
            valid_until: Instant::now(),
            ..session.clone()
        });
        assert!(cache.session(&account).is_none());
        assert!(cache.background_refresh_due(&account));
        assert_eq!(
            cache.refresh_token(&account).as_deref(),
            Some("refresh-fixture")
        );
        cache.remember_refresh(&account, "rotated-fixture".into());
        assert_eq!(
            cache.refresh_token(&account).as_deref(),
            Some("rotated-fixture")
        );

        cache.remember(session);
        cache.invalidate_session(&account.id);
        assert!(cache.session(&account).is_none());
        assert!(cache.refresh_token(&account).is_some());
        assert!(!cache.background_refresh_due(&account));
        cache.remove(&account.id);
        assert!(cache.refresh_token(&account).is_none());
    }

    #[test]
    fn persisted_login_only_refreshes_near_expiry_without_a_cached_session() {
        let cache = SessionCache::default();
        let mut account = Account {
            id: "account-a".into(),
            username: "A".into(),
            client_id: "app-a".into(),
            expires_at: crate::storage::now() + 3600,
            owned: true,
            skin: None,
        };
        assert!(!cache.background_refresh_due(&account));
        account.expires_at = crate::storage::now() + 60;
        assert!(cache.background_refresh_due(&account));
        account.expires_at = 0;
        assert!(cache.background_refresh_due(&account));

        cache.remember(MinecraftSession {
            account: account.clone(),
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            xuid: "xuid-fixture".into(),
            valid_until: Instant::now(),
        });
        account.expires_at = crate::storage::now() + 3600;
        assert!(cache.background_refresh_due(&account));
        account.client_id = "app-b".into();
        assert!(!cache.background_refresh_due(&account));
    }
}
