use anyhow::{Result, anyhow};
use std::path::Path;

pub(crate) trait ProxyVault {
    fn read(&self, key: &str) -> Result<String>;
    fn write(&self, key: &str, password: &str) -> Result<()>;
    fn remove(&self, key: &str) -> Result<()>;
}

pub(crate) struct SystemVault<'a>(pub &'a Path);
impl SystemVault<'_> {
    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        uuid::Uuid::parse_str(key).map_err(|_| anyhow!("代理密码记录无效，请重新填写密码"))?;
        crate::auth::secret_entry(self.0, &format!("proxy-{key}"))
            .map_err(|_| anyhow!("无法访问系统凭据存储"))
    }
}
impl ProxyVault for SystemVault<'_> {
    fn read(&self, key: &str) -> Result<String> {
        self.entry(key)?
            .get_password()
            .map_err(|_| anyhow!("无法读取代理密码，请重新填写密码"))
    }
    fn write(&self, key: &str, password: &str) -> Result<()> {
        self.entry(key)?
            .set_password(password)
            .map_err(|_| anyhow!("无法保存代理密码，请检查系统凭据存储"))
    }
    fn remove(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(anyhow!("无法移除旧代理密码")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Launcher, model::ProxyPassword};
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    #[derive(Default)]
    struct MemoryVault {
        entries: RefCell<HashMap<String, String>>,
        fail: Cell<bool>,
    }
    impl ProxyVault for MemoryVault {
        fn read(&self, key: &str) -> Result<String> {
            self.entries
                .borrow()
                .get(key)
                .cloned()
                .ok_or_else(|| anyhow!("missing"))
        }
        fn write(&self, key: &str, password: &str) -> Result<()> {
            if self.fail.get() {
                return Err(anyhow!("locked"));
            }
            self.entries
                .borrow_mut()
                .insert(key.into(), password.into());
            Ok(())
        }
        fn remove(&self, key: &str) -> Result<()> {
            self.entries.borrow_mut().remove(key);
            Ok(())
        }
    }

    #[test]
    fn password_is_write_only_and_kept_until_explicitly_changed_or_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().into())).unwrap();
        let vault = MemoryVault::default();
        let mut settings = launcher.settings();
        settings.proxy_username = "test-user".into();
        settings.proxy_password = Some(ProxyPassword("test-password".into()));
        settings.proxy_password_key = Some("untrusted-input".into());
        assert!(!format!("{settings:?}").contains("test-password"));
        launcher.save_settings_with_vault(settings, &vault).unwrap();
        let key = launcher.settings().proxy_password_key.unwrap();
        assert_ne!(key, "untrusted-input");
        assert_eq!(vault.read(&key).unwrap(), "test-password");
        assert!(launcher.settings().proxy_password.is_none());
        let mut settings = launcher.settings();
        settings.concurrency = 12;
        settings.proxy_password_key = None; // Old snapshots must not clear it.
        launcher.save_settings_with_vault(settings, &vault).unwrap();
        assert_eq!(launcher.settings().proxy_password_key.as_ref(), Some(&key));
        for filename in ["database.json", "database.json.bak"] {
            let saved = std::fs::read_to_string(dir.path().join(filename)).unwrap();
            assert!(!saved.contains("test-password"));
            assert!(!saved.contains("\"proxy_password\""));
        }
        assert!(
            !serde_json::to_string(&launcher.snapshot())
                .unwrap()
                .contains("test-password")
        );
        let mut settings = launcher.settings();
        settings.proxy_password = Some(ProxyPassword(String::new()));
        launcher.save_settings_with_vault(settings, &vault).unwrap();
        assert!(launcher.settings().proxy_password_key.is_none());
        assert!(vault.entries.borrow().is_empty());
    }

    #[test]
    fn failed_vault_or_database_write_preserves_the_previous_password() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(dir.path().into())).unwrap();
        let vault = MemoryVault::default();
        let mut settings = launcher.settings();
        settings.proxy_password = Some(ProxyPassword("old-fixture".into()));
        launcher.save_settings_with_vault(settings, &vault).unwrap();
        let key = launcher.settings().proxy_password_key.unwrap();
        let mut next = launcher.settings();
        next.proxy_password = Some(ProxyPassword("new-fixture".into()));
        vault.fail.set(true);
        assert!(
            launcher
                .save_settings_with_vault(next.clone(), &vault)
                .is_err()
        );
        vault.fail.set(false);
        std::fs::rename(
            dir.path().join("database.json"),
            dir.path().join("saved.json"),
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("database.json")).unwrap();
        assert!(launcher.save_settings_with_vault(next, &vault).is_err());
        assert_eq!(launcher.settings().proxy_password_key.as_ref(), Some(&key));
        assert_eq!(vault.read(&key).unwrap(), "old-fixture");
        assert_eq!(vault.entries.borrow().len(), 1);
    }
}
