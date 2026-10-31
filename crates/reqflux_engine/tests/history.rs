//! The send history in the app state: schema migration, recording, listing and limits.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::{Path, PathBuf};

use common::{collection, post, prepare, request, workspace};
use reqflux_core::model::Variable;

use pretty_assertions::assert_eq;
use reqflux_engine::state::{
    AppState, Draft, HISTORY_MAX_BODY_BYTES, HISTORY_MAX_ENTRIES, HistoryLimits, HistoryOutcome,
    HistoryRecord, RecordedResponse, StateError,
};
use reqflux_engine::{MASK, SentBody, SentRequest};

fn state_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("state.sqlite3")
}

fn sent(url: &str) -> SentRequest {
    SentRequest {
        method: "POST".to_owned(),
        url: url.to_owned(),
        headers: vec![("Accept".to_owned(), "*/*".to_owned())],
        body: SentBody::Text("{\"a\":1}".to_owned()),
    }
}

fn record(n: u32) -> HistoryRecord {
    HistoryRecord {
        sent_at: 1_000 + i64::from(n),
        request_id: format!("req-{n}"),
        request_name: format!("Request {n}"),
        request_path: format!("api/r{n}"),
        document: format!("name = \"Request {n}\"\n"),
        sent: sent(&format!("https://example.test/{n}?q=1")),
        outcome: HistoryOutcome::Response(RecordedResponse {
            status: 200,
            reason: Some("OK".to_owned()),
            headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
            body: format!("body {n}").into_bytes(),
            truncated: false,
            elapsed_ms: 12,
            size: 6,
        }),
    }
}

fn open(path: &Path) -> AppState {
    match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open: {error}"),
    }
}

/// Writes a schema v1 file the way a previous build did, with one draft in it.
fn write_v1(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE drafts (
             workspace_id TEXT NOT NULL,
             entity_id TEXT NOT NULL,
             payload TEXT NOT NULL,
             base_hash TEXT,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (workspace_id, entity_id)
         );
         INSERT INTO drafts VALUES ('ws', 'req', 'a = 1', 'hash-1', 7);
         PRAGMA user_version = 1;",
    )
    .unwrap();
}

fn user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_v1_file_upgrades_to_the_latest_and_keeps_its_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let path = state_path(&dir);
    write_v1(&path);

    let mut state = open(&path);

    assert_eq!(user_version(&path), 5);
    let draft = state.draft("ws", "req").unwrap().unwrap();
    assert_eq!(draft.payload, "a = 1");
    assert_eq!(draft.base_hash.as_deref(), Some("hash-1"));
    assert_eq!(draft.updated_at, 7);
    // The history table exists after the upgrade.
    state.record_history("ws", record(1)).unwrap();
    assert_eq!(state.history("ws", 10).unwrap().len(), 1);
}

#[test]
fn a_fresh_file_is_created_at_v3() {
    let dir = tempfile::tempdir().unwrap();
    let path = state_path(&dir);
    open(&path);
    assert_eq!(user_version(&path), 5);
}

#[test]
fn a_file_newer_than_the_build_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = state_path(&dir);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA user_version = 6;").unwrap();
    }
    match AppState::open(&path) {
        Err(StateError::UnsupportedVersion { version, .. }) => assert_eq!(version, 6),
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn drafts_still_work_after_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = state_path(&dir);
    write_v1(&path);
    let mut state = open(&path);

    state
        .put_draft("ws", "other", Draft::new("b = 2", None))
        .unwrap();
    assert_eq!(
        state.draft("ws", "other").unwrap().unwrap().payload,
        "b = 2"
    );
    assert_eq!(state.drafts("ws").unwrap().len(), 2);
    assert!(state.delete_draft("ws", "req").unwrap());
    assert_eq!(state.draft("ws", "req").unwrap(), None);
}

#[test]
fn record_get_and_round_trip_every_field() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));

    let id = state.record_history("ws", record(1)).unwrap();
    let entry = state.history_entry(id).unwrap().unwrap();

    assert_eq!(entry.id, id);
    assert_eq!(entry.workspace_id, "ws");
    assert_eq!(entry.record, record(1));
    assert_eq!(state.history_entry(id + 100).unwrap(), None);
}

#[test]
fn a_failed_send_and_the_body_markers_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    let mut failed = record(1);
    failed.sent.body = SentBody::Omitted("multipart form data".to_owned());
    failed.outcome = HistoryOutcome::Failed {
        error: "connection refused".to_owned(),
        elapsed_ms: 3,
    };
    let mut empty = record(2);
    empty.sent.body = SentBody::None;

    let failed_id = state.record_history("ws", failed.clone()).unwrap();
    let empty_id = state.record_history("ws", empty.clone()).unwrap();

    assert_eq!(
        state.history_entry(failed_id).unwrap().unwrap().record,
        failed
    );
    assert_eq!(
        state.history_entry(empty_id).unwrap().unwrap().record,
        empty
    );
}

#[test]
fn list_is_newest_first_limited_and_per_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    for n in 1..=4 {
        state.record_history("ws", record(n)).unwrap();
    }
    state.record_history("other", record(9)).unwrap();

    let listed: Vec<String> = state
        .history("ws", 3)
        .unwrap()
        .into_iter()
        .map(|entry| entry.record.request_id)
        .collect();

    assert_eq!(listed, ["req-4", "req-3", "req-2"]);
    assert_eq!(state.history("other", 10).unwrap().len(), 1);
}

#[test]
fn clear_removes_only_that_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    state.record_history("ws", record(1)).unwrap();
    state.record_history("ws", record(2)).unwrap();
    state.record_history("other", record(3)).unwrap();
    state.put_draft("ws", "req", Draft::new("a", None)).unwrap();

    assert_eq!(state.clear_history("ws").unwrap(), 2);

    assert!(state.history("ws", 10).unwrap().is_empty());
    assert_eq!(state.history("other", 10).unwrap().len(), 1);
    assert!(state.draft("ws", "req").unwrap().is_some());
}

#[test]
fn the_default_limits_are_500_entries_and_5_mb() {
    assert_eq!(HISTORY_MAX_ENTRIES, 500);
    assert_eq!(HISTORY_MAX_BODY_BYTES, 5 * 1024 * 1024);
    assert_eq!(
        HistoryLimits::default(),
        HistoryLimits {
            max_entries: 500,
            max_body_bytes: 5 * 1024 * 1024,
        }
    );
}

#[test]
fn recording_prunes_a_workspace_to_its_newest_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    let limits = HistoryLimits {
        max_entries: 3,
        ..HistoryLimits::default()
    };
    state.record_history("other", record(99)).unwrap();
    for n in 1..=5 {
        state
            .record_history_with_limits("ws", record(n), limits)
            .unwrap();
    }

    let listed: Vec<String> = state
        .history("ws", 10)
        .unwrap()
        .into_iter()
        .map(|entry| entry.record.request_id)
        .collect();

    assert_eq!(listed, ["req-5", "req-4", "req-3"]);
    assert_eq!(state.history("other", 10).unwrap().len(), 1);
}

#[test]
fn a_body_over_the_cap_is_stored_cut_with_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    let mut big = record(1);
    let body = vec![b'x'; HISTORY_MAX_BODY_BYTES + 10];
    if let HistoryOutcome::Response(response) = &mut big.outcome {
        response.body = body.clone();
        response.size = body.len() as u64;
    }
    let mut exact = record(2);
    if let HistoryOutcome::Response(response) = &mut exact.outcome {
        response.body = vec![b'y'; HISTORY_MAX_BODY_BYTES];
    }

    let big_id = state.record_history("ws", big).unwrap();
    let exact_id = state.record_history("ws", exact).unwrap();

    let HistoryOutcome::Response(stored) =
        state.history_entry(big_id).unwrap().unwrap().record.outcome
    else {
        panic!("expected a response");
    };
    assert_eq!(stored.body.len(), HISTORY_MAX_BODY_BYTES);
    assert!(stored.truncated);
    assert_eq!(stored.size, (HISTORY_MAX_BODY_BYTES + 10) as u64);
    let HistoryOutcome::Response(stored) = state
        .history_entry(exact_id)
        .unwrap()
        .unwrap()
        .record
        .outcome
    else {
        panic!("expected a response");
    };
    assert_eq!(stored.body.len(), HISTORY_MAX_BODY_BYTES);
    assert!(!stored.truncated);
}

#[test]
fn a_failed_migration_leaves_v1_and_its_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let path = state_path(&dir);
    write_v1(&path);
    {
        // A table named `history` without the v2 columns makes the v2 index fail.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE history (other TEXT);")
            .unwrap();
    }

    match AppState::open(&path) {
        Err(StateError::Migrate { .. }) => {}
        other => panic!("expected Migrate, got {other:?}"),
    }

    assert_eq!(user_version(&path), 1);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let payload: String = conn
        .query_row(
            "SELECT payload FROM drafts WHERE workspace_id = 'ws' AND entity_id = 'req'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, "a = 1");
}

#[test]
fn the_default_limit_keeps_exactly_500() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    for n in 1..=502 {
        state.record_history("ws", record(n)).unwrap();
    }

    let listed = state.history("ws", 1_000).unwrap();

    assert_eq!(listed.len(), 500);
    assert_eq!(listed.first().unwrap().record.request_path, "api/r502");
    assert_eq!(listed.last().unwrap().record.request_path, "api/r3");
}

#[test]
fn a_utf8_body_is_cut_on_a_character_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = open(&state_path(&dir));
    let mut euros = record(1);
    // `€` is three bytes, so the byte limit falls inside a character.
    let body = "€".repeat(HISTORY_MAX_BODY_BYTES / 3 + 1).into_bytes();
    assert_ne!(HISTORY_MAX_BODY_BYTES % 3, 0);
    if let HistoryOutcome::Response(response) = &mut euros.outcome {
        response.body = body;
    }
    let mut binary = record(2);
    if let HistoryOutcome::Response(response) = &mut binary.outcome {
        response.body = vec![0xff; HISTORY_MAX_BODY_BYTES + 1];
    }

    let euros_id = state.record_history("ws", euros).unwrap();
    let binary_id = state.record_history("ws", binary).unwrap();

    let stored = |id| match state.history_entry(id).unwrap().unwrap().record.outcome {
        HistoryOutcome::Response(response) => response,
        HistoryOutcome::Failed { .. } => panic!("expected a response"),
    };
    let text = stored(euros_id);
    assert!(text.truncated);
    assert!(std::str::from_utf8(&text.body).is_ok());
    assert_eq!(text.body.len(), HISTORY_MAX_BODY_BYTES / 3 * 3);
    assert_eq!(stored(binary_id).body.len(), HISTORY_MAX_BODY_BYTES);
}

fn echo_response(headers: Vec<(String, String)>, body: &[u8]) -> reqflux_engine::Response {
    reqflux_engine::Response {
        status: 401,
        reason: Some("Unauthorized".to_owned()),
        version: "1.1".to_owned(),
        headers,
        body: body.to_vec(),
        final_url: "http://h/".to_owned(),
        elapsed: std::time::Duration::from_millis(5),
    }
}

/// The masker of a send whose secret variable `token` is `s3cret`.
fn secret_masker() -> reqflux_engine::Masker {
    let ws = workspace(
        vec![Variable {
            secret: true,
            ..Variable::new("token", "s3cret")
        }],
        vec![collection(
            "c",
            vec![request("r", post("https://api.test/"))],
        )],
    );
    prepare(&ws).masker()
}

#[test]
fn a_recorded_response_masks_secrets_in_headers_reason_and_text_body() {
    let response = reqflux_engine::Response {
        reason: Some("bad s3cret".to_owned()),
        ..echo_response(
            vec![
                ("set-cookie".to_owned(), "sid=s3cret; Path=/".to_owned()),
                ("Set-Cookie".to_owned(), "theme=dark".to_owned()),
                ("x-echo".to_owned(), "key s3cret".to_owned()),
                ("content-type".to_owned(), "text/plain".to_owned()),
            ],
            b"invalid key s3cret",
        )
    };
    let recorded = RecordedResponse::from_response_masked(&response, &secret_masker());
    assert_eq!(
        recorded.headers,
        vec![
            // A cookie is stored as received, with only a secret value inside it masked.
            ("set-cookie".to_owned(), format!("sid={MASK}; Path=/")),
            ("Set-Cookie".to_owned(), "theme=dark".to_owned()),
            ("x-echo".to_owned(), format!("key {MASK}")),
            ("content-type".to_owned(), "text/plain".to_owned()),
        ]
    );
    assert_eq!(recorded.body, format!("invalid key {MASK}").into_bytes());
    assert_eq!(recorded.reason, Some(format!("bad {MASK}")));
    // The size is the response's as received.
    assert_eq!(recorded.size, 18);
    assert!(!format!("{recorded:?}").contains("s3cret"));
}

#[test]
fn a_recorded_binary_body_is_stored_as_received() {
    let body = [0xff, 0xfe, b's', b'3', b'c', b'r', b'e', b't'];
    let response = echo_response(Vec::new(), &body);
    let recorded = RecordedResponse::from_response_masked(&response, &secret_masker());
    assert_eq!(recorded.body, body.to_vec());
}

#[test]
fn a_recorded_response_without_secrets_is_unchanged() {
    let response = echo_response(
        vec![
            ("x-echo".to_owned(), "plain".to_owned()),
            ("Set-Cookie".to_owned(), "theme=dark".to_owned()),
        ],
        "{\"ok\":true} €".as_bytes(),
    );
    // A cookie without a secret in it is stored as received, like every other header.
    let expected = RecordedResponse::from_response(&response);
    assert_eq!(
        RecordedResponse::from_response_masked(&response, &secret_masker()),
        expected
    );
    assert_eq!(
        RecordedResponse::from_response_masked(&response, &reqflux_engine::Masker::default()),
        expected
    );
}
