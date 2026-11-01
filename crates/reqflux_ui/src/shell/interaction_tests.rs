//! Where the shell's features meet: drafts and environment changes, sends and saves,
//! tab switching and in-flight sends, and scripts beside drafts. Each test pins the
//! observable state one crossing must keep; the pieces alone live in the sibling tests.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::Request;
use reqflux_engine::Span;
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, present, render, state_file};
use super::send_tests::{Server, closed_port, response, serve, settle};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000061";
const PLAIN: &str = "01J00000000000000000000062";
const SECOND: &str = "01J00000000000000000000063";
const DEV: &str = "01J00000000000000000000064";
/// The id `demo_workspace` writes, the key drafts are stored under.
const WORKSPACE: &str = "01J00000000000000000000900";

/// A workspace whose collection `API` points `base` at `port`, holding `GET Plain` at
/// `{{base}}/{{host}}/ok` with one header, `GET Second` at `{{base}}/two`, and the
/// environment `dev`, which defines `host`. Without an environment `{{host}}` is
/// unresolved, so the styling has something to follow.
fn workspace(port: u16) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://127.0.0.1:{port}\"\n"
        ),
    );
    write(
        root,
        "collections/api/plain.req.toml",
        &format!(
            "id = \"{PLAIN}\"\nname = \"Plain\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/{{{{host}}}}/ok\"\n\n\
             [[headers]]\nkey = \"X-One\"\nvalue = \"1\"\n"
        ),
    );
    write(
        root,
        "collections/api/second.req.toml",
        &format!(
            "id = \"{SECOND}\"\nname = \"Second\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/two\"\n"
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{DEV}\"\nname = \"dev\"\n\n\
             [[variables]]\nkey = \"host\"\nvalue = \"h1\"\n"
        ),
    );
    dir
}

/// Reads the request head off `stream`.
fn read_head(stream: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.extend_from_slice(&byte),
            Ok(_) | Err(_) => return,
        }
    }
}

/// A server that answers every connection with `200 OK` after `delay`, so a test can act
/// on the window while the send is in flight.
fn slow_server(delay: Duration) -> u16 {
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
                read_head(&mut stream);
                thread::sleep(delay);
                let body = "{}";
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                if let Err(error) = stream.write_all(reply.as_bytes()) {
                    eprintln!("write: {error}");
                }
            });
        }
    });
    port
}

/// A pre-request script that busy-loops for `millis` of wall-clock time.
fn busy_script(millis: u64) -> String {
    format!("const start = Date.now();\nwhile (Date.now() - start < {millis}) {{}}\n")
}

/// Replaces the text of the URL field with `url`: opens the field over the read-only URL,
/// selects all and types.
fn edit_url(cx: &mut VisualTestContext, url: &str) {
    click(cx, "request-url");
    cx.simulate_keystrokes("ctrl-a");
    let typed = url
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

/// Replaces the text of the open cell field with `text`: selects all and types.
fn type_cell(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("ctrl-a");
    let typed = text
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

/// The request the draft of `id` holds, if it has one.
fn draft_request(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<Request> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(id)
            .map(|draft| draft.request.clone())
    })
}

/// The URL the request line of `id` shows: the draft's when it has one.
fn displayed_url(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<String> {
    cx.update(|_, cx| Some(shell.read(cx).displayed_url(id)))
}

/// The slug of the window's active environment, `None` for No environment.
fn active_environment(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).active_environment.clone())
}

/// The active request of the window, if any.
fn active_document(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).documents.active().map(str::to_owned))
}

/// The number of unresolved variables the request `id` is inspected with.
fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<usize> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(id)
            .map(|state| state.inspection.unresolved.len())
    })
}

/// Whether the `{{name}}` reference of `url` is styled as resolving for the request `id`:
/// the spans the URL line is rendered from, against the active environment.
fn reference_resolves(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    id: &str,
    url: &str,
    name: &str,
) -> Option<bool> {
    let url = url.to_owned();
    let name = name.to_owned();
    cx.update(|_, cx| {
        let state = shell.read(cx).requests.get(id)?;
        state
            .inspection
            .spans(&url)
            .into_iter()
            .find_map(|span| match span {
                Span::Reference {
                    name: found,
                    resolves,
                    ..
                } if found == name => Some(resolves),
                _ => None,
            })
    })
}

/// Chooses `slug` in the environment menu, or No environment for `None`.
fn choose_environment(cx: &mut VisualTestContext, slug: Option<&str>) {
    click(cx, "environment-selector");
    let option = match slug {
        Some(slug) => format!("environment-option-{slug}"),
        None => "environment-none".to_owned(),
    };
    click(cx, Box::leak(option.into_boxed_str()));
}

/// The drafts the app state file stores for the workspace.
fn stored_drafts(state_path: &std::path::Path) -> Vec<String> {
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    state
        .drafts(WORKSPACE)
        .unwrap_or_default()
        .into_iter()
        .map(|(entity, _)| entity)
        .collect()
}

/// 1. A draft exists, then the environment changes: the draft survives with its URL and
/// cell edit, the tab stays marked, and the variable styling follows the new environment.
#[gpui::test]
fn an_environment_change_keeps_the_draft_and_restyles_it(cx: &mut TestAppContext) {
    let dir = workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    assert_eq!(unresolved(cx, &shell, PLAIN), Some(1), "no environment yet");
    let file_url = "{{base}}/{{host}}/ok";
    assert_eq!(
        reference_resolves(cx, &shell, PLAIN, file_url, "host"),
        Some(false),
        "{{host}} is unresolved without an environment"
    );

    let edited = "{{base}}/{{host}}/edited";
    edit_url(cx, edited);
    click(cx, "request-headers-value-1");
    type_cell(cx, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(marker(cx, PLAIN), "the edits mark the tab");

    choose_environment(cx, Some("dev"));
    assert_eq!(active_environment(cx, &shell).as_deref(), Some("dev"));

    assert_eq!(
        displayed_url(cx, &shell, PLAIN).as_deref(),
        Some(edited),
        "the URL edit survived the environment change"
    );
    let draft = draft_request(cx, &shell, PLAIN).expect("the draft survived");
    assert_eq!(draft.headers[0].value, "42", "the cell edit survived");
    assert!(marker(cx, PLAIN), "the tab is still marked");
    assert_eq!(
        unresolved(cx, &shell, PLAIN),
        Some(0),
        "dev resolves {{host}}"
    );
    assert_eq!(
        reference_resolves(cx, &shell, PLAIN, edited, "host"),
        Some(true),
        "the draft's reference is styled as resolving now"
    );

    choose_environment(cx, None);
    assert_eq!(unresolved(cx, &shell, PLAIN), Some(1), "and back");
    assert_eq!(
        reference_resolves(cx, &shell, PLAIN, edited, "host"),
        Some(false)
    );
    assert!(
        draft_request(cx, &shell, PLAIN).is_some(),
        "the draft survives both changes"
    );
    assert!(marker(cx, PLAIN));
}

/// 2. A send is in flight when the environment changes: the environment change does not
/// cancel it and the send still completes. The environment is changed through the shell,
/// as request_tests do: pumping the executor between the dispatch and the change would
/// wait out the in-flight prepare and hide the crossing.
#[gpui::test]
fn an_environment_change_during_a_send_leaves_it_alone(cx: &mut TestAppContext) {
    let dir = workspace(serve(Server::Ok));
    write(
        dir.path(),
        "collections/api/plain.pre.js",
        &busy_script(400),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    // Dispatch without waiting, so the send is in flight while the environment changes.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.send_active(window, cx));
    });
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    shell.update(cx, |shell, cx| {
        shell.set_environment(Some("dev".to_owned()), cx);
    });
    assert_eq!(active_environment(cx, &shell).as_deref(), Some("dev"));
    assert_eq!(
        response(cx, &shell, PLAIN).as_deref(),
        Some("Sending"),
        "the environment change did not cancel the send"
    );
    assert_eq!(
        settle(cx, &shell, PLAIN).as_deref(),
        Some("200 OK"),
        "the send still completes"
    );
}

/// 3. A drafted tab with a cell edit is sent, then Save is pressed while the send is in
/// flight: the save writes both edits and the response still lands.
#[gpui::test]
fn saving_while_a_drafted_send_is_in_flight_keeps_the_response(cx: &mut TestAppContext) {
    // Long enough that the send is still in flight at the save on a loaded machine (300 ms
    // was not: a full parallel build made the response land first); `settle` waits up to 10 s.
    let port = slow_server(Duration::from_millis(2000));
    let dir = workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let saved = format!("http://127.0.0.1:{port}/saved");
    edit_url(cx, &saved);
    click(cx, "request-headers-value-1");
    type_cell(cx, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(marker(cx, PLAIN), "the URL and cell edits mark the tab");

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(!marker(cx, PLAIN), "the save wrote the draft");
    assert!(
        !cx.update(|_, cx| shell.read(cx).drafts.contains_key(PLAIN)),
        "the draft is gone"
    );
    let file = std::fs::read_to_string(dir.path().join("collections/api/plain.req.toml"))
        .unwrap_or_default();
    assert!(
        file.contains("/saved") && file.contains("42"),
        "both edits were written: {file}"
    );

    assert_eq!(
        settle(cx, &shell, PLAIN).as_deref(),
        Some("200 OK"),
        "the response still lands after the save"
    );
    assert!(
        bounds(cx, "response-status").is_some(),
        "the response shows"
    );
}

/// 4. Closing a drafted tab mid-flight cancels the send and keeps the draft: a draft is
/// unsaved work keyed by workspace and entity, not by an open tab, so the close drops the
/// send and the response but the draft stays in the app state, and reopening the request
/// shows the draft again, marked as unsaved.
#[gpui::test]
fn closing_a_drafted_tab_mid_flight_cancels_and_keeps_the_draft(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    assert!(marker(cx, PLAIN), "the tab is marked");
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    click(cx, &format!("doc-tab-close-{PLAIN}"));
    assert!(
        !cx.update(|_, cx| shell.read(cx).sends.contains_key(PLAIN)),
        "the send is cancelled with the tab"
    );
    let draft = draft_request(cx, &shell, PLAIN).expect("the draft survives the close");
    assert_eq!(draft.url, edited, "the kept draft holds what was typed");
    assert_eq!(
        stored_drafts(&state_path),
        vec![PLAIN.to_owned()],
        "the app-state row survives the close, which is what a restart restores"
    );

    click(cx, &format!("tree-row-{PLAIN}"));
    assert!(marker(cx, PLAIN), "the reopened tab is marked unsaved");
    assert_eq!(
        displayed_url(cx, &shell, PLAIN).as_deref(),
        Some(edited.as_str()),
        "reopening shows the draft, not the file"
    );
}

/// Whether the URL field of the request `id` is open: what the shell holds, as the
/// sibling tests read the open cell field.
fn url_edit_of(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> bool {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .url_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
    })
}

/// 5. Switching tabs while a URL field is open keeps the shell's keys working afterwards:
/// Ctrl+Enter sends the newly active request, and again after switching back.
#[gpui::test]
fn switching_tabs_with_the_url_field_open_keeps_the_send_keys(cx: &mut TestAppContext) {
    let dir = workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    click(cx, "request-url");
    assert!(url_edit_of(cx, &shell, PLAIN), "the field is open");

    click(cx, &format!("tree-row-{SECOND}"));
    assert_eq!(active_document(cx, &shell).as_deref(), Some(SECOND));
    assert!(
        !url_edit_of(cx, &shell, PLAIN),
        "the field went with the old tab"
    );

    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(
        settle(cx, &shell, SECOND).as_deref(),
        Some("200 OK"),
        "Ctrl+Enter still sends after switching tabs"
    );

    click(cx, &format!("doc-tab-{PLAIN}"));
    assert_eq!(active_document(cx, &shell).as_deref(), Some(PLAIN));
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(
        settle(cx, &shell, PLAIN).as_deref(),
        Some("200 OK"),
        "and still after switching back"
    );
}

/// 6. A request whose pre-request script writes a variable that the URL uses: the window
/// shows the URL from the draft, and the send resolves it with the written value.
#[gpui::test]
fn a_pre_request_write_resolves_the_drafted_url(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port);
    write(
        dir.path(),
        "collections/api/plain.pre.js",
        "pm.variables.set(\"written\", \"from-script\");\n",
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    edit_url(cx, "{{base}}/{{written}}");
    assert_eq!(
        displayed_url(cx, &shell, PLAIN).as_deref(),
        Some("{{base}}/{{written}}"),
        "the window shows the URL from the draft"
    );

    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PLAIN).as_deref(), Some("200 OK"));
    let sent = cx.update(|_, cx| {
        shell
            .read(cx)
            .last_send
            .as_ref()
            .filter(|last| last.id == PLAIN)
            .map(|last| (last.method.clone(), last.path.clone()))
    });
    assert_eq!(
        sent,
        Some(("GET".to_owned(), "/from-script".to_owned())),
        "the send resolved the draft's URL with the written value"
    );
    let file = std::fs::read_to_string(dir.path().join("collections/api/plain.req.toml"))
        .unwrap_or_default();
    assert!(
        file.contains("{{base}}/{{host}}/ok"),
        "the file was not sent and not touched: {file}"
    );
}

/// 7. A response whose test script fails, followed by a save: the failing tests stay
/// visible in the panel and the save does not touch them.
#[gpui::test]
fn a_save_keeps_a_failing_tests_panel(cx: &mut TestAppContext) {
    const FAILING_TEST: &str =
        "pm.test(\"wrong code\", function () {\n  pm.expect(200, \"bad code\").to.eql(404);\n});\n";
    let port = serve(Server::Ok);
    let dir = workspace(port);
    write(dir.path(), "collections/api/plain.test.js", FAILING_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    edit_url(cx, &format!("http://127.0.0.1:{port}/edited"));
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PLAIN).as_deref(), Some("200 OK"));
    click(cx, "response-tab-tests");
    present(cx, "response-test-1");
    present(cx, "response-test-1-message");

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(!marker(cx, PLAIN), "the save wrote the draft");
    assert!(!cx.update(|_, cx| shell.read(cx).drafts.contains_key(PLAIN)));
    let file = std::fs::read_to_string(dir.path().join("collections/api/plain.req.toml"))
        .unwrap_or_default();
    assert!(file.contains("/edited"), "the file was written: {file}");

    present(cx, "response-tab-tests");
    present(cx, "response-test-1");
    present(cx, "response-test-1-message");
}
