//! Current values (variables-editor S4, S5): the app state v3 table, resolution with shared <
//! current within each scope, and script writes persisted as current values. Every test uses
//! the in-memory secret store and a temporary state file.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{id, id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::state::{AppState, Draft, StateError};
use reqflux_engine::{
    CurrentValues, Engine, ExecuteOptions, MemorySecretStore, SecretKey, SecretScope, SecretStore,
    VariableScope, VariableWrite,
};
use tempfile::TempDir;

const WORKSPACE: u32 = 900;
const COLLECTION: u32 = 1;
const ENVIRONMENT: u32 = 3;
const WS: &str = "ws";

fn state_path(dir: &TempDir) -> PathBuf {
    dir.path().join("state.sqlite3")
}

fn open_state(path: &Path) -> AppState {
    match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open: {error}"),
    }
}

fn user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// Every row of the `current_values` table as `(workspace, scope, key, value)`.
fn rows(path: &Path) -> Vec<(String, String, String, String)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT workspace_id, scope, key, value FROM current_values
             ORDER BY workspace_id, scope, key",
        )
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

const V1: &str = "CREATE TABLE drafts (
     workspace_id TEXT NOT NULL,
     entity_id TEXT NOT NULL,
     payload TEXT NOT NULL,
     base_hash TEXT,
     updated_at INTEGER NOT NULL,
     PRIMARY KEY (workspace_id, entity_id)
 );
 INSERT INTO drafts VALUES ('ws', 'req', 'a = 1', 'hash-1', 7);";

/// The history table exactly as schema v2 wrote it, with one entry.
const V2_HISTORY: &str = "CREATE TABLE history (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     workspace_id TEXT NOT NULL,
     sent_at INTEGER NOT NULL,
     request_id TEXT NOT NULL,
     request_name TEXT NOT NULL,
     request_path TEXT NOT NULL,
     document TEXT NOT NULL,
     method TEXT NOT NULL,
     url TEXT NOT NULL,
     request_headers TEXT NOT NULL,
     request_body_kind TEXT NOT NULL,
     request_body TEXT,
     status INTEGER,
     reason TEXT,
     response_headers TEXT,
     response_body BLOB,
     truncated INTEGER NOT NULL DEFAULT 0,
     size INTEGER,
     elapsed_ms INTEGER NOT NULL,
     error TEXT
 );
 CREATE INDEX history_workspace ON history (workspace_id, id);
 INSERT INTO history (workspace_id, sent_at, request_id, request_name, request_path, document,
     method, url, request_headers, request_body_kind, elapsed_ms, error)
 VALUES ('ws', 5, 'req', 'R', 'api/r', 'name = \"R\"', 'GET', 'https://example.test/', '[]',
     'none', 3, 'refused');";

fn write_file(path: &Path, script: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(script).unwrap();
}

fn assert_draft_kept(state: &AppState) {
    let draft: Draft = state.draft("ws", "req").unwrap().unwrap();
    assert_eq!(draft.payload, "a = 1");
    assert_eq!(draft.base_hash.as_deref(), Some("hash-1"));
    assert_eq!(draft.updated_at, 7);
}

#[test]
fn s4_a_v2_file_upgrades_to_v3_keeping_drafts_and_history() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, &format!("{V1}{V2_HISTORY} PRAGMA user_version = 2;"));

    let mut state = open_state(&path);

    assert_eq!(user_version(&path), 5);
    assert_draft_kept(&state);
    let history = state.history("ws", 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].record.sent.url, "https://example.test/");
    state
        .set_current_value(WS, SecretScope::Global, "host", "local")
        .unwrap();
    assert_eq!(
        state
            .current_values(WS)
            .unwrap()
            .get(SecretScope::Global, "host"),
        Some("local")
    );
}

#[test]
fn s4_a_v1_file_upgrades_through_v2_to_v3() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, &format!("{V1} PRAGMA user_version = 1;"));

    let mut state = open_state(&path);

    assert_eq!(user_version(&path), 5);
    assert_draft_kept(&state);
    assert!(state.history("ws", 10).unwrap().is_empty());
    state
        .set_current_value(WS, SecretScope::Global, "host", "local")
        .unwrap();
    assert_eq!(rows(&path).len(), 1);
}

#[test]
fn s4_a_file_newer_than_v3_is_refused() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, "PRAGMA user_version = 6;");
    match AppState::open(&path) {
        Err(StateError::UnsupportedVersion { version, .. }) => assert_eq!(version, 6),
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn s4_set_get_overwrite_and_clear_current_values() {
    let dir = temp_dir();
    let path = state_path(&dir);
    let mut state = open_state(&path);
    let collection = SecretScope::Collection(id(COLLECTION));
    let environment = SecretScope::Environment(id(ENVIRONMENT));

    assert!(state.current_values(WS).unwrap().is_empty());
    state
        .set_current_value(WS, SecretScope::Global, "host", "a")
        .unwrap();
    state
        .set_current_value(WS, SecretScope::Global, "host", "b")
        .unwrap();
    state
        .set_current_value(WS, collection, "host", "c")
        .unwrap();
    state
        .set_current_value(WS, environment, "token", "d")
        .unwrap();
    state
        .set_current_value("other", SecretScope::Global, "host", "e")
        .unwrap();

    let values = state.current_values(WS).unwrap();
    let listed: Vec<_> = values.iter().collect();
    assert_eq!(
        listed,
        vec![
            (SecretScope::Global, "host", "b"),
            (collection, "host", "c"),
            (environment, "token", "d"),
        ]
    );
    // The scope column is the keyring account form.
    let scopes: Vec<String> = rows(&path)
        .into_iter()
        .filter(|row| row.0 == WS)
        .map(|row| row.1)
        .collect();
    assert_eq!(
        scopes,
        vec![
            format!("collection/{}", id_text(COLLECTION)),
            format!("environment/{}", id_text(ENVIRONMENT)),
            "global".to_owned(),
        ]
    );

    assert!(
        state
            .clear_current_value(WS, SecretScope::Global, "host")
            .unwrap()
    );
    assert!(
        !state
            .clear_current_value(WS, SecretScope::Global, "host")
            .unwrap()
    );
    assert_eq!(
        state
            .current_values(WS)
            .unwrap()
            .get(SecretScope::Global, "host"),
        None
    );

    assert_eq!(state.clear_workspace_current_values(WS).unwrap(), 2);
    assert!(state.current_values(WS).unwrap().is_empty());
    assert_eq!(
        state
            .current_values("other")
            .unwrap()
            .get(SecretScope::Global, "host"),
        Some("e")
    );
}

#[test]
fn s4_current_values_survive_a_reopen() {
    let dir = temp_dir();
    let path = state_path(&dir);
    open_state(&path)
        .set_current_value(WS, SecretScope::Global, "host", "kept")
        .unwrap();
    assert_eq!(
        open_state(&path)
            .current_values(WS)
            .unwrap()
            .get(SecretScope::Global, "host"),
        Some("kept")
    );
}

#[test]
fn s4_debug_names_keys_but_not_values() {
    let mut values = CurrentValues::new();
    values.insert(SecretScope::Global, "token", "very-private-value");
    let printed = format!("{values:?}");
    assert!(printed.contains("token"), "{printed}");
    assert!(!printed.contains("very-private-value"), "{printed}");
}

// --- Resolution -------------------------------------------------------------------------

/// A workspace whose globals, collection `api` and environment `dev` each declare `g`, `c`
/// and `e` with the shared value `shared-<scope>`, a secret `token` in the environment, and
/// the request `api/r` whose URL references all of them plus `x`.
fn workspace() -> TempDir {
    let dir = temp_dir();
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"g\"\nvalue = \"shared-g\"\n",
            id_text(WORKSPACE)
        ),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\n\n[[variables]]\nkey = \"c\"\nvalue = \"shared-c\"\n",
            id_text(COLLECTION)
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"e\"\nvalue = \"shared-e\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n",
            id_text(ENVIRONMENT)
        ),
    );
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\n\
             url = \"https://example.test/{{{{g}}}}/{{{{c}}}}/{{{{e}}}}/{{{{token}}}}/{{{{x}}}}\"\n",
            id_text(2)
        ),
    );
    dir
}

fn store_with(entries: &[(SecretScope, &str, &str)]) -> Arc<MemorySecretStore> {
    let store = MemorySecretStore::new();
    for (scope, key, value) in entries {
        store.set(&secret(*scope, key), value).unwrap();
    }
    Arc::new(store)
}

fn secret(scope: SecretScope, key: &str) -> SecretKey {
    SecretKey {
        workspace_id: id(WORKSPACE),
        scope,
        key: key.to_owned(),
    }
}

fn engine(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn dev() -> SecretScope {
    SecretScope::Environment(id(ENVIRONMENT))
}

fn api() -> SecretScope {
    SecretScope::Collection(id(COLLECTION))
}

fn options(current: CurrentValues, vars: &[(&str, &str)]) -> ExecuteOptions {
    ExecuteOptions {
        environment: Some("dev".to_owned()),
        vars: vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        current,
        ..ExecuteOptions::default()
    }
}

fn url(engine: &Engine, options: &ExecuteOptions) -> String {
    match engine.prepare("api/r", options) {
        Ok(prepared) => prepared.request.url,
        Err(error) => panic!("prepare: {error}"),
    }
}

fn current(entries: &[(SecretScope, &str, &str)]) -> CurrentValues {
    let mut values = CurrentValues::new();
    for (scope, key, value) in entries {
        values.insert(*scope, *key, *value);
    }
    values
}

#[test]
fn s4_without_current_values_the_shared_values_resolve() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[(dev(), "token", "kt")]));
    assert_eq!(
        url(&engine, &options(CurrentValues::new(), &[])),
        "https://example.test/shared-g/shared-c/shared-e/kt/{{x}}"
    );
}

#[test]
fn s4_a_current_value_overrides_the_shared_value_in_each_scope() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[(dev(), "token", "kt")]));
    let values = current(&[
        (SecretScope::Global, "g", "cur-g"),
        (api(), "c", "cur-c"),
        (dev(), "e", "cur-e"),
        // An undeclared key resolves too, in its scope.
        (SecretScope::Global, "x", "cur-x"),
    ]);
    assert_eq!(
        url(&engine, &options(values.clone(), &[])),
        "https://example.test/cur-g/cur-c/cur-e/kt/cur-x"
    );

    let inspection = engine
        .inspect_current("api/r", Some("dev"), &values)
        .unwrap();
    assert!(
        inspection.unresolved.is_empty(),
        "{:?}",
        inspection.unresolved
    );
}

#[test]
fn s4_a_current_value_stays_within_its_scope_precedence() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[]));
    // A current value of a global does not beat the environment's shared value.
    let values = current(&[(SecretScope::Global, "e", "cur-global-e")]);
    let resolved = url(&engine, &options(values, &[]));
    assert!(resolved.contains("/shared-e/"), "{resolved}");
}

#[test]
fn s4_local_vars_still_win_over_current_values() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[(dev(), "token", "kt")]));
    let values = current(&[(dev(), "e", "cur-e")]);
    let resolved = url(&engine, &options(values, &[("e", "from-var")]));
    assert!(resolved.contains("/from-var/"), "{resolved}");
}

#[test]
fn s4_a_secret_ignores_a_current_value_and_resolves_from_the_store() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[(dev(), "token", "kt")]));
    let values = current(&[(dev(), "token", "not-the-secret")]);
    let resolved = url(&engine, &options(values, &[]));
    assert!(resolved.contains("/kt/"), "{resolved}");
}

#[test]
fn s4_clearing_a_current_value_falls_back_to_the_shared_value() {
    let dir = workspace();
    let state_dir = temp_dir();
    let path = state_path(&state_dir);
    let mut state = open_state(&path);
    let engine = engine(dir.path(), store_with(&[]));
    let workspace_id = id_text(WORKSPACE);
    state
        .set_current_value(&workspace_id, dev(), "e", "cur-e")
        .unwrap();
    let resolved = url(
        &engine,
        &options(state.current_values(&workspace_id).unwrap(), &[]),
    );
    assert!(resolved.contains("/cur-e/"), "{resolved}");

    state
        .clear_current_value(&workspace_id, dev(), "e")
        .unwrap();
    let resolved = url(
        &engine,
        &options(state.current_values(&workspace_id).unwrap(), &[]),
    );
    assert!(resolved.contains("/shared-e/"), "{resolved}");
}

// --- Persisting script writes ------------------------------------------------------------

fn set(scope: VariableScope, key: &str, value: &str) -> VariableWrite {
    VariableWrite {
        scope,
        key: key.to_owned(),
        value: Some(value.to_owned()),
    }
}

fn unset(scope: VariableScope, key: &str) -> VariableWrite {
    VariableWrite {
        scope,
        key: key.to_owned(),
        value: None,
    }
}

/// Persists `writes` of a send of `api/r` with `dev`, returning the state file path, the
/// current values the caller holds afterwards and the store.
fn persist(
    writes: &[VariableWrite],
    seeded: &[(SecretScope, &str, &str)],
) -> (
    TempDir,
    PathBuf,
    CurrentValues,
    Arc<MemorySecretStore>,
    Engine,
    TempDir,
) {
    let dir = workspace();
    let store = store_with(&[(dev(), "token", "kt")]);
    let engine = engine(dir.path(), Arc::clone(&store));
    let state_dir = temp_dir();
    let path = state_path(&state_dir);
    let mut state = open_state(&path);
    let workspace_id = id_text(WORKSPACE);
    for (scope, key, value) in seeded {
        state
            .set_current_value(&workspace_id, *scope, key, value)
            .unwrap();
    }
    let mut values = state.current_values(&workspace_id).unwrap();
    engine
        .persist_script_writes(
            &mut state,
            &mut values,
            "api/r",
            &options(CurrentValues::new(), &[]),
            writes,
        )
        .unwrap();
    (state_dir, path, values, store, engine, dir)
}

#[test]
fn s5_environment_collection_and_global_writes_become_current_values() {
    let (_state_dir, path, values, _store, engine, _dir) = persist(
        &[
            set(VariableScope::Environment, "e", "w-e"),
            set(VariableScope::CollectionVariables, "c", "w-c"),
            set(VariableScope::Globals, "g", "w-g"),
            set(VariableScope::Globals, "fresh", "w-fresh"),
            set(VariableScope::Globals, "g", "w-g2"),
        ],
        &[],
    );
    let stored = open_state(&path)
        .current_values(&id_text(WORKSPACE))
        .unwrap();
    let listed: Vec<_> = stored.iter().collect();
    assert_eq!(
        listed,
        vec![
            (SecretScope::Global, "fresh", "w-fresh"),
            (SecretScope::Global, "g", "w-g2"),
            (api(), "c", "w-c"),
            (dev(), "e", "w-e"),
        ]
    );
    assert_eq!(values, stored, "the caller's copy follows the state");
    assert_eq!(
        url(&engine, &options(stored, &[])),
        "https://example.test/w-g2/w-c/w-e/kt/{{x}}"
    );
}

#[test]
fn s5_local_writes_are_not_persisted() {
    let (_state_dir, path, values, _store, _engine, _dir) =
        persist(&[set(VariableScope::Variables, "e", "local")], &[]);
    assert!(rows(&path).is_empty());
    assert!(values.is_empty());
}

#[test]
fn s5_a_secret_write_goes_to_the_store_and_never_to_sqlite() {
    let (_state_dir, path, values, store, _engine, _dir) = persist(
        &[set(VariableScope::Environment, "token", "rotated-secret")],
        &[],
    );
    assert_eq!(
        store.get(&secret(dev(), "token")).unwrap().as_deref(),
        Some("rotated-secret")
    );
    assert!(rows(&path).is_empty(), "a secret never gets a row");
    assert!(values.is_empty());
    let bytes = std::fs::read(&path).unwrap();
    let wal = std::fs::read(path.with_extension("sqlite3-wal")).unwrap_or_default();
    for file in [bytes, wal] {
        assert!(
            !file
                .windows("rotated-secret".len())
                .any(|window| window == b"rotated-secret"),
            "the secret value reached the state file"
        );
    }
}

#[test]
fn s5_an_unset_clears_the_current_value_and_leaves_a_secret_in_the_store() {
    let (_state_dir, path, values, store, _engine, _dir) = persist(
        &[
            unset(VariableScope::Environment, "e"),
            unset(VariableScope::Environment, "token"),
        ],
        &[(dev(), "e", "cur-e"), (SecretScope::Global, "g", "cur-g")],
    );
    let listed: Vec<_> = rows(&path).into_iter().map(|row| row.2).collect();
    assert_eq!(listed, vec!["g".to_owned()]);
    assert_eq!(values.get(dev(), "e"), None);
    assert_eq!(values.get(SecretScope::Global, "g"), Some("cur-g"));
    assert_eq!(
        store.get(&secret(dev(), "token")).unwrap().as_deref(),
        Some("kt"),
        "an unset of a secret keeps the keyring entry"
    );
}

#[test]
fn s5_an_environment_write_without_an_environment_is_dropped() {
    let dir = workspace();
    let engine = engine(dir.path(), store_with(&[]));
    let state_dir = temp_dir();
    let path = state_path(&state_dir);
    let mut state = open_state(&path);
    let mut values = CurrentValues::new();
    engine
        .persist_script_writes(
            &mut state,
            &mut values,
            "api/r",
            &ExecuteOptions::default(),
            &[
                set(VariableScope::Environment, "e", "w-e"),
                set(VariableScope::Globals, "g", "w-g"),
            ],
        )
        .unwrap();
    let listed: Vec<_> = rows(&path).into_iter().map(|row| (row.1, row.2)).collect();
    assert_eq!(listed, vec![("global".to_owned(), "g".to_owned())]);
}
