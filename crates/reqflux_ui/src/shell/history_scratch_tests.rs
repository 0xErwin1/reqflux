//! Opening a history entry as an unsaved copy (history-copy-and-scratch S2): an editable,
//! sendable scratch tab that never changes the source request, its file or its draft, and
//! becomes a request of its own only through Save as.

use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::Shell;
use super::draft_tests::{bounds, click, edit_url, present, render, state_file};
use super::history_tests::{PING, history_workspace, open_top_row, send};
use super::send_tests::{Server, serve, settle};
use super::toasts::tests::{error, pushed};
use crate::workspace_tests::write;

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
    let scratch = active(cx, &shell).unwrap_or_default();
    assert!(scratch.starts_with("scratch-"), "active {scratch:?}");
    present(cx, &format!("doc-tab-{scratch}"));
    present(cx, &format!("doc-tab-close-{scratch}"));
    present(cx, &format!("doc-tab-unsaved-{scratch}"));
    present(cx, "request-send");
    assert!(bounds(cx, "history-readonly").is_none());
    assert_eq!(active(cx, &shell).as_deref(), Some(scratch.as_str()));
    // The source tab stays open, with no draft; only the copy has one.
    present(cx, &format!("doc-tab-{PING}"));
    assert_eq!(drafts(cx, &shell), vec![scratch.clone()]);
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
    let scratch = active(cx, &shell).unwrap_or_default();
    assert!(scratch.starts_with("scratch-"), "active {scratch:?}");

    edit_url(cx, &format!("http://127.0.0.1:{port}/changed"));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, &scratch).as_deref(), Some("200 OK"));

    let recorded = history_urls(cx, &shell);
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].0, PING);
    assert!(
        recorded[0].1.ends_with("/changed"),
        "sent {:?}",
        recorded[0].1
    );
    assert_eq!(drafts(cx, &shell), vec![scratch.clone()]);
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
    let scratch = active(cx, &shell).unwrap_or_default();
    assert!(scratch.starts_with("scratch-"), "active {scratch:?}");
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
    assert!(bounds(cx, &format!("doc-tab-{scratch}")).is_none());
    assert!(drafts(cx, &shell).is_empty());
    let id = active(cx, &shell).unwrap_or_default();
    assert_ne!(id, PING);
    assert_ne!(id, scratch);
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
    let scratch = active(cx, &shell).unwrap_or_default();
    assert!(scratch.starts_with("scratch-"), "active {scratch:?}");

    click(cx, &format!("doc-tab-close-{scratch}"));
    assert!(bounds(cx, &format!("doc-tab-{scratch}")).is_none());
    assert!(drafts(cx, &shell).is_empty());
    assert_eq!(active(cx, &shell).as_deref(), Some(PING));
    assert_eq!(ping_file(dir.path()), before);
}

const TAKEN: &str = "01J00000000000000000000088";

/// The history workspace plus `GET <name>` as a sibling of `Ping`.
fn with_sibling(port: u16, name: &str, file: &str) -> tempfile::TempDir {
    let dir = history_workspace(port);
    write(
        dir.path(),
        &format!("collections/api/{file}.req.toml"),
        &format!("id = \"{TAKEN}\"\nname = \"{name}\"\nmethod = \"GET\"\nurl = \"\"\n"),
    );
    dir
}

fn save_as_copy(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    assert_eq!(send(cx, shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    click(cx, "history-open-copy");
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
}

fn read_in(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join("collections/api").join(file)).ok()
}

#[gpui::test]
fn save_as_numbers_the_name_when_the_copy_name_exists(cx: &mut TestAppContext) {
    let dir = with_sibling(serve(Server::Ok), "Ping copy", "ping-copy");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    save_as_copy(cx, &shell);
    let created = read_in(dir.path(), "ping-copy-2.req.toml").unwrap_or_default();
    assert!(created.contains("name = \"Ping copy 2\""), "{created}");
    assert!(drafts(cx, &shell).is_empty());
}

#[gpui::test]
fn save_as_skips_a_name_whose_slug_clashes_by_punctuation(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = with_sibling(port, "Ping copy", "ping-copy");
    // The source name has punctuation: "Ping! copy" slugs to `ping-copy`, which is taken.
    write(
        dir.path(),
        "collections/api/ping.req.toml",
        &format!(
            "id = \"{PING}\"\nname = \"Ping!\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:{port}/ping\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    save_as_copy(cx, &shell);
    let created = read_in(dir.path(), "ping-copy-2.req.toml").unwrap_or_default();
    assert!(created.contains("name = \"Ping! copy 2\""), "{created}");
    assert!(drafts(cx, &shell).is_empty(), "the copy was saved");
}

#[gpui::test]
fn save_as_writes_the_scripts_the_copy_ran(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    write(dir.path(), "collections/api/ping.pre.js", "// warm up\n");
    write(dir.path(), "collections/api/ping.test.js", "// check\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    save_as_copy(cx, &shell);
    assert_eq!(
        read_in(dir.path(), "ping-copy.pre.js").as_deref(),
        Some("// warm up\n")
    );
    assert_eq!(
        read_in(dir.path(), "ping-copy.test.js").as_deref(),
        Some("// check\n")
    );
    assert_eq!(
        read_in(dir.path(), "ping.pre.js").as_deref(),
        Some("// warm up\n")
    );
}

#[gpui::test]
fn a_copy_stays_openable_after_its_source_is_deleted(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    click(cx, "history-open-copy");
    let scratch = active(cx, &shell).unwrap_or_default();

    click(cx, "rail-collections");
    click(cx, &format!("tree-row-{PING}"));
    click(cx, &format!("tree-row-menu-{PING}"));
    click(cx, "tree-menu-delete");
    click(cx, "tree-delete-confirm");
    assert_eq!(drafts(cx, &shell), vec![scratch.clone()]);

    // Activating the copy again (the open path every tab entry point shares) still works.
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.open_document(&scratch, cx)));
    cx.run_until_parked();
    assert_eq!(active(cx, &shell).as_deref(), Some(scratch.as_str()));
    present(cx, &format!("doc-tab-{scratch}"));
    present(cx, "request-send");
    assert!(pushed(cx).is_empty(), "no failure toast: {:?}", pushed(cx));
}

#[gpui::test]
fn opening_a_copy_without_its_recorded_send_says_so(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    open_top_row(cx);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            if let Some(key) = shell.orphan.as_ref().map(|tab| tab.key()) {
                shell.sends.remove(&key);
            }
        });
    });

    click(cx, "history-open-copy");
    assert_eq!(
        pushed(cx).last(),
        Some(&error(
            "Cannot open this entry",
            "The recorded request is no longer available."
        ))
    );
    assert!(drafts(cx, &shell).is_empty());
}
