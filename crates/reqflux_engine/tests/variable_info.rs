//! What the variable popover shows and writes (T12, S10): the scope a `{{name}}` resolves
//! from, the scope it overrides, secrets masked, and the current-value and quick-add writes.
//! Every test uses the in-memory secret store and a temporary state file.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{id, id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::state::AppState;
use reqflux_engine::{
    CurrentValues, Engine, MemorySecretStore, SecretKey, SecretScope, SecretStore, VariableInfo,
};
use tempfile::TempDir;

const WORKSPACE: u32 = 900;
const COLLECTION: u32 = 1;
const STAGING: u32 = 3;
const PRODUCTION: u32 = 4;
const REQUEST: &str = "api/r";

/// Globals `base_url` and `g`; collection Billing `base_url` and `customer_id`; environment
/// Staging `base_url` and the secret `api_token`; environment Production `prod_only`.
fn workspace_dir() -> TempDir {
    let dir = temp_dir();
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"base_url\"\n\
             value = \"http://global\"\n\n[[globals]]\nkey = \"g\"\nvalue = \"shared-g\"\n",
            id_text(WORKSPACE)
        ),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Billing\"\n\n[[variables]]\nkey = \"base_url\"\n\
             value = \"http://localhost:8080\"\n\n[[variables]]\nkey = \"customer_id\"\n\
             value = \"cus_1000\"\n",
            id_text(COLLECTION)
        ),
    );
    write(
        root,
        "environments/staging.env.toml",
        &format!(
            "id = \"{}\"\nname = \"Staging\"\n\n[[variables]]\nkey = \"base_url\"\n\
             value = \"https://api.staging.acme.dev\"\n\n[[variables]]\nkey = \"api_token\"\n\
             secret = true\n",
            id_text(STAGING)
        ),
    );
    write(
        root,
        "environments/production.env.toml",
        &format!(
            "id = \"{}\"\nname = \"Production\"\n\n[[variables]]\nkey = \"prod_only\"\n\
             value = \"p\"\n",
            id_text(PRODUCTION)
        ),
    );
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\nurl = \"{{{{base_url}}}}/v2\"\n",
            id_text(2)
        ),
    );
    dir
}

fn engine(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn staging() -> SecretScope {
    SecretScope::Environment(id(STAGING))
}

fn billing() -> SecretScope {
    SecretScope::Collection(id(COLLECTION))
}

fn token_key() -> SecretKey {
    SecretKey {
        workspace_id: id(WORKSPACE),
        scope: staging(),
        key: "api_token".to_owned(),
    }
}

fn info(
    engine: &Engine,
    environment: Option<&str>,
    current: &CurrentValues,
    name: &str,
) -> VariableInfo {
    engine
        .variable_info(REQUEST, environment, current, name)
        .unwrap()
}

#[test]
fn s10_the_environment_wins_and_overrides_the_collection() {
    let dir = workspace_dir();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let info = info(&engine, Some("staging"), &CurrentValues::new(), "base_url");

    let winner = info.winner.as_ref().unwrap();
    assert_eq!(winner.at.scope, staging());
    assert_eq!(winner.at.name, "Staging");
    assert_eq!(winner.value(), Some("https://api.staging.acme.dev"));
    assert_eq!(
        winner.initial.as_deref(),
        Some("https://api.staging.acme.dev")
    );
    assert_eq!(winner.current, None);
    assert!(!winner.secret);
    assert!(info.resolves());

    let overrides = info.overrides.as_ref().unwrap();
    assert_eq!(overrides.at.scope, billing());
    assert_eq!(overrides.at.name, "Billing");
    assert_eq!(overrides.value(), Some("http://localhost:8080"));
}

#[test]
fn s10_without_an_environment_the_collection_overrides_the_globals() {
    let dir = workspace_dir();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let info = info(&engine, None, &CurrentValues::new(), "base_url");

    assert_eq!(info.winner.as_ref().unwrap().at.scope, billing());
    let overrides = info.overrides.as_ref().unwrap();
    assert_eq!(overrides.at.scope, SecretScope::Global);
    assert_eq!(overrides.at.name, "Globals");
    let environments: Vec<_> = info
        .environments
        .iter()
        .map(|choice| choice.slug.as_str())
        .collect();
    assert_eq!(environments, vec!["staging"]);
}

#[test]
fn s10_a_current_value_wins_over_the_initial_value_in_its_scope() {
    let dir = workspace_dir();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let mut current = CurrentValues::new();
    current.insert(billing(), "customer_id", "cus_4821");
    let info = info(&engine, Some("staging"), &current, "customer_id");

    let winner = info.winner.as_ref().unwrap();
    assert_eq!(winner.at.scope, billing());
    assert_eq!(winner.value(), Some("cus_4821"));
    assert_eq!(winner.current.as_deref(), Some("cus_4821"));
    assert_eq!(winner.initial.as_deref(), Some("cus_1000"));
    assert!(info.overrides.is_none());
}

#[test]
fn s10_a_secret_is_masked_and_only_a_reveal_reads_it() {
    let dir = workspace_dir();
    let store = Arc::new(MemorySecretStore::new());
    store.set(&token_key(), "tok-123").unwrap();
    let engine = engine(dir.path(), store);
    let info = info(&engine, Some("staging"), &CurrentValues::new(), "api_token");

    let winner = info.winner.as_ref().unwrap();
    assert!(winner.secret);
    assert!(winner.stored);
    assert_eq!(winner.value(), None);
    assert_eq!(winner.initial, None);
    assert!(info.resolves());
    assert!(!format!("{info:?}").contains("tok-123"));
    assert_eq!(
        engine
            .reveal_variable(staging(), "api_token")
            .unwrap()
            .as_deref(),
        Some("tok-123")
    );
}

#[test]
fn s10_an_unknown_name_is_unresolved_with_the_quick_add_scopes() {
    let dir = workspace_dir();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let info = info(&engine, Some("staging"), &CurrentValues::new(), "tenant_id");

    assert!(info.winner.is_none());
    assert!(!info.resolves());
    let targets: Vec<_> = info
        .targets
        .iter()
        .map(|target| (target.scope, target.name.as_str()))
        .collect();
    assert_eq!(
        targets,
        vec![
            (staging(), "Staging"),
            (billing(), "Billing"),
            (SecretScope::Global, "Globals"),
        ]
    );
}

#[test]
fn s10_an_environment_only_name_lists_its_environments_when_none_is_active() {
    let dir = workspace_dir();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let info = info(&engine, None, &CurrentValues::new(), "prod_only");

    assert!(info.winner.is_none());
    let environments: Vec<_> = info
        .environments
        .iter()
        .map(|choice| (choice.slug.as_str(), choice.at.name.as_str()))
        .collect();
    assert_eq!(environments, vec![("production", "Production")]);
    let targets: Vec<_> = info.targets.iter().map(|target| target.scope).collect();
    assert_eq!(targets, vec![billing(), SecretScope::Global]);
}

#[test]
fn s10_setting_a_value_writes_the_current_value_and_leaves_the_file() {
    let dir = workspace_dir();
    let state_dir = temp_dir();
    let mut state = AppState::open(&state_dir.path().join("state.sqlite3")).unwrap();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let before = std::fs::read(dir.path().join("environments/staging.env.toml")).unwrap();
    let mut current = CurrentValues::new();

    engine
        .set_variable_value(
            Some(&mut state),
            &mut current,
            staging(),
            "base_url",
            "https://x",
        )
        .unwrap();

    assert_eq!(current.get(staging(), "base_url"), Some("https://x"));
    let stored = state.current_values(&id_text(WORKSPACE)).unwrap();
    assert_eq!(stored.get(staging(), "base_url"), Some("https://x"));
    let after = std::fs::read(dir.path().join("environments/staging.env.toml")).unwrap();
    assert_eq!(before, after);
    let info = info(&engine, Some("staging"), &current, "base_url");
    assert_eq!(info.winner.unwrap().value(), Some("https://x"));
}

#[test]
fn s10_resetting_clears_the_current_value() {
    let dir = workspace_dir();
    let state_dir = temp_dir();
    let mut state = AppState::open(&state_dir.path().join("state.sqlite3")).unwrap();
    let engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));
    let mut current = CurrentValues::new();
    engine
        .set_variable_value(
            Some(&mut state),
            &mut current,
            billing(),
            "customer_id",
            "c",
        )
        .unwrap();

    assert!(
        engine
            .reset_variable_value(Some(&mut state), &mut current, billing(), "customer_id")
            .unwrap()
    );

    assert_eq!(current.get(billing(), "customer_id"), None);
    assert!(
        state
            .current_values(&id_text(WORKSPACE))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn s10_setting_a_secret_writes_the_keyring_and_no_current_value() {
    let dir = workspace_dir();
    let state_dir = temp_dir();
    let mut state = AppState::open(&state_dir.path().join("state.sqlite3")).unwrap();
    let store = Arc::new(MemorySecretStore::new());
    let engine = engine(dir.path(), store.clone());
    let mut current = CurrentValues::new();

    engine
        .set_variable_value(
            Some(&mut state),
            &mut current,
            staging(),
            "api_token",
            "s3cret",
        )
        .unwrap();

    assert_eq!(store.get(&token_key()).unwrap().as_deref(), Some("s3cret"));
    assert!(current.is_empty());
    assert!(
        state
            .current_values(&id_text(WORKSPACE))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn s10_quick_add_stores_the_value_as_the_initial_value() {
    let dir = workspace_dir();
    let mut engine = engine(dir.path(), Arc::new(MemorySecretStore::new()));

    engine
        .quick_add_variable(billing(), "tenant_id", "tnt_7f2a", false)
        .unwrap();

    let file = std::fs::read_to_string(dir.path().join("collections/api/collection.toml")).unwrap();
    assert!(file.contains("key = \"tenant_id\""));
    assert!(file.contains("value = \"tnt_7f2a\""));
    let info = info(&engine, Some("staging"), &CurrentValues::new(), "tenant_id");
    let winner = info.winner.unwrap();
    assert_eq!(winner.at.scope, billing());
    assert_eq!(winner.initial.as_deref(), Some("tnt_7f2a"));
}

#[test]
fn s10_quick_add_of_a_secret_keeps_only_the_name_in_the_file() {
    let dir = workspace_dir();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = engine(dir.path(), store.clone());

    engine
        .quick_add_variable(SecretScope::Global, "pass", "hunter2", true)
        .unwrap();

    let file = std::fs::read_to_string(dir.path().join("reqflux.toml")).unwrap();
    assert!(file.contains("key = \"pass\""));
    assert!(!file.contains("hunter2"));
    let key = SecretKey {
        workspace_id: id(WORKSPACE),
        scope: SecretScope::Global,
        key: "pass".to_owned(),
    };
    assert_eq!(store.get(&key).unwrap().as_deref(), Some("hunter2"));
}
