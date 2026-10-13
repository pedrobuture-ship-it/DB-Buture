//! Secure credential storage. Passwords live in the OS keyring only.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "db-buture";

pub trait CredentialStore: Send + Sync {
    fn save(&self, key: &str, secret: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<Option<String>>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

/// Reference stored in the internal SQLite instead of the password.
pub fn secret_key_for(connection_id: &str) -> String {
    format!("db-buture.connection.{connection_id}")
}

/// Native OS keyring (Secret Service / Keychain / Credential Manager).
/// There is deliberately no plaintext fallback: failures surface as errors.
pub struct KeyringStore;

impl KeyringStore {
    fn entry(key: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, key).map_err(map_err)
    }
}

fn map_err(err: keyring::Error) -> AppError {
    AppError::CredentialStore(format!(
        "{err}. DB-BUTURE never stores passwords in plaintext; start a keyring service \
         (e.g. gnome-keyring or KWallet) and try again"
    ))
}

impl CredentialStore for KeyringStore {
    fn save(&self, key: &str, secret: &str) -> AppResult<()> {
        Self::entry(key)?.set_password(secret).map_err(map_err)
    }

    fn get(&self, key: &str) -> AppResult<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn delete(&self, key: &str) -> AppResult<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// In-memory store for tests and keyring-less environments in test builds only.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl CredentialStore for MemoryStore {
    fn save(&self, key: &str, secret: &str) -> AppResult<()> {
        self.0.lock().unwrap().insert(key.into(), secret.into());
        Ok(())
    }
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_key_format() {
        assert_eq!(secret_key_for("abc"), "db-buture.connection.abc");
    }

    #[test]
    fn memory_store_roundtrip() {
        let s = MemoryStore::default();
        s.save("k", "v").unwrap();
        assert_eq!(s.get("k").unwrap().as_deref(), Some("v"));
        s.delete("k").unwrap();
        assert_eq!(s.get("k").unwrap(), None);
    }
}
