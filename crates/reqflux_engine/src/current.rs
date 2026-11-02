//! Current values (variables-editor S4, S5): a per-user value that overrides the shared value
//! of a variable within its scope, kept in the app state, never in the workspace.
//!
//! A front end reads a workspace's current values with
//! [`AppState::current_values`](crate::state::AppState::current_values) and passes them in
//! [`ExecuteOptions::current`](crate::ExecuteOptions::current); `reqflux run` passes none.
//! A secret variable has no current value: its keyring value is both, so resolution ignores
//! a current value for a secret and [`Engine::persist_script_writes`] never stores one.

use std::collections::BTreeMap;
use std::fmt;

use reqflux_core::model::{EnvironmentNode, Variable, Workspace};
use reqflux_script::{VariableScope, VariableWrite};

use crate::ExecuteOptions;
use crate::engine::Engine;
use crate::error::PersistError;
use crate::prepare::select_environment;
use crate::secrets::{SecretKey, SecretScope};
use crate::select::Target;
use crate::state::AppState;

/// The current values of one workspace, by scope and key. Its `Debug` output names the keys,
/// never the values.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CurrentValues {
    values: BTreeMap<(SecretScope, String), String>,
}

impl fmt::Debug for CurrentValues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_set()
            .entries(self.values.keys().map(|(scope, key)| (scope.path(), key)))
            .finish()
    }
}

impl CurrentValues {
    /// No current values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current value of `key` in `scope`, if any.
    #[must_use]
    pub fn get(&self, scope: SecretScope, key: &str) -> Option<&str> {
        self.values
            .get(&(scope, key.to_owned()))
            .map(String::as_str)
    }

    /// Sets the current value of `key` in `scope`, replacing any previous one.
    pub fn insert(&mut self, scope: SecretScope, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert((scope, key.into()), value.into());
    }

    /// Removes the current value of `key` in `scope`, returning it.
    pub fn remove(&mut self, scope: SecretScope, key: &str) -> Option<String> {
        self.values.remove(&(scope, key.to_owned()))
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Every current value as `(scope, key, value)`, ordered by scope and key.
    pub fn iter(&self) -> impl Iterator<Item = (SecretScope, &str, &str)> {
        self.values
            .iter()
            .map(|((scope, key), value)| (*scope, key.as_str(), value.as_str()))
    }

    /// The current values of `scope` whose key `declared` does not hold, in key order: a
    /// script can set a variable the scope does not declare, and it still resolves.
    pub(crate) fn undeclared<'a>(
        &'a self,
        scope: SecretScope,
        declared: &'a [Variable],
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.iter()
            .filter(move |(candidate, key, _)| {
                *candidate == scope && !declared.iter().any(|variable| variable.key == *key)
            })
            .map(|(_, key, value)| (key, value))
    }
}

impl Engine {
    /// Persists the variable writes a script made during a window send of the request
    /// `selector` with `options` (variables-editor S5), as Postman does.
    ///
    /// - A write to `pm.environment`, `pm.collectionVariables` or `pm.globals` sets the
    ///   current value of that key in the environment the send used, the request's
    ///   collection or the globals; an unset clears it. A key the scope does not declare
    ///   gets a current value too.
    /// - A write to a variable declared secret in that scope goes to the secret store
    ///   instead and never reaches the app state. An unset of a secret changes nothing: the
    ///   keyring entry stays, so a script cannot delete a stored credential.
    /// - `pm.variables` writes are local to the execution and never persisted, and an
    ///   environment write with no environment selected is dropped.
    ///
    /// Writes apply in order, so a later write to the same key wins. Every write is tried;
    /// the first failure is returned after the rest were applied. Errors name keys, never
    /// values. `values` is updated beside the app state, so the caller's copy stays in step.
    ///
    /// # Errors
    ///
    /// [`PersistError::Execute`] when the request or the environment no longer resolves,
    /// [`PersistError::State`] or [`PersistError::Secret`] for the first failed write.
    pub fn persist_script_writes(
        &self,
        state: &mut AppState,
        values: &mut CurrentValues,
        selector: &str,
        options: &ExecuteOptions,
        writes: &[VariableWrite],
    ) -> Result<(), PersistError> {
        let workspace = self.workspace();
        let target = crate::select::find(workspace, selector)?;
        let environment = select_environment(workspace, target.collection, options)?;
        let workspace_id = workspace.manifest.id;
        let mut first_error = None;
        for write in writes {
            let Some((scope, secret)) = write_scope(workspace, &target, environment, write) else {
                continue;
            };
            let result = if secret {
                write.value.as_deref().map_or(Ok(()), |value| {
                    let key = SecretKey {
                        workspace_id,
                        scope,
                        key: write.key.clone(),
                    };
                    self.secret_store()
                        .set(&key, value)
                        .map_err(|source| PersistError::Secret {
                            account: key.account(),
                            source,
                        })
                })
            } else {
                let id = workspace_id.to_string();
                match &write.value {
                    Some(value) => state
                        .set_current_value(&id, scope, &write.key, value)
                        .map(|()| values.insert(scope, write.key.clone(), value.clone())),
                    None => state.clear_current_value(&id, scope, &write.key).map(|_| {
                        values.remove(scope, &write.key);
                    }),
                }
                .map_err(PersistError::State)
            };
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

/// Where a script `write` made during a send of `target` lands as a current value: the
/// scope, and whether the variable is declared secret there. `None` for a `pm.variables`
/// write, which is local to the execution, and for an environment write with no
/// environment selected.
pub(crate) fn write_scope(
    workspace: &Workspace,
    target: &Target<'_>,
    environment: Option<&EnvironmentNode>,
    write: &VariableWrite,
) -> Option<(SecretScope, bool)> {
    let (scope, declared) = match write.scope {
        VariableScope::Variables => return None,
        VariableScope::Globals => (SecretScope::Global, workspace.manifest.globals.as_slice()),
        VariableScope::CollectionVariables => (
            SecretScope::Collection(target.collection.collection.id),
            target.collection.collection.variables.as_slice(),
        ),
        VariableScope::Environment => {
            let environment = environment?;
            (
                SecretScope::Environment(environment.environment.id),
                environment.environment.variables.as_slice(),
            )
        }
    };
    let secret = declared
        .iter()
        .any(|variable| variable.secret && variable.key == write.key);
    Some((scope, secret))
}
