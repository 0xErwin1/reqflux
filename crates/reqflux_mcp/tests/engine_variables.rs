//! The environment and variable tools (mcp-engine-tools S6, S7), called directly on temp
//! workspaces with an in-memory secret store, never the OS keyring.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqflux_engine::MemorySecretStore;
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, EngineState, EnvironmentArgs, EnvironmentCreateArgs,
    EnvironmentRenameArgs, ScopeArgs, VariableDeleteArgs, VariableSetArgs, WorkspaceCreateArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::{Value, json};
use tempfile::TempDir;

const SECRET: &str = "hunter2-very-secret";

fn ok(result: Result<String, ToolError>) -> Value {
    match result {
        Ok(text) => {
            assert!(
                !text.contains(SECRET),
                "a tool output holds the secret: {text}"
            );
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("not JSON ({error}): {text}"))
        }
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn err(result: Result<String, ToolError>) -> String {
    match result {
        Ok(text) => panic!("expected a tool error, got {text}"),
        Err(error) => {
            let message = error.to_string();
            assert!(
                !message.contains(SECRET),
                "an error holds the secret: {message}"
            );
            message
        }
    }
}

/// A state over `store` with a fresh workspace `Demo` at `<dir>/ws` open; returns the
/// workspace id too.
fn open(dir: &TempDir, store: Arc<MemorySecretStore>) -> (EngineState, PathBuf, String) {
    let mut state = EngineState::new(
        store,
        Some(dir.path().join("state.sqlite3")),
        common::allow_all(dir.path()),
    );
    let root = dir.path().join("ws");
    let created = ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: root.display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    let id = created["id"].as_str().unwrap().to_owned();
    (state, root, id)
}

/// Every file under `root`, concatenated, so a test can assert a value is on no file.
fn all_files(root: &Path) -> String {
    fn walk(dir: &Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
            }
        }
    }
    let mut out = String::new();
    walk(root, &mut out);
    out
}

fn set(scope: &str, key: &str) -> VariableSetArgs {
    VariableSetArgs {
        scope: scope.to_owned(),
        key: key.to_owned(),
        value: None,
        secret: None,
        enabled: None,
        description: None,
    }
}

fn variables(state: &mut EngineState, scope: &str) -> Value {
    ok(engine_tools::variables_get(
        state,
        ScopeArgs {
            scope: scope.to_owned(),
        },
    ))
}

fn variable(state: &mut EngineState, scope: &str, key: &str) -> Value {
    variables(state, scope)["variables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variable| variable["key"] == key)
        .cloned()
        .unwrap_or_else(|| panic!("no variable {key}"))
}

fn environments(state: &mut EngineState) -> Vec<Value> {
    ok(engine_tools::environment_list(state))
        .as_array()
        .unwrap()
        .clone()
}

fn create_environment(state: &mut EngineState, name: Option<&str>) -> String {
    let created = ok(engine_tools::environment_create(
        state,
        EnvironmentCreateArgs {
            name: name.map(ToOwned::to_owned),
        },
    ));
    created["id"].as_str().unwrap().to_owned()
}

#[test]
fn environments_are_listed_created_renamed_and_deleted() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    assert_eq!(environments(&mut state), Vec::<Value>::new());

    let unnamed = create_environment(&mut state, None);
    let staging = create_environment(&mut state, Some("Staging"));
    ok(engine_tools::variable_set(&mut state, {
        let mut args = set(&format!("environment:{staging}"), "host");
        args.value = Some("example.test".to_owned());
        args
    }));
    let listed = environments(&mut state);
    assert_eq!(listed.len(), 2);
    let staging_entry = listed
        .iter()
        .find(|env| env["id"] == staging.as_str())
        .unwrap();
    assert_eq!(
        staging_entry,
        &json!({"id": staging, "name": "Staging", "slug": "staging", "variable_count": 1})
    );
    let unnamed_entry = listed
        .iter()
        .find(|env| env["id"] == unnamed.as_str())
        .unwrap();
    assert_eq!(unnamed_entry["name"], "New environment");
    assert_eq!(unnamed_entry["variable_count"], 0);

    let renamed = ok(engine_tools::environment_rename(
        &mut state,
        EnvironmentRenameArgs {
            id: unnamed.clone(),
            name: "Production".to_owned(),
        },
    ));
    assert_eq!(renamed, json!({"id": unnamed}));
    let listed = environments(&mut state);
    let production = listed
        .iter()
        .find(|env| env["id"] == unnamed.as_str())
        .unwrap();
    assert_eq!(production["name"], "Production");
    assert_eq!(production["slug"], "production");

    let deleted = ok(engine_tools::environment_delete(
        &mut state,
        EnvironmentArgs {
            id: unnamed.clone(),
        },
    ));
    assert_eq!(deleted, json!({"id": unnamed}));
    let listed = environments(&mut state);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], staging.as_str());
}

#[test]
fn a_refused_rename_or_create_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    let staging = create_environment(&mut state, Some("Staging"));
    let other = create_environment(&mut state, Some("Other"));
    let before = environments(&mut state);

    let message = err(engine_tools::environment_rename(
        &mut state,
        EnvironmentRenameArgs {
            id: other.clone(),
            name: "Staging".to_owned(),
        },
    ));
    assert!(!message.is_empty());
    let message = err(engine_tools::environment_create(
        &mut state,
        EnvironmentCreateArgs {
            name: Some("Staging".to_owned()),
        },
    ));
    assert!(!message.is_empty());
    assert_eq!(environments(&mut state), before, "nothing changed");

    let unknown = "01J00000000000000000000000";
    let message = err(engine_tools::environment_delete(
        &mut state,
        EnvironmentArgs {
            id: unknown.to_owned(),
        },
    ));
    assert!(message.contains(unknown), "{message}");
    assert_eq!(environments(&mut state), before);
    assert!(before.iter().any(|env| env["id"] == staging.as_str()));
}

#[test]
fn variables_get_masks_secrets_and_reports_whether_they_are_set() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(MemorySecretStore::new());
    let (mut state, _root, _) = open(&dir, Arc::clone(&store) as _);
    let mut plain = set("global", "host");
    plain.value = Some("example.test".to_owned());
    plain.description = Some("The host".to_owned());
    ok(engine_tools::variable_set(&mut state, plain));
    let mut token = set("global", "token");
    token.value = Some(SECRET.to_owned());
    token.secret = Some(true);
    ok(engine_tools::variable_set(&mut state, token));
    let mut empty = set("global", "unset");
    empty.secret = Some(true);
    ok(engine_tools::variable_set(&mut state, empty));

    let listed = variables(&mut state, "global");
    assert_eq!(
        listed,
        json!({
            "scope": "global",
            "variables": [
                {"key": "host", "value": "example.test", "enabled": true, "secret": false, "description": "The host"},
                {"key": "token", "value": {"set": true}, "enabled": true, "secret": true, "description": null},
                {"key": "unset", "value": {"set": false}, "enabled": true, "secret": true, "description": null},
            ]
        })
    );
}

#[test]
fn an_unknown_keyring_reports_the_reason_once() {
    let dir = TempDir::new().unwrap();
    let (mut state, root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    // Declare two secrets in the file directly: the store is unavailable for this state.
    let manifest = root.join("reqflux.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str(
        "\n[[globals]]\nkey = \"a\"\nsecret = true\n\n[[globals]]\nkey = \"b\"\nsecret = true\n",
    );
    std::fs::write(&manifest, text).unwrap();
    drop(state);
    state = EngineState::new(
        Arc::new(MemorySecretStore::unavailable("no secret service")),
        Some(dir.path().join("state.sqlite3")),
        common::allow_all(dir.path()),
    );
    ok(engine_tools::workspace_open(
        &mut state,
        engine_tools::PathArgs {
            path: root.display().to_string(),
        },
    ));

    let listed = variables(&mut state, "global");
    assert_eq!(listed["variables"][0]["value"], "unknown");
    assert_eq!(listed["variables"][1]["value"], "unknown");
    let reason = listed["keyring_error"].as_str().unwrap();
    assert!(reason.contains("no secret service"), "{reason}");
}

#[test]
fn variable_set_creates_plain_and_secret_variables_keeping_secrets_off_the_files() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(MemorySecretStore::new());
    let (mut state, root, workspace_id) = open(&dir, Arc::clone(&store) as _);
    let collection = ok(engine_tools::collection_create(
        &mut state,
        CollectionCreateArgs {
            name: Some("Api".to_owned()),
        },
    ))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let scope = format!("collection:{collection}");

    let mut plain = set(&scope, "version");
    plain.value = Some("v1".to_owned());
    plain.enabled = Some(false);
    let created = ok(engine_tools::variable_set(&mut state, plain));
    assert_eq!(
        created,
        json!({"scope": scope, "key": "version", "created": true})
    );
    let mut token = set(&scope, "token");
    token.value = Some(SECRET.to_owned());
    token.secret = Some(true);
    ok(engine_tools::variable_set(&mut state, token));

    assert_eq!(
        variable(&mut state, &scope, "version"),
        json!({"key": "version", "value": "v1", "enabled": false, "secret": false, "description": null})
    );
    assert_eq!(
        variable(&mut state, &scope, "token")["value"],
        json!({"set": true})
    );
    assert!(all_files(&root).contains("version"));
    assert!(
        !all_files(&root).contains(SECRET),
        "no file holds the secret"
    );
    assert_eq!(
        store.accounts(),
        vec![format!("{workspace_id}/collection/{collection}/token")]
    );
}

#[test]
fn variable_set_updates_only_the_given_fields() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    let mut host = set("global", "host");
    host.value = Some("example.test".to_owned());
    host.description = Some("The host".to_owned());
    ok(engine_tools::variable_set(&mut state, host));

    let mut disable = set("global", "host");
    disable.enabled = Some(false);
    let updated = ok(engine_tools::variable_set(&mut state, disable));
    assert_eq!(
        updated,
        json!({"scope": "global", "key": "host", "created": false})
    );
    assert_eq!(
        variable(&mut state, "global", "host"),
        json!({"key": "host", "value": "example.test", "enabled": false, "secret": false, "description": "The host"})
    );

    let mut value = set("global", "host");
    value.value = Some("api.test".to_owned());
    ok(engine_tools::variable_set(&mut state, value));
    assert_eq!(
        variable(&mut state, "global", "host"),
        json!({"key": "host", "value": "api.test", "enabled": false, "secret": false, "description": "The host"})
    );
}

#[test]
fn turning_a_plain_variable_secret_moves_its_value_to_the_keyring() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(MemorySecretStore::new());
    let (mut state, root, workspace_id) = open(&dir, Arc::clone(&store) as _);
    let environment = create_environment(&mut state, Some("Staging"));
    let scope = format!("environment:{environment}");
    let mut plain = set(&scope, "token");
    plain.value = Some("old-plain-value".to_owned());
    ok(engine_tools::variable_set(&mut state, plain));

    let mut mark = set(&scope, "token");
    mark.secret = Some(true);
    mark.value = Some(SECRET.to_owned());
    ok(engine_tools::variable_set(&mut state, mark));

    let files = all_files(&root);
    assert!(!files.contains(SECRET), "no file holds the secret");
    assert!(
        !files.contains("old-plain-value"),
        "the plain value left the file"
    );
    assert_eq!(
        variable(&mut state, &scope, "token"),
        json!({"key": "token", "value": {"set": true}, "enabled": true, "secret": true, "description": null})
    );
    assert_eq!(
        store.accounts(),
        vec![format!("{workspace_id}/environment/{environment}/token")]
    );
}

#[test]
fn an_unknown_scope_or_key_is_an_error_naming_it() {
    let dir = TempDir::new().unwrap();
    let (mut state, root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    let before = all_files(&root);
    let unknown = "01J00000000000000000000000";
    for scope in [
        format!("collection:{unknown}"),
        format!("environment:{unknown}"),
    ] {
        let mut args = set(&scope, "host");
        args.value = Some("x".to_owned());
        let message = err(engine_tools::variable_set(&mut state, args));
        assert!(message.contains(unknown), "{message}");
        let message = err(engine_tools::variables_get(
            &mut state,
            ScopeArgs {
                scope: scope.clone(),
            },
        ));
        assert!(message.contains(unknown), "{message}");
    }
    let message = err(engine_tools::variables_get(
        &mut state,
        ScopeArgs {
            scope: "planet".to_owned(),
        },
    ));
    assert!(message.contains("planet"), "{message}");
    let message = err(engine_tools::variable_delete(
        &mut state,
        VariableDeleteArgs {
            scope: "global".to_owned(),
            key: "missing".to_owned(),
        },
    ));
    assert!(message.contains("missing"), "{message}");
    assert_eq!(all_files(&root), before, "nothing changed");
}

#[test]
fn variable_delete_removes_the_variable_and_its_secret() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(MemorySecretStore::new());
    let (mut state, _root, _) = open(&dir, Arc::clone(&store) as _);
    let mut token = set("global", "token");
    token.value = Some(SECRET.to_owned());
    token.secret = Some(true);
    ok(engine_tools::variable_set(&mut state, token));
    let mut host = set("global", "host");
    host.value = Some("example.test".to_owned());
    ok(engine_tools::variable_set(&mut state, host));

    let deleted = ok(engine_tools::variable_delete(
        &mut state,
        VariableDeleteArgs {
            scope: "global".to_owned(),
            key: "token".to_owned(),
        },
    ));
    assert_eq!(deleted, json!({"scope": "global", "key": "token"}));
    let listed = variables(&mut state, "global");
    assert_eq!(listed["variables"].as_array().unwrap().len(), 1);
    assert_eq!(listed["variables"][0]["key"], "host");
    assert!(store.accounts().is_empty(), "the secret left the keyring");
}

#[test]
fn an_unavailable_keyring_refuses_a_secret_without_echoing_it_or_changing_anything() {
    let dir = TempDir::new().unwrap();
    let (mut state, root, _) = open(&dir, Arc::new(MemorySecretStore::new()));
    let mut plain = set("global", "token");
    plain.value = Some("plain-value".to_owned());
    ok(engine_tools::variable_set(&mut state, plain));
    drop(state);
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::unavailable("no secret service")),
        Some(dir.path().join("state.sqlite3")),
        common::allow_all(dir.path()),
    );
    ok(engine_tools::workspace_open(
        &mut state,
        engine_tools::PathArgs {
            path: root.display().to_string(),
        },
    ));
    let before = all_files(&root);

    let mut new = set("global", "api_key");
    new.value = Some(SECRET.to_owned());
    new.secret = Some(true);
    let message = err(engine_tools::variable_set(&mut state, new));
    assert!(message.contains("keyring"), "{message}");

    let mut mark = set("global", "token");
    mark.value = Some(SECRET.to_owned());
    mark.secret = Some(true);
    let message = err(engine_tools::variable_set(&mut state, mark));
    assert!(message.contains("keyring"), "{message}");

    assert_eq!(all_files(&root), before, "nothing changed");
    assert_eq!(
        variable(&mut state, "global", "token"),
        json!({"key": "token", "value": "plain-value", "enabled": true, "secret": false, "description": null})
    );
}
