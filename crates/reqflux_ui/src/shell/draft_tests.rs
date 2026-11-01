//! The draft of an edited request: the URL field (S7), the dirty tab marker (S2) and the
//! draft's persistence in the app state (S1). Saving, reverting and the conflict live in
//! the save tests below.

use std::path::{Path, PathBuf};

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::send_tests::{Server, closed_port, response, serve};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000041";
const PLAIN: &str = "01J00000000000000000000042";

/// A workspace whose collection `API` declares `base` pointing at `port` and holds
/// `GET Plain` at `{{base}}/ok`.
fn draft_workspace(port: u16) -> TempDir {
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
            "id = \"{PLAIN}\"\nname = \"Plain\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/ok\"\n"
        ),
    );
    dir
}

/// A temp file for the app state, held alive for the test.
/// A temp file for the app state, held alive for the test.
pub(crate) fn state_file() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = dir.path().join("state.sqlite3");
    (dir, path)
}

pub(crate) fn render<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    // The engine wakes the awaiting task from its own threads.
    cx.executor().allow_parking();
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    cx.update(|cx| {
        bolt_legacy::install(crate::brand::reqflux(), bolt_legacy::PaletteMode::Dark, cx)
    });
    cx.update(super::bind_keys);
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::with_state(workspace, Some(state), cx));
    cx.run_until_parked();
    (shell, cx)
}

pub(crate) fn bounds(
    cx: &mut VisualTestContext,
    selector: &str,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

pub(crate) fn present(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    match bounds(cx, selector) {
        Some(found) => found,
        None => panic!("{selector} is missing"),
    }
}

pub(crate) fn click(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

/// Replaces the text of the URL field with `url`: opens the field over the read-only URL,
/// selects all and types.
pub(super) fn edit_url(cx: &mut VisualTestContext, url: &str) {
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

/// The URL of the draft of the request `id`, if it has one.
fn draft_url(cx: &mut TestAppContext, shell: &Entity<Shell>, id: &str) -> Option<String> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(id)
            .map(|draft| draft.request.url.clone())
    })
}

pub(crate) fn marker(cx: &mut VisualTestContext, id: &str) -> bool {
    bounds(cx, &format!("doc-tab-dirty-{id}")).is_some()
}

#[gpui::test]
fn typing_marks_the_draft_and_persists_it(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    assert!(!marker(cx, PLAIN), "no marker without a draft");
    assert!(draft_url(cx, &shell, PLAIN).is_none(), "no draft yet");

    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    assert!(marker(cx, PLAIN), "typing marks the draft");
    assert_eq!(
        draft_url(cx, &shell, PLAIN).as_deref(),
        Some(edited.as_str())
    );

    // A second shell over the same workspace and state file finds the draft again.
    let (_shell_2, cx) = render(cx, &dir, &state_path);
    assert_eq!(
        draft_url(cx, &_shell_2, PLAIN).as_deref(),
        Some(edited.as_str()),
        "the draft survives a restart"
    );
}

#[gpui::test]
fn enter_commits_and_returns_to_the_read_only_line(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let edited = format!("http://127.0.0.1:{port}/committed");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(
        bounds(cx, "request-url-input").is_none(),
        "the field is gone"
    );
    assert!(bounds(cx, "request-url").is_some(), "the read-only line");
    assert_eq!(
        draft_url(cx, &shell, PLAIN).as_deref(),
        Some(edited.as_str())
    );
    assert!(marker(cx, PLAIN), "the draft is still there");
}

#[gpui::test]
fn escape_in_the_field_discards_the_text_and_keeps_the_send(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    let edited = format!("http://127.0.0.1:{port}/discarded");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(
        bounds(cx, "request-url-input").is_none(),
        "the field is gone"
    );
    assert!(draft_url(cx, &shell, PLAIN).is_none(), "no draft is left");
    assert!(!marker(cx, PLAIN), "no marker without a draft");
    assert_eq!(
        response(cx, &shell, PLAIN).as_deref(),
        Some("Sending"),
        "Escape in the field does not cancel the send"
    );
}

#[gpui::test]
fn escape_with_no_field_focused_cancels_the_send(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    cx.simulate_keystrokes("escape");
    assert_eq!(
        super::send_tests::settle(cx, &shell, PLAIN).as_deref(),
        Some("Failed: request cancelled")
    );
}

/// The file of `GET Plain`, rewritten to `url`, as another writer would.
fn write_plain(root: &Path, url: &str) {
    write(
        root,
        "collections/api/plain.req.toml",
        &format!("id = \"{PLAIN}\"\nname = \"Plain\"\nmethod = \"GET\"\nurl = \"{url}\"\n"),
    );
}

#[gpui::test]
fn ctrl_s_writes_the_file_and_clears_the_marker(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let saved = format!("http://127.0.0.1:{port}/saved");
    edit_url(cx, &saved);
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    assert!(!marker(cx, PLAIN), "the tab loses its marker");
    assert!(draft_url(cx, &shell, PLAIN).is_none(), "the draft is gone");
    let shown = cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(PLAIN)
            .map(|state| state.inspection.url.clone())
    });
    assert_eq!(shown.as_deref(), Some(saved.as_str()), "the view reloaded");
    let file = std::fs::read_to_string(dir.path().join("collections/api/plain.req.toml"))
        .unwrap_or_default();
    assert!(file.contains("/saved"), "the file was written: {file}");
    let state = match AppState::open(&state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    let stored = state
        .drafts("01J00000000000000000000900")
        .unwrap_or_default();
    assert!(stored.is_empty(), "the store's draft is gone");
}

#[gpui::test]
fn revert_clears_the_marker_and_restores_the_saved_request(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    edit_url(cx, &format!("http://127.0.0.1:{port}/discarded"));
    click(cx, "request-action-revert");

    assert!(!marker(cx, PLAIN), "the tab loses its marker");
    assert!(draft_url(cx, &shell, PLAIN).is_none(), "the draft is gone");
    let shown = cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(PLAIN)
            .map(|state| state.inspection.url.clone())
    });
    assert_eq!(shown.as_deref(), Some("{{base}}/ok"), "as the file has it");
}

#[gpui::test]
fn a_file_edited_behind_the_draft_makes_save_conflict(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    // Another writer moves the file while the draft is open.
    write_plain(dir.path(), &format!("http://127.0.0.1:{port}/moved"));

    click(cx, "request-action-save");
    assert!(
        bounds(cx, &format!("doc-tab-conflict-{PLAIN}")).is_some(),
        "the tab says the saved version moved"
    );
    assert!(marker(cx, PLAIN), "the draft is kept");
    assert_eq!(
        draft_url(cx, &shell, PLAIN).as_deref(),
        Some(edited.as_str())
    );

    click(cx, "request-action-revert");
    assert!(
        bounds(cx, &format!("doc-tab-conflict-{PLAIN}")).is_none(),
        "the conflict label is gone"
    );
    assert!(draft_url(cx, &shell, PLAIN).is_none());
    let moved = format!("http://127.0.0.1:{port}/moved");
    let shown = cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(PLAIN)
            .map(|state| state.inspection.url.clone())
    });
    assert_eq!(shown.as_deref(), Some(moved.as_str()));

    // Saving after reverting succeeds.
    let again = format!("http://127.0.0.1:{port}/again");
    edit_url(cx, &again);
    click(cx, "request-action-save");
    assert!(!marker(cx, PLAIN));
    let file = std::fs::read_to_string(dir.path().join("collections/api/plain.req.toml"))
        .unwrap_or_default();
    assert!(file.contains("/again"), "the file was written: {file}");
}

#[gpui::test]
fn sending_a_drafted_request_sends_the_draft(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    edit_url(cx, &format!("http://127.0.0.1:{port}/drafted"));
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    let path = cx.update(|_, cx| {
        shell
            .read(cx)
            .last_send
            .as_ref()
            .filter(|last| last.id == PLAIN)
            .map(|last| last.path.clone())
    });
    assert_eq!(path.as_deref(), Some("/drafted"), "the draft was sent");
}

/// S9 of send-response with S3 of request-editing: saving a draft reloads the workspace, and
/// that must not kill a send that is in flight.
#[gpui::test]
fn saving_a_draft_keeps_the_send_in_flight(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PLAIN).as_deref(), Some("Sending"));

    edit_url(cx, &format!("http://127.0.0.1:{port}/saved"));
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(
        response(cx, &shell, PLAIN).as_deref(),
        Some("Sending"),
        "the in-flight send survives the save"
    );
}

/// S12 of request-editing with S1 of send-response: Ctrl+Enter in an open cell field commits
/// the typed text before sending, and the field closes.
#[gpui::test]
fn ctrl_enter_in_the_cell_field_sends_the_typed_text(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = draft_workspace(port);
    write(
        dir.path(),
        "collections/api/plain.req.toml",
        &format!(
            "id = \"{PLAIN}\"\nname = \"Plain\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:{port}/ok\"\n\n\
             [[headers]]\nkey = \"X-One\"\nvalue = \"1\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    click(cx, "request-tab-headers");
    click(cx, "request-headers-value-1");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_keystrokes("4 2");
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    let (sent, drafted, open) = cx.update(|_, cx| {
        let shell = shell.read(cx);
        (
            shell.sends.contains_key(PLAIN),
            shell.drafts.contains_key(PLAIN),
            shell.cell_edit.is_some(),
        )
    });
    assert!(sent, "Ctrl+Enter sends");
    assert!(drafted, "the typed cell text is part of what was sent");
    assert!(!open, "the cell field closed when the send started");
}
