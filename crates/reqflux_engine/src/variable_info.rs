//! What the variable popover of a front end shows and writes (T12, S10): for a `{{name}}` of a
//! request, the scope it resolves from, the value that wins there, the scope it overrides,
//! and the writes the popover makes.
//!
//! Resolution follows [`prepare`](crate::prepare): the active environment over the request's
//! collection over the globals, and within each scope a current value over the initial
//! value. A disabled variable does not resolve. A secret has no current value and its value
//! stays out of [`VariableInfo`]; [`Engine::reveal_variable`] reads it on request.
//!
//! Errors and logs name keys, never a value.

use std::fmt;

use reqflux_core::model::{Variable, Workspace};
use reqflux_core::template::DynamicVariable;

use crate::ExecuteOptions;
use crate::current::CurrentValues;
use crate::engine::Engine;
use crate::error::{ExecuteError, PersistError, VariableError};
use crate::prepare::{StoredSecrets, select_environment};
use crate::secrets::{SecretError, SecretKey, SecretScope};
use crate::state::AppState;

/// A scope a variable can live in, with the name a front end shows for it: `Globals`, the
/// collection's name or the environment's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableScopeRef {
    /// The scope.
    pub scope: SecretScope,
    /// Its display name.
    pub name: String,
}

/// A variable as one scope declares it. Its `Debug` output leaves out the values.
#[derive(Clone, PartialEq, Eq)]
pub struct ScopedValue {
    /// Where it is declared.
    pub at: VariableScopeRef,
    /// The initial value, from the workspace file; `None` for a secret or a variable with
    /// no value, and for a current value of a key the scope does not declare.
    pub initial: Option<String>,
    /// The current value on this machine, if any; never set for a secret.
    pub current: Option<String>,
    /// Whether the variable is secret: its value lives in the secret store.
    pub secret: bool,
    /// For a secret, whether the secret store holds a value for it.
    pub stored: bool,
    /// Whether the scope declares the key; `false` for a current value only.
    pub declared: bool,
}

impl fmt::Debug for ScopedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScopedValue")
            .field("at", &self.at)
            .field("secret", &self.secret)
            .field("stored", &self.stored)
            .field("declared", &self.declared)
            .finish_non_exhaustive()
    }
}

impl ScopedValue {
    /// The value that wins in this scope: the current value, else the initial value. `None`
    /// for a secret, whose value only [`Engine::reveal_variable`] reads.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.current.as_deref().or(self.initial.as_deref())
    }

    /// Whether the scope gives the variable a value.
    #[must_use]
    pub fn resolves(&self) -> bool {
        !self.secret || self.stored
    }
}

/// An environment that declares a variable, which the window can make active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentChoice {
    /// The environment slug, as the active environment is named.
    pub slug: String,
    /// The environment scope and name.
    pub at: VariableScopeRef,
}

/// A `{{name}}` of a request against the active environment, as the popover shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableInfo {
    /// The trimmed name.
    pub name: String,
    /// The scope the name resolves from. When no scope gives it a value, the most specific
    /// scope that declares it, so a missing secret can still be set; `None` when no scope
    /// declares it.
    pub winner: Option<ScopedValue>,
    /// The next scope down that also resolves the name, which the winner overrides.
    pub overrides: Option<ScopedValue>,
    /// Whether the name is a dynamic variable such as `$guid`, which no scope holds.
    pub dynamic: bool,
    /// With no active environment, the environments the collection allows that declare the
    /// name, sorted by name.
    pub environments: Vec<EnvironmentChoice>,
    /// The scopes a quick add can create the variable in: the active environment, if any,
    /// the request's collection and the globals.
    pub targets: Vec<VariableScopeRef>,
}

impl VariableInfo {
    /// Whether the name resolves: a scope gives it a value, or it is dynamic.
    #[must_use]
    pub fn resolves(&self) -> bool {
        self.dynamic || self.winner.as_ref().is_some_and(ScopedValue::resolves)
    }
}

impl Engine {
    /// What the popover shows about `name` in the request `selector`, with the environment
    /// slug `environment` and the `current` values. An environment that is unknown or that
    /// the collection does not allow counts as none, as in an inspection.
    ///
    /// # Errors
    ///
    /// No request has this id or slug path.
    pub fn variable_info(
        &self,
        selector: &str,
        environment: Option<&str>,
        current: &CurrentValues,
        name: &str,
    ) -> Result<VariableInfo, ExecuteError> {
        let workspace = self.workspace();
        let target = crate::select::find(workspace, selector)?;
        let collection = target.collection;
        let options = ExecuteOptions {
            environment: environment.map(str::to_owned),
            ..ExecuteOptions::default()
        };
        let selected = select_environment(workspace, collection, &options)
            .ok()
            .flatten();
        let stored = StoredSecrets::read(
            self.secret_store().as_ref(),
            workspace,
            collection,
            selected,
        );

        let globals = VariableScopeRef {
            scope: SecretScope::Global,
            name: "Globals".to_owned(),
        };
        let collection_ref = VariableScopeRef {
            scope: SecretScope::Collection(collection.collection.id),
            name: collection.collection.name.clone(),
        };
        let environment_ref = selected.map(|node| VariableScopeRef {
            scope: SecretScope::Environment(node.environment.id),
            name: node.environment.name.clone(),
        });
        let mut chain = Vec::new();
        if let Some(at) = &environment_ref
            && let Some(node) = selected
        {
            chain.push((at.clone(), node.environment.variables.as_slice()));
        }
        chain.push((
            collection_ref.clone(),
            collection.collection.variables.as_slice(),
        ));
        chain.push((globals.clone(), workspace.manifest.globals.as_slice()));

        let found: Vec<ScopedValue> = chain
            .into_iter()
            .filter_map(|(at, variables)| scoped(at, variables, name, current, &stored))
            .collect();
        let mut resolving = found.iter().filter(|value| value.resolves());
        let (winner, overrides) = match resolving.next() {
            Some(winner) => (Some(winner.clone()), resolving.next().cloned()),
            None => (found.first().cloned(), None),
        };

        let environments = if selected.is_none() {
            environments_declaring(workspace, collection, name)
        } else {
            Vec::new()
        };
        let targets = environment_ref
            .into_iter()
            .chain([collection_ref, globals])
            .collect();
        Ok(VariableInfo {
            name: name.to_owned(),
            winner,
            overrides,
            dynamic: DynamicVariable::parse(name).is_some(),
            environments,
            targets,
        })
    }

    /// The value of the secret `key` declared in `scope`, read from the secret store; `None`
    /// when it holds none. Only an explicit reveal in a front end calls this.
    ///
    /// # Errors
    ///
    /// The secret store failed.
    pub fn reveal_variable(
        &self,
        scope: SecretScope,
        key: &str,
    ) -> Result<Option<String>, SecretError> {
        self.secret_store().get(&self.popover_key(scope, key))
    }

    /// Sets the value of `key` in `scope` as the popover saves it: a secret's value goes to
    /// the secret store; any other value becomes the current value, in `state` when there is
    /// one and in `current` beside it. The workspace file is never written.
    ///
    /// # Errors
    ///
    /// [`PersistError::Secret`] or [`PersistError::State`] for the refused write.
    pub fn set_variable_value(
        &self,
        state: Option<&mut AppState>,
        current: &mut CurrentValues,
        scope: SecretScope,
        key: &str,
        value: &str,
    ) -> Result<(), PersistError> {
        if self.declares_secret(scope, key) {
            let secret = self.popover_key(scope, key);
            return self.secret_store().set(&secret, value).map_err(|source| {
                PersistError::Secret {
                    account: secret.account(),
                    source,
                }
            });
        }
        if let Some(state) = state {
            let workspace_id = self.workspace().manifest.id.to_string();
            state.set_current_value(&workspace_id, scope, key, value)?;
        }
        current.insert(scope, key, value);
        Ok(())
    }

    /// Clears the current value of `key` in `scope`, so its initial value resolves again;
    /// reports whether there was one.
    ///
    /// # Errors
    ///
    /// [`PersistError::State`] when the app state refused the write.
    pub fn reset_variable_value(
        &self,
        state: Option<&mut AppState>,
        current: &mut CurrentValues,
        scope: SecretScope,
        key: &str,
    ) -> Result<bool, PersistError> {
        if let Some(state) = state {
            let workspace_id = self.workspace().manifest.id.to_string();
            state.clear_current_value(&workspace_id, scope, key)?;
        }
        Ok(current.remove(scope, key).is_some())
    }

    /// Adds the enabled variable `key` to `scope` with `value` as its initial value. A
    /// `secret` keeps only the key in the file and the value in the secret store.
    ///
    /// # Errors
    ///
    /// As [`Engine::add_variable`].
    pub fn quick_add_variable(
        &mut self,
        scope: SecretScope,
        key: &str,
        value: &str,
        secret: bool,
    ) -> Result<(), VariableError> {
        let mut variable = Variable::new(key, value);
        variable.secret = secret;
        self.add_variable(scope, variable)
    }

    fn declares_secret(&self, scope: SecretScope, key: &str) -> bool {
        declared(self.workspace(), scope)
            .iter()
            .any(|variable| variable.secret && variable.key == key)
    }

    fn popover_key(&self, scope: SecretScope, key: &str) -> SecretKey {
        SecretKey {
            workspace_id: self.workspace().manifest.id,
            scope,
            key: key.to_owned(),
        }
    }
}

/// `name` as the scope `at` with `variables` gives it: its enabled declaration, else a
/// current value of an undeclared key.
fn scoped(
    at: VariableScopeRef,
    variables: &[Variable],
    name: &str,
    current: &CurrentValues,
    stored: &StoredSecrets,
) -> Option<ScopedValue> {
    let scope = at.scope;
    match variables
        .iter()
        .find(|variable| variable.enabled && variable.key == name)
    {
        Some(variable) if variable.secret => Some(ScopedValue {
            at,
            initial: None,
            current: None,
            secret: true,
            stored: variable.value.is_some() || stored.value(scope, name).is_some(),
            declared: true,
        }),
        Some(variable) => Some(ScopedValue {
            at,
            initial: variable.value.clone(),
            current: current.get(scope, name).map(str::to_owned),
            secret: false,
            stored: false,
            declared: true,
        }),
        None if variables.iter().any(|variable| variable.key == name) => None,
        None => current.get(scope, name).map(|value| ScopedValue {
            at,
            initial: None,
            current: Some(value.to_owned()),
            secret: false,
            stored: false,
            declared: false,
        }),
    }
}

/// The environments `collection` allows whose enabled variables hold `name`, sorted by name.
fn environments_declaring(
    workspace: &Workspace,
    collection: &reqflux_core::model::CollectionNode,
    name: &str,
) -> Vec<EnvironmentChoice> {
    let mut choices: Vec<EnvironmentChoice> = workspace
        .environments
        .iter()
        .filter(|node| {
            collection
                .collection
                .allows_environment(node.environment.id)
        })
        .filter(|node| {
            node.environment
                .variables
                .iter()
                .any(|variable| variable.enabled && variable.key == name)
        })
        .map(|node| EnvironmentChoice {
            slug: node.slug.clone(),
            at: VariableScopeRef {
                scope: SecretScope::Environment(node.environment.id),
                name: node.environment.name.clone(),
            },
        })
        .collect();
    choices.sort_by(|a, b| a.at.name.cmp(&b.at.name));
    choices
}

/// The variables `scope` declares in `workspace`; empty for an unknown scope.
fn declared(workspace: &Workspace, scope: SecretScope) -> &[Variable] {
    match scope {
        SecretScope::Global => &workspace.manifest.globals,
        SecretScope::Collection(id) => workspace
            .collections
            .iter()
            .find(|node| node.collection.id == id)
            .map_or(&[], |node| node.collection.variables.as_slice()),
        SecretScope::Environment(id) => workspace
            .environments
            .iter()
            .find(|node| node.environment.id == id)
            .map_or(&[], |node| node.environment.variables.as_slice()),
    }
}
