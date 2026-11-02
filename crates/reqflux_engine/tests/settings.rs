//! Settings (light-theme S4): the app state v5 `settings` table, its migration from v4 and
//! the key/value get and set. Every test uses a temporary state file.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::{Path, PathBuf};

use common::temp_dir;
use pretty_assertions::assert_eq;
use reqflux_engine::SecretScope;
use reqflux_engine::state::AppState;
use tempfile::TempDir;

fn state_path(dir: &TempDir) -> PathBuf {
    dir.path().join("state.sqlite3")
}

fn user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn write_file(path: &Path, script: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(script).unwrap();
}

/// Schema v4 exactly as that build wrote it, with one draft, one history entry, one current
/// value and one recent workspace.
const V4: &str = "CREATE TABLE drafts (
     workspace_id TEXT NOT NULL,
     entity_id TEXT NOT NULL,
     payload TEXT NOT NULL,
     base_hash TEXT,
     updated_at INTEGER NOT NULL,
     PRIMARY KEY (workspace_id, entity_id)
 );
 INSERT INTO drafts VALUES ('ws', 'req', 'a = 1', 'hash-1', 7);
 CREATE TABLE history (
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
     'none', 3, 'refused');
 CREATE TABLE current_values (
     workspace_id TEXT NOT NULL,
     scope TEXT NOT NULL,
     key TEXT NOT NULL,
     value TEXT NOT NULL,
     PRIMARY KEY (workspace_id, scope, key)
 );
 INSERT INTO current_values VALUES ('ws', 'global', 'host', 'local');
  CREATE TABLE recent_workspaces (
     path TEXT PRIMARY KEY,
     workspace_id TEXT NOT NULL,
     name TEXT NOT NULL,
     opened_at INTEGER NOT NULL
 );
 INSERT INTO recent_workspaces VALUES ('/w/a', 'ws', 'A', 9);
 PRAGMA user_version = 4;";

#[test]
fn s4_a_v4_file_upgrades_to_v5_keeping_drafts_history_current_values_and_recents() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, V4);

    let mut state = AppState::open(&path).unwrap();

    assert_eq!(user_version(&path), 7);
    let draft = state.draft("ws", "req").unwrap().unwrap();
    assert_eq!(draft.payload, "a = 1");
    assert_eq!(draft.base_hash.as_deref(), Some("hash-1"));
    assert_eq!(draft.updated_at, 7);
    let history = state.history("ws", 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].record.sent.url, "https://example.test/");
    assert_eq!(
        state
            .current_values("ws")
            .unwrap()
            .get(SecretScope::Global, "host"),
        Some("local")
    );
    let recents = state.recent_workspaces().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].path, PathBuf::from("/w/a"));
    assert_eq!(state.setting("theme").unwrap(), None);
    state.set_setting("theme", "light").unwrap();
    assert_eq!(state.setting("theme").unwrap().as_deref(), Some("light"));
}

#[test]
fn s4_a_setting_is_absent_until_set_and_set_again_replaces_it() {
    let dir = temp_dir();
    let mut state = AppState::open(&state_path(&dir)).unwrap();

    assert_eq!(state.setting("theme").unwrap(), None);
    state.set_setting("theme", "light").unwrap();
    state.set_setting("other", "x").unwrap();
    state.set_setting("theme", "system").unwrap();

    assert_eq!(state.setting("theme").unwrap().as_deref(), Some("system"));
    assert_eq!(state.setting("other").unwrap().as_deref(), Some("x"));
}

#[test]
fn s4_a_setting_survives_reopening_the_file() {
    let dir = temp_dir();
    let path = state_path(&dir);
    AppState::open(&path)
        .unwrap()
        .set_setting("theme", "dark")
        .unwrap();

    let state = AppState::open(&path).unwrap();

    assert_eq!(state.setting("theme").unwrap().as_deref(), Some("dark"));
}
