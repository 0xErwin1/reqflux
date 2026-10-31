//! Scope variable edits, secret moves and environments through the [`Engine`]
//! (variables-editor S1-S3). Every test uses the in-memory secret store.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use common::{id, id_text, snapshot, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::Variable;
use reqflux_engine::{
    Engine, MemorySecretStore, SecretKey, SecretScope, SecretStore, VariableEdit, VariableError,
};
use reqflux_store::StoreError;

const ENV: &str = "environments/dev.env.toml";

fn workspace(root: &Path) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", id_text(1)),
    );
    write(
        root,
        ENV,
        &format!(
            "id = \"{}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"abc\"\n",
            id_text(3)
        ),
    );
}

fn open(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn read(root: &Path, relative: &str) -> String {
    match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("read {relative}: {error}"),
    }
}

fn ok<T>(result: Result<T, VariableError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("variable edit: {error}"),
    }
}

fn key(scope: SecretScope, name: &str) -> SecretKey {
    SecretKey {
        workspace_id: id(900),
        scope,
        key: name.to_owned(),
    }
}

fn env_scope() -> SecretScope {
    SecretScope::Environment(id(3))
}

fn get(store: &MemorySecretStore, name: &str) -> Option<String> {
    match store.get(&key(env_scope(), name)) {
        Ok(value) => value,
        Err(error) => panic!("get: {error}"),
    }
}

#[test]
fn add_edit_and_delete_reload_the_workspace() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    ok(engine.add_variable(SecretScope::Global, Variable::new("host", "a.test")));
    ok(engine.add_variable(SecretScope::Collection(id(1)), Variable::new("v", "1")));
    assert_eq!(engine.workspace().manifest.globals[0].key, "host");
    assert_eq!(
        engine.workspace().collections[0].collection.variables[0].key,
        "v"
    );
    ok(engine.edit_variable(
        SecretScope::Global,
        0,
        VariableEdit::Value(Some("b.test".to_owned())),
    ));
    ok(engine.edit_variable(
        SecretScope::Global,
        0,
        VariableEdit::Description(Some("H".to_owned())),
    ));
    ok(engine.edit_variable(SecretScope::Global, 0, VariableEdit::Enabled(false)));
    ok(engine.edit_variable(SecretScope::Global, 0, VariableEdit::Key("base".to_owned())));
    assert_eq!(
        read(root, "reqflux.toml"),
        format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"base\"\nvalue = \"b.test\"\ndescription = \"H\"\nenabled = false\n",
            id_text(900)
        )
    );
    ok(engine.delete_variable(SecretScope::Global, 0));
    assert!(engine.workspace().manifest.globals.is_empty());
    assert_eq!(
        read(root, "reqflux.toml"),
        format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900))
    );
}

#[test]
fn an_empty_or_duplicate_key_changes_nothing() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    ok(engine.add_variable(env_scope(), Variable::new("other", "x")));
    let before = snapshot(root);
    let duplicate = engine.add_variable(env_scope(), Variable::new("token", "y"));
    assert!(
        matches!(
            duplicate,
            Err(VariableError::Store(
                StoreError::DuplicateVariableKey { .. }
            ))
        ),
        "{duplicate:?}"
    );
    let renamed = engine.edit_variable(env_scope(), 1, VariableEdit::Key("token".to_owned()));
    assert!(
        matches!(
            renamed,
            Err(VariableError::Store(
                StoreError::DuplicateVariableKey { .. }
            ))
        ),
        "{renamed:?}"
    );
    let empty = engine.edit_variable(env_scope(), 0, VariableEdit::Key(String::new()));
    assert!(
        matches!(
            empty,
            Err(VariableError::Store(StoreError::EmptyVariableKey))
        ),
        "{empty:?}"
    );
    assert_eq!(snapshot(root), before);
    assert!(store.accounts().is_empty());
}

#[test]
fn marking_secret_moves_the_value_into_the_store() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    ok(engine.edit_variable(env_scope(), 0, VariableEdit::Secret(true)));
    assert_eq!(
        store.accounts(),
        [format!("{}/environment/{}/token", id_text(900), id_text(3))]
    );
    assert_eq!(get(&store, "token").as_deref(), Some("abc"));
    let text = read(root, ENV);
    assert!(!text.contains("abc"), "{text}");
    assert!(text.ends_with("key = \"token\"\nsecret = true\n"), "{text}");
    assert!(engine.workspace().environments[0].environment.variables[0].secret);
}

#[test]
fn an_unavailable_store_refuses_mark_secret_and_changes_nothing() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::unavailable("locked")));
    let before = snapshot(root);
    let refused = engine.edit_variable(env_scope(), 0, VariableEdit::Secret(true));
    match refused {
        Err(error @ VariableError::Secret(_)) => {
            assert!(!error.to_string().contains("abc"), "{error}");
        }
        other => panic!("expected a secret error, got {other:?}"),
    }
    assert_eq!(snapshot(root), before);
    assert!(!engine.workspace().environments[0].environment.variables[0].secret);
}

#[test]
fn adding_a_secret_with_a_value_stores_it_and_writes_none() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    let mut secret = Variable::new("pw", "hunter2");
    secret.secret = true;
    ok(engine.add_variable(env_scope(), secret));
    assert_eq!(get(&store, "pw").as_deref(), Some("hunter2"));
    assert!(!read(root, ENV).contains("hunter2"));
}

/// The engine with `token` already secret, holding `abc` in the store.
fn secret_engine(root: &Path, store: &Arc<MemorySecretStore>) -> Engine {
    workspace(root);
    let mut engine = open(root, store.clone());
    ok(engine.edit_variable(env_scope(), 0, VariableEdit::Secret(true)));
    engine
}

#[test]
fn setting_a_secret_value_writes_the_store_only() {
    let dir = temp_dir();
    let root = dir.path();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = secret_engine(root, &store);
    let before = snapshot(root);
    ok(engine.edit_variable(env_scope(), 0, VariableEdit::Value(Some("new".to_owned()))));
    assert_eq!(get(&store, "token").as_deref(), Some("new"));
    assert_eq!(snapshot(root), before);
}

#[test]
fn unmarking_leaves_an_empty_plain_value_and_removes_the_entry() {
    let dir = temp_dir();
    let root = dir.path();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = secret_engine(root, &store);
    ok(engine.edit_variable(env_scope(), 0, VariableEdit::Secret(false)));
    assert!(store.accounts().is_empty());
    let text = read(root, ENV);
    assert!(text.ends_with("[[variables]]\nkey = \"token\"\n"), "{text}");
    let variable = &engine.workspace().environments[0].environment.variables[0];
    assert!(!variable.secret);
    assert_eq!(variable.value, None);
}

#[test]
fn renaming_a_secret_key_moves_its_entry() {
    let dir = temp_dir();
    let root = dir.path();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = secret_engine(root, &store);
    ok(engine.edit_variable(env_scope(), 0, VariableEdit::Key("bearer".to_owned())));
    assert_eq!(
        store.accounts(),
        [format!(
            "{}/environment/{}/bearer",
            id_text(900),
            id_text(3)
        )]
    );
    assert_eq!(get(&store, "bearer").as_deref(), Some("abc"));
    assert!(read(root, ENV).contains("key = \"bearer\""));
}

#[test]
fn deleting_a_secret_removes_its_entry() {
    let dir = temp_dir();
    let root = dir.path();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = secret_engine(root, &store);
    ok(engine.delete_variable(env_scope(), 0));
    assert!(store.accounts().is_empty());
    assert!(!read(root, ENV).contains("token"));
}

#[test]
fn a_file_changed_on_disk_is_a_conflict_and_keeps_the_store() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    let changed = format!("id = \"{}\"\nname = \"edited\"\n", id_text(3));
    write(root, ENV, &changed);
    let conflict = engine.edit_variable(env_scope(), 0, VariableEdit::Secret(true));
    assert!(
        matches!(
            conflict,
            Err(VariableError::Store(StoreError::Conflict { .. }))
        ),
        "{conflict:?}"
    );
    assert_eq!(read(root, ENV), changed);
    assert!(store.accounts().is_empty());
}

#[test]
fn environments_are_created_renamed_and_deleted() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = secret_engine(root, &store);
    let created = match engine.create_environment() {
        Ok(id) => id,
        Err(error) => panic!("create: {error}"),
    };
    let names = |engine: &Engine| -> Vec<String> {
        let mut names: Vec<String> = engine
            .workspace()
            .environments
            .iter()
            .map(|node| format!("{}:{}", node.slug, node.environment.name))
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        names(&engine),
        ["dev:dev", "new-environment:New environment"]
    );
    let clash = engine.rename_environment(created, "Dev");
    assert!(clash.is_err(), "{clash:?}");
    match engine.rename_environment(id(3), "Staging") {
        Ok(renamed) => assert_eq!(renamed, id(3)),
        Err(error) => panic!("rename: {error}"),
    }
    assert_eq!(
        names(&engine),
        ["new-environment:New environment", "staging:Staging"]
    );
    if let Err(error) = engine.delete_environment(id(3)) {
        panic!("delete: {error}");
    }
    assert!(!root.join("environments/staging.env.toml").exists());
    assert_eq!(engine.workspace().environments[0].environment.id, created);
    assert_eq!(store.accounts().len(), 1, "the keyring entry stays");
}

#[test]
fn declaring_a_key_writes_it_with_no_value_and_refuses_a_taken_one() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    ok(engine.declare_variable(env_scope(), "customer_id"));
    assert_eq!(
        read(root, ENV),
        format!(
            "id = \"{}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"abc\"\n\n[[variables]]\nkey = \"customer_id\"\n",
            id_text(3)
        )
    );
    let declared = &engine.workspace().environments[0].environment.variables[1];
    assert_eq!(declared.key, "customer_id");
    assert_eq!(declared.value, None);
    assert!(declared.enabled);
    assert!(!declared.secret);

    let before = snapshot(root);
    let taken = engine.declare_variable(env_scope(), "token");
    assert!(
        matches!(
            taken,
            Err(VariableError::Store(
                StoreError::DuplicateVariableKey { .. }
            ))
        ),
        "{taken:?}"
    );
    assert_eq!(snapshot(root), before);
    assert!(store.accounts().is_empty());
}
