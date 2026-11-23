//! Opening a history entry as an unsaved copy (history-copy-and-scratch S2): an editable,
//! sendable scratch tab that never changes the source request, its file or its draft, and
//! becomes a request of its own only through Save as.

use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::Shell;
use super::draft_tests::{bounds, click, edit_url, present, render, state_file};
use super::history_tests::{PING, history_workspace, open_top_row, send};
use super::send_tests::{Server, serve, settle};

const SCRATCH: &str = "scratch-1";

fn ping_file(dir: &Path) -> Vec<u8> {
    match std::fs::read(dir.join("collections/api/ping.req.toml")) {
        Ok(bytes) => bytes,
        Err(error) => panic!("read ping: {error}"),
    }
}

fn active(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).documents.active().map(str::to_owned))
}

fn drafts(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| shell.read(cx).drafts.keys().cloned().collect())
}

fn stored_drafts(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> usize {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        match (shell.state.as_ref(), shell.workspace_id.as_deref()) {
            (Some(state), Some(workspace)) => {
                state.drafts(workspace).map_or(usize::MAX, |d| d.len())
            }
            _ => usize::MAX,
        }
    })
}

fn history_urls(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<(String, String)> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        match (shell.state.as_ref(), shell.workspace_id.as_deref()) {
            (Some(state), Some(workspace)) => state
                .history(workspace, 100)
                .map(|entries| {
                    entries
                        .into_iter()
                        .map(|e| (e.record.request_id, e.record.sent.url))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    })
}

#[gpui::test]
fn the_copy_opens_in_a_scratch_tab_and_leaves_the_source_alone(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let before = ping_file(dir.path());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);

    click(cx, "history-open-copy");
    present(cx, &format!("doc-tab-{SCRATCH}"));
    present(cx, &format!("doc-tab-close-{SCRATCH}"));
    present(cx, &format!("doc-tab-unsaved-{SCRATCH}"));
    present(cx, "request-send");
    assert!(bounds(cx, "history-readonly").is_none());
    assert_eq!(active(cx, &shell).as_deref(), Some(SCRATCH));
    // The source tab stays open, with no draft; only the copy has one.
    present(cx, &format!("doc-tab-{PING}"));
    assert_eq!(drafts(cx, &shell), vec![SCRATCH.to_owned()]);
    assert_eq!(
        stored_drafts(cx, &shell),
        0,
        "a scratch draft is never stored"
    );
    assert_eq!(ping_file(dir.path()), before);
}

#[gpui::test]
fn editing_and_sending_the_copy_records_under_the_source(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = history_workspace(port);
    let before = ping_file(dir.path());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    click(cx, "history-open-copy");

    edit_url(cx, &format!("http://127.0.0.1:{port}/changed"));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, SCRATCH).as_deref(), Some("200 OK"));

    let recorded = history_urls(cx, &shell);
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].0, PING);
    assert!(
        recorded[0].1.ends_with("/changed"),
        "sent {:?}",
        recorded[0].1
    );
    assert_eq!(drafts(cx, &shell), vec![SCRATCH.to_owned()]);
    assert_eq!(stored_drafts(cx, &shell), 0);
    assert_eq!(ping_file(dir.path()), before);
}

#[gpui::test]
fn save_as_creates_a_request_beside_the_source(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = history_workspace(port);
    let before = ping_file(dir.path());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    click(cx, "history-open-copy");
    edit_url(cx, &format!("http://127.0.0.1:{port}/changed"));

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let created =
        match std::fs::read_to_string(dir.path().join("collections/api/ping-copy.req.toml")) {
            Ok(text) => text,
            Err(error) => panic!("the copy was not written: {error}"),
        };
    assert!(created.contains("name = \"Ping copy\""), "{created}");
    assert!(created.contains("/changed"), "{created}");
    assert_eq!(ping_file(dir.path()), before);
    assert!(bounds(cx, &format!("doc-tab-{SCRATCH}")).is_none());
    assert!(drafts(cx, &shell).is_empty());
    let id = active(cx, &shell).unwrap_or_default();
    assert_ne!(id, PING);
    assert_ne!(id, SCRATCH);
    present(cx, &format!("doc-tab-{id}"));
    present(cx, &format!("doc-tab-{PING}"));
}

#[gpui::test]
fn the_button_is_disabled_once_the_source_is_deleted(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, &format!("tree-row-menu-{PING}"));
    click(cx, "tree-menu-delete");
    click(cx, "tree-delete-confirm");
    click(cx, "rail-history");
    click(cx, "history-row-1");

    present(cx, "history-open-copy");
    present(cx, "history-open-copy-disabled");
    present(cx, "history-resend-note");
    click(cx, "history-open-copy");
    assert!(bounds(cx, &format!("doc-tab-{SCRATCH}")).is_none());
    assert!(drafts(cx, &shell).is_empty());
}

#[gpui::test]
fn closing_the_scratch_tab_drops_it(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let before = ping_file(dir.path());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    click(cx, "history-open-copy");

    click(cx, &format!("doc-tab-close-{SCRATCH}"));
    assert!(bounds(cx, &format!("doc-tab-{SCRATCH}")).is_none());
    assert!(drafts(cx, &shell).is_empty());
    assert_eq!(active(cx, &shell).as_deref(), Some(PING));
    assert_eq!(ping_file(dir.path()), before);
}
