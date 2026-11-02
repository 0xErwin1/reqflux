//! Recent workspaces (recent-workspaces S1): the app state v4 table, upsert, the cap of 20,
//! removal and the refusal of paths that are not UTF-8. Every test uses a temporary state
//! file.

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
use reqflux_engine::state::{AppState, RECENT_WORKSPACES_MAX, RecentWorkspace, StateError};
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

/// Schema v3 exactly as that build wrote it, with one draft, one history entry and one
/// current value.
const V3: &str = "CREATE TABLE drafts (
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
 PRAGMA user_version = 3;";

fn recent(path: &str, id: &str, name: &str, opened_at_ms: i64) -> RecentWorkspace {
    RecentWorkspace {
        path: PathBuf::from(path),
        workspace_id: id.to_owned(),
        name: name.to_owned(),
        opened_at_ms,
    }
}

#[test]
fn s1_a_v3_file_upgrades_to_v4_keeping_drafts_history_and_current_values() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, V3);

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
    assert!(state.recent_workspaces().unwrap().is_empty());
    state
        .record_recent_workspace(Path::new("/w/a"), "ws", "A", 10)
        .unwrap();
    assert_eq!(
        state.recent_workspaces().unwrap(),
        vec![recent("/w/a", "ws", "A", 10)]
    );
}

#[test]
fn s1_an_empty_file_is_created_at_v4_with_every_table() {
    let dir = temp_dir();
    let path = state_path(&dir);

    let mut state = AppState::open(&path).unwrap();

    assert_eq!(user_version(&path), 7);
    assert!(state.drafts("ws").unwrap().is_empty());
    assert!(state.history("ws", 10).unwrap().is_empty());
    assert!(state.current_values("ws").unwrap().is_empty());
    state
        .record_recent_workspace(Path::new("/w/a"), "ws", "A", 1)
        .unwrap();
    assert_eq!(state.recent_workspaces().unwrap().len(), 1);
}

#[test]
fn s1_a_file_newer_than_v4_is_refused() {
    let dir = temp_dir();
    let path = state_path(&dir);
    write_file(&path, "PRAGMA user_version = 8;");
    match AppState::open(&path) {
        Err(StateError::UnsupportedVersion { version, .. }) => assert_eq!(version, 8),
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn s1_recent_workspaces_are_newest_first_and_recording_again_refreshes() {
    let dir = temp_dir();
    let mut state = AppState::open(&state_path(&dir)).unwrap();
    state
        .record_recent_workspace(Path::new("/w/a"), "id-a", "A", 1)
        .unwrap();
    state
        .record_recent_workspace(Path::new("/w/b"), "id-b", "B", 2)
        .unwrap();
    assert_eq!(
        state.recent_workspaces().unwrap(),
        vec![
            recent("/w/b", "id-b", "B", 2),
            recent("/w/a", "id-a", "A", 1)
        ]
    );

    // Reopening `a` under a new name moves it to the front and updates it in place.
    state
        .record_recent_workspace(Path::new("/w/a"), "id-a2", "A renamed", 3)
        .unwrap();

    assert_eq!(
        state.recent_workspaces().unwrap(),
        vec![
            recent("/w/a", "id-a2", "A renamed", 3),
            recent("/w/b", "id-b", "B", 2)
        ]
    );
}

#[test]
fn s1_the_list_keeps_only_the_newest_twenty() {
    let dir = temp_dir();
    let mut state = AppState::open(&state_path(&dir)).unwrap();
    assert_eq!(RECENT_WORKSPACES_MAX, 20);
    for n in 0..25_i64 {
        state
            .record_recent_workspace(Path::new(&format!("/w/{n}")), "id", "W", n)
            .unwrap();
    }

    let listed = state.recent_workspaces().unwrap();

    assert_eq!(listed.len(), 20);
    let opened: Vec<i64> = listed.iter().map(|entry| entry.opened_at_ms).collect();
    assert_eq!(opened, (5..25).rev().collect::<Vec<_>>());
    assert_eq!(listed[0].path, PathBuf::from("/w/24"));
    assert_eq!(listed[19].path, PathBuf::from("/w/5"));
}

#[test]
fn s1_remove_drops_one_entry_and_reports_whether_it_existed() {
    let dir = temp_dir();
    let mut state = AppState::open(&state_path(&dir)).unwrap();
    state
        .record_recent_workspace(Path::new("/w/a"), "id-a", "A", 1)
        .unwrap();
    state
        .record_recent_workspace(Path::new("/w/b"), "id-b", "B", 2)
        .unwrap();

    assert!(state.remove_recent_workspace(Path::new("/w/a")).unwrap());
    assert!(!state.remove_recent_workspace(Path::new("/w/a")).unwrap());

    assert_eq!(
        state.recent_workspaces().unwrap(),
        vec![recent("/w/b", "id-b", "B", 2)]
    );
}

#[cfg(unix)]
#[test]
fn s1_a_path_that_is_not_utf8_is_refused_and_nothing_is_stored() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = temp_dir();
    let mut state = AppState::open(&state_path(&dir)).unwrap();
    let bad = Path::new(OsStr::from_bytes(b"/w/\xff\xfe"));

    let error = state
        .record_recent_workspace(bad, "id", "W", 1)
        .unwrap_err();
    assert!(
        matches!(error, StateError::NonUtf8Path { .. }),
        "expected NonUtf8Path, got {error:?}"
    );
    assert!(state.recent_workspaces().unwrap().is_empty());
    assert!(matches!(
        state.remove_recent_workspace(bad),
        Err(StateError::NonUtf8Path { .. })
    ));
}
