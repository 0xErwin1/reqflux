//! Copying variables from one scope into another (feedback-nov3 S13): the keys of a
//! collection or of the globals into an environment of the workspace.
//!
//! A new key goes through [`Engine::add_variable`], so it keeps every invariant that
//! enforces (unique keys, the secret store ordering, the reload after each write). A key the
//! destination already declares is skipped, or overwritten in place (keeping its position)
//! under [`CopyPolicy::Overwrite`]; the overwrite is one file write per key, so a failure
//! leaves the old variable intact and the key is skipped.
//!
//! A secret copies its keyring value to the destination's own entry. When the keyring
//! cannot be read or written the key is skipped as [`SkipReason::Keyring`]; it is never
//! copied without its value. The report names keys, never a value.

use reqflux_core::model::Variable;
use reqflux_store::VariableChange;

use crate::engine::Engine;
use crate::error::VariableError;
use crate::secrets::SecretScope;
use crate::variables::scope_file;

/// What to do with a key the destination already declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CopyPolicy {
    /// Leave the destination's variable as it is.
    #[default]
    Skip,
    /// Replace the destination's variable (value, description, enabled flag and where the
    /// value is stored) with the source's, in place.
    Overwrite,
}

/// Why a key was not copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The destination declares it and the policy is [`CopyPolicy::Skip`] (or the source
    /// and the destination are the same scope).
    Exists,
    /// The source scope does not declare it.
    Missing,
    /// The keyring could not be read or written, so the secret value would not have moved.
    Keyring,
}

/// A key that was not copied, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedVariable {
    /// The variable's key.
    pub key: String,
    /// Why it was skipped.
    pub reason: SkipReason,
}

/// What [`Engine::copy_variables`] did, per key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyReport {
    /// The keys now in the destination, in the order they were asked for.
    pub copied: Vec<String>,
    /// The keys left out, in the order they were asked for.
    pub skipped: Vec<SkippedVariable>,
}

impl CopyReport {
    fn skip(&mut self, key: &str, reason: SkipReason) {
        self.skipped.push(SkippedVariable {
            key: key.to_owned(),
            reason,
        });
    }
}

impl Engine {
    /// Copies the variables `keys` of `from` into `to`, applying `policy` to the keys `to`
    /// already declares. A repeated key is copied once.
    ///
    /// Each key is its own write: when a later one fails with a non-keyring error, the
    /// earlier ones stay copied.
    ///
    /// # Errors
    ///
    /// The store error (an unknown source or destination scope, even when no key reaches it,
    /// or a conflict) or [`VariableError::Reload`]; a
    /// keyring failure is a [`SkipReason::Keyring`] in the report instead.
    pub fn copy_variables(
        &mut self,
        from: SecretScope,
        keys: &[String],
        to: SecretScope,
        policy: CopyPolicy,
    ) -> Result<CopyReport, VariableError> {
        let source = reqflux_store::scope_variables(&self.loaded, scope_file(from))?.to_vec();
        reqflux_store::scope_variables(&self.loaded, scope_file(to))?;
        let mut report = CopyReport::default();
        let mut seen: Vec<&str> = Vec::new();
        for key in keys {
            if seen.contains(&key.as_str()) {
                continue;
            }
            seen.push(key);
            let Some(variable) = source.iter().find(|variable| variable.key == *key) else {
                report.skip(key, SkipReason::Missing);
                continue;
            };
            let existing = reqflux_store::scope_variables(&self.loaded, scope_file(to))?
                .iter()
                .position(|existing| existing.key == *key);
            if existing.is_some() && (policy == CopyPolicy::Skip || from == to) {
                report.skip(key, SkipReason::Exists);
                continue;
            }
            let value = match self.copy_value(from, variable) {
                Ok(value) => value,
                Err(()) => {
                    report.skip(key, SkipReason::Keyring);
                    continue;
                }
            };
            let outcome = match existing {
                Some(index) => self.overwrite(to, index, variable, value),
                None => self.add_variable(
                    to,
                    Variable {
                        value,
                        ..variable.clone()
                    },
                ),
            };
            match outcome {
                Ok(()) => report.copied.push(key.clone()),
                Err(VariableError::SecretCleanup { account, .. }) => {
                    log::warn!("copied {key}, but the keyring entry {account} was left behind");
                    report.copied.push(key.clone());
                }
                Err(VariableError::Secret(error)) => {
                    log::warn!("{key} was not copied: {error}");
                    report.skip(key, SkipReason::Keyring);
                }
                Err(VariableError::Orphaned { account, .. }) => {
                    log::warn!("{key} was not copied; the keyring entry {account} is left behind");
                    report.skip(key, SkipReason::Keyring);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(report)
    }

    /// The value `variable` carries into the copy: the file value, or a secret's keyring
    /// value. `Err` when the keyring cannot be read.
    fn copy_value(&self, from: SecretScope, variable: &Variable) -> Result<Option<String>, ()> {
        if !variable.secret {
            return Ok(variable.value.clone());
        }
        match self
            .secret_store()
            .get(&self.secret_key(from, &variable.key))
        {
            Ok(value) => Ok(value),
            Err(error) => {
                log::warn!(
                    "the keyring entry of {} cannot be read: {error}",
                    variable.key
                );
                Err(())
            }
        }
    }

    /// Makes the variable at `index` in `to` match `variable`, carrying `value`, in one file
    /// write.
    ///
    /// A secret's new value is stored before the write, so a keyring failure refuses the
    /// whole key; when the write then fails the previous entry is put back. The entry of a
    /// secret that stops being one is deleted after the write, as in
    /// [`Engine::edit_variable`].
    fn overwrite(
        &mut self,
        to: SecretScope,
        index: usize,
        variable: &Variable,
        value: Option<String>,
    ) -> Result<(), VariableError> {
        let current = reqflux_store::scope_variables(&self.loaded, scope_file(to))?
            .get(index)
            .cloned()
            .ok_or(reqflux_store::StoreError::NoVariable { index })?;
        let replacement = Variable {
            value: if variable.secret { None } else { value.clone() },
            ..variable.clone()
        };
        let plan = reqflux_store::plan_variables(
            &self.root,
            &self.loaded,
            scope_file(to),
            VariableChange::Replace(index, replacement),
        )?;
        let store = self.secret_store().clone();
        let account = self.secret_key(to, &variable.key);
        let mut restore = None;
        if variable.secret {
            let previous = store.get(&account)?;
            match &value {
                Some(value) => store.set(&account, value)?,
                None => store.delete(&account)?,
            }
            restore = Some(previous);
        }
        if let Err(error) = plan.write() {
            let undone = match restore {
                Some(Some(previous)) => store.set(&account, &previous),
                Some(None) => store.delete(&account),
                None => Ok(()),
            };
            return Err(match undone {
                Ok(()) => error.into(),
                Err(rollback) => VariableError::Orphaned {
                    source: Box::new(error),
                    account: account.account(),
                    rollback,
                },
            });
        }
        let cleanup = if current.secret && !variable.secret {
            store
                .delete(&account)
                .map_err(|source| VariableError::SecretCleanup {
                    account: account.account(),
                    source,
                })
        } else {
            Ok(())
        };
        self.reload().map_err(VariableError::Reload)?;
        cleanup
    }
}
