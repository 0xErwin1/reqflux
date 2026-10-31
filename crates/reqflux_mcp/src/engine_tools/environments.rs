//! The environment and variable tools (S6), through the engine's environment and variable
//! API. A secret's value lives in the secret store: no output or error carries it (S7), a
//! listed secret shows only whether a value is set.

use reqflux_core::model::{Id, Variable, Workspace};
use reqflux_engine::{Engine, SecretKey, SecretScope, TreeError, VariableEdit, VariableError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::policy::Scope;
use super::{EngineState, encode, id_result, parse_id};
use crate::tools::ToolError;

/// Arguments of `environment_create`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnvironmentCreateArgs {
    /// The name; `New environment` (numbered when taken) unless passed.
    pub name: Option<String>,
}

/// Arguments of `environment_rename`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnvironmentRenameArgs {
    /// The environment id.
    pub id: String,
    /// The new name.
    pub name: String,
}

/// Arguments of `environment_delete`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnvironmentArgs {
    /// The environment id.
    pub id: String,
}

/// Arguments of `variables_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScopeArgs {
    /// `global`, `collection:<id>` or `environment:<id>`.
    pub scope: String,
}

/// Arguments of `variable_set`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VariableSetArgs {
    /// `global`, `collection:<id>` or `environment:<id>`.
    pub scope: String,
    /// The variable key; created when the scope has no variable with it.
    pub key: String,
    /// The value; a secret's value goes to the OS keyring, never to a file.
    pub value: Option<String>,
    /// Whether the value is kept in the OS keyring instead of the file.
    pub secret: Option<bool>,
    /// Whether the variable resolves; `true` for a new variable unless passed.
    pub enabled: Option<bool>,
    /// The description; an empty one clears it.
    pub description: Option<String>,
}

/// Arguments of `variable_delete`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VariableDeleteArgs {
    /// `global`, `collection:<id>` or `environment:<id>`.
    pub scope: String,
    /// The variable key.
    pub key: String,
}

fn tree_error(error: TreeError) -> ToolError {
    ToolError::Engine(error.to_string())
}

fn variable_error(error: VariableError) -> ToolError {
    ToolError::Engine(error.to_string())
}

/// `environment_list`: every environment with its id, name, slug and variable count.
///
/// # Errors
///
/// No workspace is open, or it no longer loads.
pub fn environment_list(state: &mut EngineState) -> Result<String, ToolError> {
    state.authorize("environment_list", Scope::Open, &[])?;
    let engine = state.reloaded()?;
    let environments: Vec<Value> = engine
        .workspace()
        .environments
        .iter()
        .map(|node| {
            json!({
                "id": node.environment.id.to_string(),
                "name": node.environment.name,
                "slug": node.slug,
                "variable_count": node.environment.variables.len(),
            })
        })
        .collect();
    encode(&Value::Array(environments))
}

/// `environment_create`: a new environment, named `name` when passed.
///
/// # Errors
///
/// The store refused the environment or the name; nothing is left behind.
pub fn environment_create(
    state: &mut EngineState,
    args: EnvironmentCreateArgs,
) -> Result<String, ToolError> {
    state.authorize("environment_create", Scope::Open, &[])?;
    let engine = state.reloaded()?;
    let id = engine.create_environment().map_err(tree_error)?;
    if let Some(name) = &args.name
        && let Err(error) = engine.rename_environment(id, name)
    {
        if let Err(cleanup) = engine.delete_environment(id) {
            log::warn!(
                target: "reqflux_mcp",
                "could not delete environment {id} after a refused create: {cleanup}"
            );
        }
        return Err(tree_error(error));
    }
    id_result(id)
}

/// `environment_rename`: renames the environment `id`, moving its file to the new slug.
///
/// # Errors
///
/// A bad id, or the store refused the name.
pub fn environment_rename(
    state: &mut EngineState,
    args: EnvironmentRenameArgs,
) -> Result<String, ToolError> {
    state.authorize("environment_rename", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    id_result(
        engine
            .rename_environment(id, &args.name)
            .map_err(tree_error)?,
    )
}

/// `environment_delete`: deletes the environment `id`.
///
/// # Errors
///
/// A bad id, or the store refused the delete.
pub fn environment_delete(
    state: &mut EngineState,
    args: EnvironmentArgs,
) -> Result<String, ToolError> {
    state.authorize("environment_delete", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    engine.delete_environment(id).map_err(tree_error)?;
    id_result(id)
}

/// `variables_get`: the variables of `scope`; a secret's value is `{"set": bool}`, or
/// `"unknown"` when the keyring cannot be read, with the reason once as `keyring_error`.
///
/// # Errors
///
/// A bad or unknown scope, or no workspace is open.
pub fn variables_get(state: &mut EngineState, args: ScopeArgs) -> Result<String, ToolError> {
    state.authorize(
        "variables_get",
        Scope::Open,
        &[EngineState::scope_target(&args.scope)],
    )?;
    let scope = parse_scope(&args.scope)?;
    let engine = state.reloaded()?;
    let variables = scope_variables(engine.workspace(), scope, &args.scope)?;
    let workspace_id = engine.workspace().manifest.id;
    let store = engine.secret_store();
    let mut keyring_error = None;
    let listed: Vec<Value> = variables
        .iter()
        .map(|variable| {
            let value = if variable.secret {
                let key = SecretKey {
                    workspace_id,
                    scope,
                    key: variable.key.clone(),
                };
                match store.is_set(&key) {
                    Ok(set) => json!({ "set": set }),
                    Err(error) => {
                        keyring_error.get_or_insert_with(|| error.to_string());
                        json!("unknown")
                    }
                }
            } else {
                json!(variable.value)
            };
            json!({
                "key": variable.key,
                "value": value,
                "enabled": variable.enabled,
                "secret": variable.secret,
                "description": variable.description,
            })
        })
        .collect();
    let mut result = json!({ "scope": args.scope, "variables": listed });
    if let (Some(reason), Value::Object(map)) = (keyring_error, &mut result) {
        map.insert("keyring_error".to_owned(), Value::String(reason));
    }
    encode(&result)
}

/// `variable_set`: creates `key` in `scope` when absent, else applies only the given
/// fields. The secret flag changes before the value, so a value meant for the keyring
/// never reaches the file.
///
/// # Errors
///
/// A bad or unknown scope, or the engine refused an edit (a keyring failure refuses it
/// before the file is touched).
pub fn variable_set(state: &mut EngineState, args: VariableSetArgs) -> Result<String, ToolError> {
    state.authorize(
        "variable_set",
        Scope::Open,
        &[EngineState::scope_target(&args.scope)],
    )?;
    let scope = parse_scope(&args.scope)?;
    let engine = state.reloaded()?;
    let existing = scope_variables(engine.workspace(), scope, &args.scope)?
        .iter()
        .enumerate()
        .find(|(_, variable)| variable.key == args.key)
        .map(|(index, variable)| (index, variable.clone()));
    let description_given = args.description.is_some();
    let description = args.description.filter(|text| !text.is_empty());
    let created = existing.is_none();
    match existing {
        None => {
            let variable = Variable {
                key: args.key.clone(),
                value: args.value,
                description,
                secret: args.secret.unwrap_or(false),
                enabled: args.enabled.unwrap_or(true),
            };
            engine
                .add_variable(scope, variable)
                .map_err(variable_error)?;
        }
        Some((index, old)) => {
            let mut edits = Vec::new();
            if let Some(secret) = args.secret.filter(|secret| *secret != old.secret) {
                edits.push(VariableEdit::Secret(secret));
            }
            if let Some(value) = args.value {
                edits.push(VariableEdit::Value(Some(value)));
            }
            if let Some(enabled) = args.enabled.filter(|enabled| *enabled != old.enabled) {
                edits.push(VariableEdit::Enabled(enabled));
            }
            if description_given && description != old.description {
                edits.push(VariableEdit::Description(description));
            }
            for edit in edits {
                engine
                    .edit_variable(scope, index, edit)
                    .map_err(variable_error)?;
            }
        }
    }
    encode(&json!({ "scope": args.scope, "key": args.key, "created": created }))
}

/// `variable_delete`: deletes `key` from `scope`, and a secret's keyring entry with it.
///
/// # Errors
///
/// A bad or unknown scope or key, or the engine refused the delete.
pub fn variable_delete(
    state: &mut EngineState,
    args: VariableDeleteArgs,
) -> Result<String, ToolError> {
    state.authorize(
        "variable_delete",
        Scope::Open,
        &[EngineState::scope_target(&args.scope)],
    )?;
    let scope = parse_scope(&args.scope)?;
    let engine = state.reloaded()?;
    let index = index_of(engine, scope, &args.scope, &args.key)?;
    engine
        .delete_variable(scope, index)
        .map_err(variable_error)?;
    encode(&json!({ "scope": args.scope, "key": args.key }))
}

/// Parses `global`, `collection:<id>` or `environment:<id>`.
fn parse_scope(text: &str) -> Result<SecretScope, ToolError> {
    if text == "global" {
        return Ok(SecretScope::Global);
    }
    let invalid = || {
        ToolError::Invalid(format!(
            "invalid scope `{text}`: expected global, collection:<id> or environment:<id>"
        ))
    };
    let (kind, id) = text.split_once(':').ok_or_else(invalid)?;
    let id: Id = parse_id(id)?;
    match kind {
        "collection" => Ok(SecretScope::Collection(id)),
        "environment" => Ok(SecretScope::Environment(id)),
        _ => Err(invalid()),
    }
}

/// The variables declared in `scope`; an unknown collection or environment is an error
/// naming `text`.
fn scope_variables<'a>(
    workspace: &'a Workspace,
    scope: SecretScope,
    text: &str,
) -> Result<&'a [Variable], ToolError> {
    let found = match scope {
        SecretScope::Global => Some(workspace.manifest.globals.as_slice()),
        SecretScope::Collection(id) => workspace
            .collections
            .iter()
            .find(|node| node.collection.id == id)
            .map(|node| node.collection.variables.as_slice()),
        SecretScope::Environment(id) => workspace
            .environments
            .iter()
            .find(|node| node.environment.id == id)
            .map(|node| node.environment.variables.as_slice()),
    };
    found.ok_or_else(|| ToolError::Invalid(format!("no scope `{text}` in the workspace")))
}

/// The index of `key` in `scope`; an absent key is an error naming it.
fn index_of(
    engine: &Engine,
    scope: SecretScope,
    text: &str,
    key: &str,
) -> Result<usize, ToolError> {
    scope_variables(engine.workspace(), scope, text)?
        .iter()
        .position(|variable| variable.key == key)
        .ok_or_else(|| ToolError::Invalid(format!("no variable `{key}` in scope `{text}`")))
}
