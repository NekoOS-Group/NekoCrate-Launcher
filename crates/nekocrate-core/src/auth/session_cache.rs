//! Account-bound credentials for this process only; never serialized or logged.
use super::MinecraftSession;
use crate::model::Account;
use std::{collections::HashMap, sync::Mutex, time::Instant};

struct Credentials {
    client_id: String,
    refresh_token: String,
    session: Option<MinecraftSession>,
}

#[derive(Default)]
pub(crate) struct SessionCache(Mutex<HashMap<String, Credentials>>);

impl SessionCache {
    pub fn session(&self, account: &Account) -> Option<MinecraftSession> {
        let entries = self.0.lock().unwrap();
        let credentials = entries.get(&account.id)?;
        let session = credentials.session.as_ref()?;
        (credentials.client_id == account.client_id
            && session.account.owned
            // Some monotonic clocks stop during sleep. The wall-clock guard
            // also expires those sessions after wake; the monotonic guard
            // prevents a wall-clock rollback from extending their lifetime.
            && crate::storage::now() < session.account.expires_at.saturating_sub(60)
            && Instant::now() < session.valid_until)
            .then(|| session.clone())
    }

    pub fn refresh_token(&self, account: &Account) -> Option<String> {
        let entries = self.0.lock().unwrap();
        let credentials = entries.get(&account.id)?;
        (credentials.client_id == account.client_id).then(|| credentials.refresh_token.clone())
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
        cache.remember(MinecraftSession {
            valid_until: Instant::now(),
            ..session.clone()
        });
        assert!(cache.session(&account).is_none());
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
        cache.remove(&account.id);
        assert!(cache.refresh_token(&account).is_none());
    }
}
