//! The secret store values one preparation resolves with (keyring-secrets S2, S3).

use std::collections::HashMap;

use reqflux_core::model::{
    Auth, CollectionNode, EnvironmentNode, Item, Variable, Workspace, secret_fields,
};

use crate::secrets::{SecretError, SecretKey, SecretScope, SecretStore};
use crate::sent::Secrets;

/// The stored values of the secret variables that have no value in the workspace files, read
/// once per preparation, and why some could not be read.
#[derive(Debug, Default)]
pub(crate) struct StoredSecrets {
    values: HashMap<(SecretScope, String), String>,
    /// Why secrets were not read from the store, reported once per preparation.
    pub(crate) problem: Option<String>,
}

impl StoredSecrets {
    /// Reads every valueless secret of the globals, `collection` and `environment` from
    /// `store`. A store that is unavailable stops the reads and leaves every secret unset; a
    /// failed read leaves that secret unset. Either is recorded in `problem`, never as an
    /// error, so the send still prepares.
    pub(crate) fn read(
        store: &dyn SecretStore,
        workspace: &Workspace,
        collection: &CollectionNode,
        environment: Option<&EnvironmentNode>,
    ) -> Self {
        let workspace_id = workspace.manifest.id;
        let scopes = [
            (SecretScope::Global, workspace.manifest.globals.as_slice()),
            (
                SecretScope::Collection(collection.collection.id),
                collection.collection.variables.as_slice(),
            ),
        ]
        .into_iter()
        .chain(environment.map(|environment| {
            (
                SecretScope::Environment(environment.environment.id),
                environment.environment.variables.as_slice(),
            )
        }));
        let mut stored = Self::default();
        for (scope, variables) in scopes {
            for variable in variables
                .iter()
                .filter(|variable| is_valueless_secret(variable))
            {
                let key = SecretKey {
                    workspace_id,
                    scope,
                    key: variable.key.clone(),
                };
                match store.get(&key) {
                    Ok(Some(value)) => {
                        stored.values.insert((scope, key.key), value);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let unavailable = matches!(error, SecretError::Unavailable(_));
                        stored.problem.get_or_insert_with(|| {
                            format!("secret variables were not read from the keyring: {error}")
                        });
                        if unavailable {
                            return stored;
                        }
                    }
                }
            }
        }
        stored
    }

    /// The stored value of the secret `key` declared in `scope`, if any.
    pub(crate) fn value(&self, scope: SecretScope, key: &str) -> Option<&str> {
        self.values
            .get(&(scope, key.to_owned()))
            .map(String::as_str)
    }
}

/// Whether `variable` is a secret whose value must come from the store.
fn is_valueless_secret(variable: &Variable) -> bool {
    variable.secret && variable.value.is_none()
}

/// Every secret value of `workspace`: each secret variable of the globals, every collection
/// and every environment, from the file or from `store`, and every literal basic password,
/// bearer token, API key value and field of another auth type on a collection, folder or
/// request. Also why some secret variables could not be read from `store`, if any: the
/// returned values then lack those.
pub(crate) fn workspace_secrets(
    store: &dyn SecretStore,
    workspace: &Workspace,
) -> (Secrets, Option<String>) {
    let workspace_id = workspace.manifest.id;
    let scopes = std::iter::once((SecretScope::Global, workspace.manifest.globals.as_slice()))
        .chain(workspace.collections.iter().map(|collection| {
            (
                SecretScope::Collection(collection.collection.id),
                collection.collection.variables.as_slice(),
            )
        }))
        .chain(workspace.environments.iter().map(|environment| {
            (
                SecretScope::Environment(environment.environment.id),
                environment.environment.variables.as_slice(),
            )
        }));
    let mut secrets = Secrets::default();
    let mut problem = None;
    let mut readable = true;
    for (scope, variables) in scopes {
        for variable in variables.iter().filter(|variable| variable.secret) {
            if let Some(value) = &variable.value {
                secrets.add(value.as_str());
                continue;
            }
            if !readable {
                continue;
            }
            let key = SecretKey {
                workspace_id,
                scope,
                key: variable.key.clone(),
            };
            match store.get(&key) {
                Ok(Some(value)) => secrets.add(value),
                Ok(None) => {}
                Err(error) => {
                    readable = !matches!(error, SecretError::Unavailable(_));
                    problem.get_or_insert_with(|| {
                        format!("secret variables were not read from the keyring: {error}")
                    });
                }
            }
        }
    }
    for collection in &workspace.collections {
        add_auth(&collection.collection.auth, &mut secrets);
        add_items(&collection.items, &mut secrets);
    }
    (secrets, problem)
}

fn add_items(items: &[Item], secrets: &mut Secrets) {
    for item in items {
        match item {
            Item::Folder(folder) => {
                add_auth(&folder.folder.auth, secrets);
                add_items(&folder.items, secrets);
            }
            Item::Request(request) => add_auth(&request.request.auth, secrets),
        }
    }
}

/// Adds the credential fields of `auth`, as written.
fn add_auth(auth: &Auth, secrets: &mut Secrets) {
    match auth {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { password, .. } => secrets.add(password.as_str()),
        Auth::Bearer { token } => secrets.add(token.as_str()),
        Auth::ApiKey { value, .. } => secrets.add(value.as_str()),
        // A typed kind names its credential keys, in both spellings: the region or the
        // algorithm is not a secret, and masking it would hide it in every text.
        Auth::Other { kind, fields } if !secret_fields(kind).is_empty() => {
            for key in secret_fields(kind) {
                if let Some(toml::Value::String(text)) = fields.get(*key) {
                    secrets.add(text.as_str());
                }
            }
        }
        // A type the engine does not read, or one whose fields do not read: every value
        // may be a credential.
        Auth::Other { fields, .. } => {
            for value in fields.values() {
                if let toml::Value::String(text) = value {
                    secrets.add(text.as_str());
                }
            }
        }
    }
}
