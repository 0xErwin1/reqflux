//! Send history in the window (history S6-S8): every completed or failed send is recorded
//! in the app state, the rail's History button lists it in the sidebar island grouped by
//! day, a row opens the read-only history tab with its recorded request and response, and
//! Resend sends the recorded document again (design/history/01-sidebar-readonly-tab).

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
use super::send_tests::{Server, closed_port, serve, settle};
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

/// The texts of the History panel rows, top first: `METHOD name · status`.
fn rows(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).history_texts()
    })
}

fn no_drafts(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).drafts.is_empty())
}

/// The accessible label of the element `selector`, which must be on screen.
fn label(cx: &mut VisualTestContext, selector: &str) -> String {
    present(cx, selector);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .get(selector)
        .and_then(|node| node.label.clone())
        .unwrap_or_default()
}

/// The id of the active request tab, `None` while the history tab is active or none is.
fn active_request(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        shell.active_request().map(str::to_owned)
    })
}

/// Opens the History panel and the history tab of its top row.
fn open_top_row(cx: &mut VisualTestContext) {
    click(cx, "rail-history");
    click(cx, "history-row-1");
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
    assert_eq!(rows(cx, &shell), vec!["POST Other · failed"]);
    assert_eq!(label(cx, "history-row-1-status"), "failed");
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
        vec!["POST Other · 200".to_owned(), "GET Ping · 200".to_owned()]
    );
    // One day group with its count, then the rows' method, name, time and status.
    assert_eq!(label(cx, "history-group-1"), "Today");
    assert_eq!(label(cx, "history-group-1-count"), "2");
    assert!(bounds(cx, "history-group-2").is_none());
    assert_eq!(label(cx, "history-row-1-method"), "POST");
    assert_eq!(label(cx, "history-row-1-name"), "Other");
    assert_eq!(label(cx, "history-row-1-status"), "200");
    let time = label(cx, "history-row-1-time");
    assert!(
        time.len() == 5 && time.as_bytes().get(2) == Some(&b':'),
        "HH:MM, got {time}"
    );
    assert_eq!(label(cx, "history-row-2-name"), "Ping");

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
    assert_eq!(rows(cx, &shell), vec!["GET Ping · 200"]);
}

#[gpui::test]
fn the_empty_panel_explains_history(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-history");
    assert_eq!(label(cx, "history-empty-title"), "No requests sent yet");
    assert_eq!(
        label(cx, "history-empty-text"),
        "Every send is recorded here with its response, grouped by day. Secrets are never stored."
    );
    assert_eq!(label(cx, "history-empty-hint"), "Send the open request");
    present(cx, "history-empty-hint-keycap");
    assert!(
        bounds(cx, "history-filter").is_none(),
        "no filter without rows"
    );
}

#[gpui::test]
fn the_filter_narrows_the_rows_by_name_url_or_status(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert_eq!(send(cx, &shell, OTHER).as_deref(), Some("200 OK"));
    click(cx, "rail-history");

    click(cx, "history-filter");
    cx.simulate_input("/pi");
    cx.run_until_parked();
    assert_eq!(label(cx, "history-row-1-name"), "Ping");
    assert!(
        bounds(cx, "history-row-2").is_none(),
        "Other is filtered out"
    );
    assert_eq!(label(cx, "history-group-1-count"), "1");

    cx.simulate_keystrokes("ctrl-a backspace");
    cx.simulate_input("oth");
    cx.run_until_parked();
    assert_eq!(label(cx, "history-row-1-name"), "Other");
    assert!(bounds(cx, "history-row-2").is_none());

    cx.simulate_keystrokes("ctrl-a backspace");
    cx.simulate_input("404");
    cx.run_until_parked();
    assert!(bounds(cx, "history-row-1").is_none());
    assert_eq!(label(cx, "history-no-match"), "No sends match the filter");

    // The panel keeps every row; only the view is filtered.
    assert_eq!(rows(cx, &shell).len(), 2);
}

#[gpui::test]
fn a_row_opens_the_read_only_history_tab(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "rail-history");
    click(cx, "history-row-1");

    // The history tab is active beside the request's own tab.
    assert_eq!(label(cx, "doc-tab-history"), "GET Ping");
    present(cx, "doc-tab-history-time");
    present(cx, &format!("doc-tab-{PING}"));
    assert_eq!(active_request(cx, &shell), None);
    assert!(
        bounds(cx, "request-send").is_none(),
        "no editable request line"
    );

    // The banner, the recorded request line and its response.
    assert_eq!(
        label(cx, "history-banner-title"),
        "Recorded request, read only"
    );
    present(cx, "history-banner-meta");
    present(cx, "history-open-source");
    assert_eq!(label(cx, "history-resend"), "Resend");
    present(cx, "history-resend-key");
    assert!(bounds(cx, "history-resend-disabled").is_none());
    assert_eq!(label(cx, "history-method"), "GET");
    assert!(label(cx, "history-url").ends_with("/ping"));
    present(cx, "response-status");
    assert!(bounds(cx, "response-history-label").is_none());
    assert!(no_drafts(cx, &shell), "opening history makes no draft");

    // The recorded headers, with the secret as history masked it.
    click(cx, "history-tab-headers");
    assert_eq!(
        label(cx, "history-tabs-note"),
        "Values as sent; variables already resolved"
    );
    let mut headers = Vec::new();
    for n in 1..=8 {
        let id = format!("history-grid-key-{n}");
        if bounds(cx, &id).is_none() {
            break;
        }
        headers.push(label(cx, &id));
    }
    let Some(n) = headers
        .iter()
        .position(|key| key.eq_ignore_ascii_case("authorization"))
    else {
        panic!("no Authorization row in {headers:?}");
    };
    let value = label(cx, &format!("history-grid-value-{}", n + 1));
    assert_eq!(value, format!("Bearer {MASK}"));
    assert!(!value.contains(TOKEN));

    // Activating the request tab leaves the history tab in the row; closing it removes it.
    click(cx, &format!("doc-tab-{PING}"));
    assert_eq!(active_request(cx, &shell).as_deref(), Some(PING));
    present(cx, "request-send");
    present(cx, "doc-tab-history");
    click(cx, "doc-tab-history");
    present(cx, "history-banner");
    click(cx, "doc-tab-history-close");
    assert!(bounds(cx, "doc-tab-history").is_none());
    assert!(bounds(cx, "history-banner").is_none());
    assert_eq!(active_request(cx, &shell).as_deref(), Some(PING));
}

#[gpui::test]
fn open_source_request_opens_its_tab(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, &format!("doc-tab-close-{PING}"));
    open_top_row(cx);
    assert!(bounds(cx, &format!("doc-tab-{PING}")).is_none());

    click(cx, "history-open-source");
    assert_eq!(active_request(cx, &shell).as_deref(), Some(PING));
    present(cx, "request-send");
    present(cx, "doc-tab-history");
}

#[gpui::test]
fn resend_records_a_new_entry_and_shows_its_response(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);

    click(cx, "history-resend");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert_eq!(active_request(cx, &shell).as_deref(), Some(PING));
    assert_eq!(history(cx, &shell).len(), 2);
    assert!(no_drafts(cx, &shell), "resending makes no draft");
    assert_eq!(rows(cx, &shell).len(), 2);
}

#[gpui::test]
fn ctrl_enter_in_the_history_tab_resends(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, &format!("doc-tab-close-{PING}"));
    open_top_row(cx);

    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert_eq!(history(cx, &shell).len(), 2);
    assert_eq!(active_request(cx, &shell).as_deref(), Some(PING));
}

#[gpui::test]
fn ctrl_enter_in_a_request_tab_still_sends_it(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{OTHER}"));
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, OTHER).as_deref(), Some("200 OK"));
    assert_eq!(history(cx, &shell).len(), 1);
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
    present(cx, "history-row-1-deleted");
    click(cx, "history-row-1");
    present(cx, "history-readonly");
    present(cx, "response-status");
    assert_eq!(
        label(cx, "history-banner-title"),
        "The source request Ping was deleted."
    );
    assert_eq!(
        label(cx, "history-banner-meta"),
        "This record can no longer be sent."
    );
    assert_eq!(
        label(cx, "history-url"),
        format!("http://127.0.0.1:{port}/ping")
    );
    assert!(bounds(cx, "history-open-source").is_none());
    present(cx, "history-resend-disabled");
    assert_eq!(
        label(cx, "history-resend-note"),
        "Resend needs the request to exist"
    );
    click(cx, "history-resend");
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(history(cx, &shell).len(), 1, "Resend is disabled");
    assert!(bounds(cx, &format!("doc-tab-{PING}")).is_none());

    // Opening a request activates it; the history tab stays in the row.
    click(cx, "rail-collections");
    click(cx, &format!("tree-row-{OTHER}"));
    assert!(bounds(cx, "history-readonly").is_none());
    present(cx, "doc-tab-history");
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
    assert!(bounds(cx, "history-banner").is_none());
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
