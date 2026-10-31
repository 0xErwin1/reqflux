//! Send history in the window (history S6-S8): every completed or failed send is recorded
//! in the app state, the rail's History button lists it in the sidebar island, a row
//! reopens its recorded response, and Resend sends the recorded document again.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::state::{AppState, HistoryEntry, HistoryOutcome};
use reqflux_engine::{MASK, SecretKey, SecretScope};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::{Server, closed_port, response, serve, settle};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000081";
const PING: &str = "01J00000000000000000000082";
const OTHER: &str = "01J00000000000000000000083";
const KEYED: &str = "01J00000000000000000000084";
const ECHO: &str = "01J00000000000000000000085";
const TOKEN: &str = "s3cret-token";
const WORKSPACE: &str = "01J00000000000000000000900";
const API_KEY: &str = "k-echoed-SECRET";

/// A workspace whose collection `API` holds `GET Ping` at `/ping` with a bearer token and
/// `POST Other` at `/other`, both on `port`.
fn history_workspace(port: u16) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/ping.req.toml",
        &format!(
            "id = \"{PING}\"\nname = \"Ping\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{port}/ping\"\n\n[auth]\ntype = \"bearer\"\ntoken = \"{TOKEN}\"\n"
        ),
    );
    write(
        root,
        "collections/api/other.req.toml",
        &format!(
            "id = \"{OTHER}\"\nname = \"Other\"\nmethod = \"POST\"\n\
             url = \"http://127.0.0.1:{port}/other\"\n"
        ),
    );
    dir
}

/// The recorded history of the loaded workspace, newest first.
fn history(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<HistoryEntry> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        match (shell.state.as_ref(), shell.workspace_id.as_deref()) {
            (Some(state), Some(workspace)) => match state.history(workspace, 100) {
                Ok(entries) => entries,
                Err(error) => panic!("history: {error}"),
            },
            _ => Vec::new(),
        }
    })
}

/// Opens the request `id` and sends it, waiting for the outcome.
fn send(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<String> {
    click(cx, &format!("tree-row-{id}"));
    click(cx, "request-send");
    settle(cx, shell, id)
}

/// The texts of the History panel rows, top first.
fn rows(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).history_texts()
    })
}

fn no_drafts(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).drafts.is_empty())
}

/// The text of the history label of the response, if it shows one.
fn history_label(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .sends
            .get(id)
            .and_then(|send| send.history.as_ref())
            .map(super::history::Reopened::label)
    })
}

#[gpui::test]
fn a_completed_send_is_recorded_with_its_secret_masked(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));

    let entries = history(cx, &shell);
    assert_eq!(entries.len(), 1);
    let record = &entries.first().map(|entry| entry.record.clone());
    let Some(record) = record else {
        panic!("no entry");
    };
    assert_eq!(record.request_id, PING);
    assert_eq!(record.request_name, "Ping");
    assert_eq!(record.request_path, "api/ping");
    assert_eq!(record.sent.method, "GET");
    assert!(record.document.contains("/ping"), "{}", record.document);
    let authorization = record
        .sent
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.clone());
    assert_eq!(authorization, Some(format!("Bearer {MASK}")));
    assert!(
        !format!("{:?}", record.sent).contains(TOKEN),
        "the token reached history"
    );
    assert!(
        matches!(&record.outcome, HistoryOutcome::Response(response) if response.status == 200)
    );
}

#[gpui::test]
fn a_failed_send_is_recorded_as_failed(cx: &mut TestAppContext) {
    let dir = history_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let text = send(cx, &shell, OTHER).unwrap_or_default();
    assert!(text.starts_with("Failed: could not connect"), "{text}");

    let entries = history(cx, &shell);
    assert_eq!(entries.len(), 1);
    assert!(
        entries.first().is_some_and(|entry| matches!(
            &entry.record.outcome,
            HistoryOutcome::Failed { error, .. } if error.starts_with("could not connect")
        )),
        "{entries:?}"
    );
    click(cx, "rail-history");
    assert_eq!(rows(cx, &shell), vec!["POST /other · failed · just now"]);
}

#[gpui::test]
fn a_failed_send_with_a_query_secret_is_recorded_masked(cx: &mut TestAppContext) {
    let dir = history_workspace(closed_port());
    write(
        dir.path(),
        "collections/api/keyed.req.toml",
        &format!(
            "id = \"{KEYED}\"\nname = \"Keyed\"\nmethod = \"GET\"\nurl = \"{{{{baseUrl}}}}/v1\"\n\n\
             [auth]\ntype = \"apikey\"\nkey = \"api_key\"\nvalue = \"k-SECRET-123\"\nlocation = \"query\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    send(cx, &shell, KEYED);

    let entries = history(cx, &shell);
    let Some(HistoryOutcome::Failed { error, .. }) =
        entries.first().map(|entry| &entry.record.outcome)
    else {
        panic!("expected a failed entry: {entries:?}");
    };
    assert!(error.contains(MASK), "{error}");
    assert!(!error.contains("k-SECRET-123"), "{error}");
}

#[gpui::test]
fn a_cancelled_send_is_not_recorded(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Silent));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PING}"));
    click(cx, "request-send");
    click(cx, "request-cancel");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
    assert!(history(cx, &shell).is_empty());
}

#[gpui::test]
fn the_history_panel_lists_newest_first_and_collections_returns(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, "rail-history");
    assert!(
        bounds(cx, "collection-tree").is_none(),
        "the tree is swapped out"
    );
    present(cx, "history-empty");
    assert!(cx.update(|_, cx| shell.read(cx).sidebar.read(cx).showing_history()));

    click(cx, "rail-collections");
    present(cx, "collection-tree");
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert_eq!(send(cx, &shell, OTHER).as_deref(), Some("200 OK"));

    click(cx, "rail-history");
    assert!(bounds(cx, "history-empty").is_none());
    assert_eq!(
        rows(cx, &shell),
        vec![
            "POST /other · 200 · just now".to_owned(),
            "GET /ping · 200 · just now".to_owned(),
        ]
    );
    present(cx, "history-row-1");
    present(cx, "history-row-2");

    click(cx, "rail-collections");
    assert!(cx.update(|_, cx| !shell.read(cx).sidebar.read(cx).showing_history()));
    present(cx, &format!("tree-row-{PING}"));
}

#[gpui::test]
fn the_panel_refreshes_when_a_send_is_recorded(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PING}"));
    click(cx, "rail-history");
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert_eq!(rows(cx, &shell), vec!["GET /ping · 200 · just now"]);
}

#[gpui::test]
fn a_row_reopens_the_recorded_response_and_leaves_no_draft(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, &format!("doc-tab-close-{PING}"));
    assert!(bounds(cx, &format!("doc-tab-{PING}")).is_none());

    click(cx, "rail-history");
    click(cx, "history-row-1");
    present(cx, &format!("doc-tab-{PING}"));
    present(cx, "response-history-label");
    present(cx, "history-resend");
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("200 OK"));
    let label = history_label(cx, &shell, PING).unwrap_or_default();
    assert!(label.starts_with("From history · "), "{label}");
    assert!(no_drafts(cx, &shell), "reopening makes no draft");

    // A normal send replaces the history view.
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert!(bounds(cx, "response-history-label").is_none());
    assert_eq!(history(cx, &shell).len(), 2);
}

#[gpui::test]
fn resend_records_a_new_entry_and_shows_its_response(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "rail-history");
    click(cx, "history-row-1");

    click(cx, "history-resend");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert!(bounds(cx, "response-history-label").is_none());
    assert!(history_label(cx, &shell, PING).is_none());
    let entries = history(cx, &shell);
    assert_eq!(entries.len(), 2);
    assert!(no_drafts(cx, &shell), "resending makes no draft");
    assert_eq!(rows(cx, &shell).len(), 2);
}

#[gpui::test]
fn a_row_of_a_deleted_request_is_read_only(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = history_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, &format!("tree-row-{PING}"));
    click(cx, &format!("tree-row-menu-{PING}"));
    click(cx, "tree-menu-delete");
    click(cx, "tree-delete-confirm");
    assert!(bounds(cx, &format!("tree-row-{PING}")).is_none());

    click(cx, "rail-history");
    click(cx, "history-row-1");
    present(cx, "history-readonly");
    present(cx, "response-history-label");
    present(cx, "response-status");
    let shown = cx.update(|_, cx| {
        shell
            .read(cx)
            .orphan
            .as_ref()
            .map(super::history::Orphan::request_line)
    });
    assert_eq!(shown, Some(format!("GET http://127.0.0.1:{port}/ping")));
    present(cx, "history-resend-disabled");
    present(cx, "history-resend-note");
    click(cx, "history-resend");
    assert_eq!(history(cx, &shell).len(), 1, "Resend is disabled");
    assert!(bounds(cx, &format!("doc-tab-{PING}")).is_none());

    // Opening a request leaves the read-only view.
    click(cx, "rail-collections");
    click(cx, &format!("tree-row-{OTHER}"));
    assert!(bounds(cx, "history-readonly").is_none());
}

#[gpui::test]
fn a_send_and_the_tree_still_work_after_history(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-history");
    click(cx, "rail-collections");
    assert_eq!(send(cx, &shell, OTHER).as_deref(), Some("200 OK"));
    present(cx, "response-status");
    assert!(bounds(cx, "response-history-label").is_none());
    click(cx, &format!("tree-row-{PING}"));
    present(cx, &format!("doc-tab-{PING}"));
}

/// Starts a server that answers every request `401` with the request body echoed in an
/// `x-echo` header and in the text body `invalid key <body>`; returns its port.
fn serve_echo() -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("bind: {error}"),
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => panic!("address: {error}"),
    };
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut received = Vec::new();
                let mut byte = [0_u8; 1];
                while !received.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => received.extend_from_slice(&byte),
                        Ok(_) | Err(_) => return,
                    }
                }
                let head = String::from_utf8_lossy(&received).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = vec![0_u8; length];
                if let Err(error) = stream.read_exact(&mut body) {
                    eprintln!("read: {error}");
                    return;
                }
                let body = String::from_utf8_lossy(&body);
                let text = format!("invalid key {body}");
                let reply = format!(
                    "HTTP/1.1 401 Unauthorized\r\nx-echo: {body}\r\ncontent-type: text/plain\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
                if let Err(error) = stream.write_all(reply.as_bytes()) {
                    eprintln!("write: {error}");
                }
            });
        }
    });
    port
}

/// A workspace whose `POST Echo` sends the secret global `apiKey` as its body to `port`.
fn echo_workspace(port: u16) -> TempDir {
    let dir = history_workspace(port);
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{WORKSPACE}\"\nname = \"Demo\"\nformat = 1\n\n\
             [[globals]]\nkey = \"apiKey\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/api/echo.req.toml",
        &format!(
            "id = \"{ECHO}\"\nname = \"Echo\"\nmethod = \"POST\"\n\
             url = \"http://127.0.0.1:{port}/echo\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"text\"\ntext = \"{{{{apiKey}}}}\"\n"
        ),
    );
    dir
}

/// Puts `value` in the secret store as the global `key` of the loaded workspace.
fn store_secret(cx: &mut VisualTestContext, shell: &Entity<Shell>, key: &str, value: &str) {
    cx.update(|_, cx| {
        if let OpenedWorkspace::Loaded(engine) = &shell.read(cx).workspace {
            let key = SecretKey {
                workspace_id: engine.workspace().manifest.id,
                scope: SecretScope::Global,
                key: key.to_owned(),
            };
            if let Err(error) = engine.secret_store().set(&key, value) {
                panic!("secret store: {error}");
            }
        } else {
            panic!("no workspace");
        }
    });
}

/// The response of the newest entry, read from the state file itself.
fn stored_response(state_path: &Path) -> reqflux_engine::state::RecordedResponse {
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    let entries = match state.history(WORKSPACE, 10) {
        Ok(entries) => entries,
        Err(error) => panic!("history: {error}"),
    };
    match entries.into_iter().next().map(|entry| entry.record.outcome) {
        Some(HistoryOutcome::Response(response)) => response,
        other => panic!("expected a response: {other:?}"),
    }
}

#[gpui::test]
fn an_echoed_secret_is_stored_masked_in_the_response(cx: &mut TestAppContext) {
    let dir = echo_workspace(serve_echo());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    store_secret(cx, &shell, "apiKey", API_KEY);
    assert_eq!(send(cx, &shell, ECHO).as_deref(), Some("401 Unauthorized"));

    let stored = stored_response(&state_path);
    let body = String::from_utf8_lossy(&stored.body).into_owned();
    assert_eq!(body, format!("invalid key {MASK}"));
    let echo = stored
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("x-echo"))
        .map(|(_, value)| value.clone());
    assert_eq!(echo, Some(MASK.to_owned()));
    assert!(!format!("{stored:?}").contains(API_KEY), "{stored:?}");
}

#[gpui::test]
fn a_response_without_secrets_is_stored_as_received(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, OTHER).as_deref(), Some("200 OK"));
    let stored = stored_response(&state_path);
    assert_eq!(stored.body, super::send_tests::JSON.as_bytes().to_vec());
    assert_eq!(stored.reason.as_deref(), Some("OK"));
    let names: Vec<_> = stored
        .headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(names.contains(&"content-type"), "{names:?}");
    assert!(
        stored
            .headers
            .contains(&("content-type".to_owned(), "application/json".to_owned()))
    );
}
