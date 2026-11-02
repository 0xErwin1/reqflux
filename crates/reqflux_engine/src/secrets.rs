//! Secret variable values, kept out of workspace files in a [`SecretStore`] (keyring-secrets
//! S1, S2, S8): the OS keyring in the app, an in-memory store in tests.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::{Mutex, PoisonError};

use reqflux_core::model::Id;

/// The keyring service every ReqFlux secret is stored under.
pub const SECRET_SERVICE: &str = "reqflux";

/// Where a secret variable is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretScope {
    /// The workspace globals.
    Global,
    /// The variables of the collection with this id.
    Collection(Id),
    /// The variables of the environment with this id.
    Environment(Id),
}

impl SecretScope {
    /// The scope part of a keyring account: `global`, `collection/<id>` or
    /// `environment/<id>`. The app state keys current values by the same text.
    #[must_use]
    pub fn path(self) -> String {
        match self {
            Self::Global => "global".to_owned(),
            Self::Collection(id) => format!("collection/{id}"),
            Self::Environment(id) => format!("environment/{id}"),
        }
    }

    /// Parses the text [`SecretScope::path`] writes; `None` for anything else.
    #[must_use]
    pub fn from_path(text: &str) -> Option<Self> {
        if text == "global" {
            return Some(Self::Global);
        }
        let (kind, id) = text.split_once('/')?;
        let id = id.parse().ok()?;
        match kind {
            "collection" => Some(Self::Collection(id)),
            "environment" => Some(Self::Environment(id)),
            _ => None,
        }
    }
}

/// One secret variable: its workspace, its scope and its key. Ids, not slugs, so a rename or
/// a move keeps the value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey {
    /// The id of the workspace manifest.
    pub workspace_id: Id,
    /// Where the variable is declared.
    pub scope: SecretScope,
    /// The variable key.
    pub key: String,
}

impl SecretKey {
    /// The keyring account: `<workspace id>/global/<key>`,
    /// `<workspace id>/collection/<collection id>/<key>` or
    /// `<workspace id>/environment/<environment id>/<key>`.
    #[must_use]
    pub fn account(&self) -> String {
        format!("{}/{}/{}", self.workspace_id, self.scope.path(), self.key)
    }
}

/// Why a secret store operation failed. The message never contains a secret value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// The store cannot be reached at all, such as no Secret Service on a headless Linux box
    /// or a locked keychain.
    #[error("the OS keyring is not available: {0}")]
    Unavailable(String),
    /// The store answered but the operation failed for this entry.
    #[error("the OS keyring failed: {0}")]
    Failed(String),
}

/// Reads and writes secret values by keyring account: a [`SecretKey`]'s for a secret
/// variable, a [`NetworkSecret`](crate::NetworkSecret)'s for a proxy password or a
/// certificate passphrase. A missing entry is `Ok(None)`, never an error.
pub trait SecretStore: Debug + Send + Sync {
    /// The value stored under the keyring `account`, or `None` when none is.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn get_account(&self, account: &str) -> Result<Option<String>, SecretError>;

    /// Stores `value` under the keyring `account`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn set_account(&self, account: &str, value: &str) -> Result<(), SecretError>;

    /// Deletes the value under the keyring `account`; deleting a missing entry succeeds.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn delete_account(&self, account: &str) -> Result<(), SecretError>;

    /// The value stored for `key`, or `None` when none is.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        self.get_account(&key.account())
    }

    /// Stores `value` for `key`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.set_account(&key.account(), value)
    }

    /// Deletes the value for `key`; deleting a missing entry succeeds.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.delete_account(&key.account())
    }

    /// Whether a value is stored for `key`.
    ///
    /// # Errors
    ///
    /// The store is unavailable or failed.
    fn is_set(&self, key: &SecretKey) -> Result<bool, SecretError> {
        self.get(key).map(|value| value.is_some())
    }
}

/// The OS keyring: Secret Service on Linux, Keychain on macOS, Credential Manager on Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry(account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SECRET_SERVICE, account).map_err(keyring_error)
    }
}

impl SecretStore for KeyringSecretStore {
    fn get_account(&self, account: &str) -> Result<Option<String>, SecretError> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keyring_error(error)),
        }
    }

    fn set_account(&self, account: &str, value: &str) -> Result<(), SecretError> {
        Self::entry(account)?
            .set_password(value)
            .map_err(keyring_error)
    }

    fn delete_account(&self, account: &str) -> Result<(), SecretError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error(error)),
        }
    }
}

/// Maps a keyring error to a [`SecretError`] whose text cannot carry a value: the variants
/// that hold stored bytes are described, never printed.
fn keyring_error(error: keyring::Error) -> SecretError {
    use keyring::Error;
    match error {
        Error::NoStorageAccess(cause) => SecretError::Unavailable(cause.to_string()),
        Error::NoDefaultStore => {
            SecretError::Unavailable("no credential store could be started".to_owned())
        }
        Error::PlatformFailure(cause) => SecretError::Failed(cause.to_string()),
        Error::BadEncoding(_) | Error::BadDataFormat(..) => {
            SecretError::Failed("the stored value is not valid text".to_owned())
        }
        Error::NoEntry => SecretError::Failed("no entry".to_owned()),
        Error::Ambiguous(_) => SecretError::Failed("more than one entry matches".to_owned()),
        Error::TooLong(attribute, limit) => {
            SecretError::Failed(format!("{attribute} is longer than {limit}"))
        }
        Error::Invalid(attribute, reason) => {
            SecretError::Failed(format!("invalid {attribute}: {reason}"))
        }
        Error::NotSupportedByStore(operation) => {
            SecretError::Failed(format!("the store does not support {operation}"))
        }
        // A variant added after this was written; its text could carry anything.
        _ => SecretError::Failed("an unexpected keyring error".to_owned()),
    }
}

/// An in-memory [`SecretStore`] for tests and front ends that must not touch the OS keyring.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    values: Mutex<BTreeMap<String, String>>,
    unavailable: Option<String>,
}

impl MemorySecretStore {
    /// An empty, available store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose every operation fails with [`SecretError::Unavailable`] and `reason`.
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            values: Mutex::default(),
            unavailable: Some(reason.into()),
        }
    }

    /// The accounts that hold a value, in order.
    #[must_use]
    pub fn accounts(&self) -> Vec<String> {
        self.values
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    fn check(&self) -> Result<(), SecretError> {
        match &self.unavailable {
            Some(reason) => Err(SecretError::Unavailable(reason.clone())),
            None => Ok(()),
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn get_account(&self, account: &str) -> Result<Option<String>, SecretError> {
        self.check()?;
        Ok(self
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(account)
            .cloned())
    }

    fn set_account(&self, account: &str, value: &str) -> Result<(), SecretError> {
        self.check()?;
        self.values
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete_account(&self, account: &str) -> Result<(), SecretError> {
        self.check()?;
        self.values
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_errors_never_carry_the_stored_bytes() {
        let value = "s3cr3t-value";
        let errors = [
            keyring::Error::BadEncoding(value.as_bytes().to_vec()),
            keyring::Error::BadDataFormat(
                value.as_bytes().to_vec(),
                Box::new(std::io::Error::other("bad")),
            ),
        ];
        for error in errors {
            let text = keyring_error(error).to_string();
            assert!(!text.contains(value), "{text}");
            assert_eq!(
                text,
                "the OS keyring failed: the stored value is not valid text"
            );
        }
    }

    #[test]
    fn no_storage_access_is_unavailable() {
        let error = keyring_error(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("locked"),
        )));
        assert_eq!(
            error,
            SecretError::Unavailable("locked".to_owned()),
            "{error}"
        );
    }

    #[test]
    #[ignore = "touches the real OS keyring; run by hand on a desktop session"]
    fn keyring_store_round_trips() -> Result<(), SecretError> {
        let store = KeyringSecretStore;
        let key = SecretKey {
            workspace_id: Id::new(),
            scope: SecretScope::Global,
            key: "reqflux-test".to_owned(),
        };
        store.set(&key, "value")?;
        assert_eq!(store.get(&key)?.as_deref(), Some("value"));
        store.delete(&key)?;
        assert_eq!(store.get(&key)?, None);
        Ok(())
    }
}
