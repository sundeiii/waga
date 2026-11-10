//! OS-backed secret storage. On Windows this is Credential Manager.
//! Secrets never touch SQLite, logs, or the frontend after submission.

use std::collections::HashMap;
use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

pub const SERVICE: &str = "yoru";

pub trait SecretStore: Send + Sync {
    fn set(&self, account: &str, secret: &str) -> AppResult<()>;
    fn get(&self, account: &str) -> AppResult<Option<Zeroizing<String>>>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, account: &str) -> AppResult<()>;
}

pub fn password_account(server_id: &str) -> String {
    format!("server/{server_id}/password")
}

pub fn passphrase_account(server_id: &str) -> String {
    format!("server/{server_id}/key-passphrase")
}

/// Remove every secret belonging to a server profile.
pub fn delete_server_secrets(store: &dyn SecretStore, server_id: &str) -> AppResult<()> {
    store.delete(&password_account(server_id))?;
    store.delete(&passphrase_account(server_id))?;
    Ok(())
}

/// Windows Credential Manager (or the platform equivalent via `keyring`).
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, account).map_err(|e| AppError::Credentials(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|e| AppError::Credentials(e.to_string()))
    }

    fn get(&self, account: &str) -> AppResult<Option<Zeroizing<String>>> {
        match Self::entry(account)?.get_password() {
            Ok(s) => Ok(Some(Zeroizing::new(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Credentials(e.to_string())),
        }
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Credentials(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

impl SecretStore for MemoryStore {
    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        self.inner.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }
    fn get(&self, account: &str) -> AppResult<Option<Zeroizing<String>>> {
        Ok(self.inner.lock().unwrap().get(account).cloned().map(Zeroizing::new))
    }
    fn delete(&self, account: &str) -> AppResult<()> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_server_secrets_removes_both_and_tolerates_missing() {
        let s = MemoryStore::default();
        s.set(&password_account("a"), "pw").unwrap();
        s.set(&passphrase_account("a"), "pp").unwrap();
        s.set(&password_account("b"), "other").unwrap();
        delete_server_secrets(&s, "a").unwrap();
        assert!(s.get(&password_account("a")).unwrap().is_none());
        assert!(s.get(&passphrase_account("a")).unwrap().is_none());
        assert_eq!(s.len(), 1, "other servers' secrets untouched");
        delete_server_secrets(&s, "a").unwrap(); // idempotent
    }

    /// Real Windows Credential Manager round-trip. Run explicitly:
    /// `cargo test -- --ignored keyring_roundtrip`
    #[test]
    #[ignore]
    fn keyring_roundtrip() {
        let s = KeyringStore;
        let acct = format!("test/{}", uuid::Uuid::new_v4());
        s.set(&acct, "s3cret").unwrap();
        assert_eq!(s.get(&acct).unwrap().as_deref().map(|x| x.as_str()), Some("s3cret"));
        s.delete(&acct).unwrap();
        assert!(s.get(&acct).unwrap().is_none());
    }
}
