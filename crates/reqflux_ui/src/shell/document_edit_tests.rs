//! The Document layout's edit beyond one sitting (v1-close S5b-S5d): the unsaved text of a
//! folder survives a folder switch and a restart, Escape cancels the send a section
//! started, and a section keeps its request across an unsaved rename, so its Retry and
//! its response block still find it.

use std::sync::atomic::Ordering;

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::tests::{
    CANONICAL, CREATE, FOLDER, LIST, folder_workspace, open_send_editor, refusing_server,
    section_document, send_workspace, set_cursor, set_document_text,
};
use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::{Server, closed_port, response, serve, settle};
use crate::shell::toasts::Kind;
use crate::shell::toasts::tests::pushed;
use crate::workspace_tests::write;

/// A request directly in the collection, beside the `invoices` folder, so opening it shows
/// another document.
const PING: &str = "01JF0000000000000000000005";

/// The folder workspace with `GET Ping` at the collection's top level.
fn two_folder_workspace() -> TempDir {
    let dir = folder_workspace();
    write(
        dir.path(),
        "collections/api/ping.req.toml",
        &format!(
            "id = \"{PING}\"\nname = \"Ping\"\nmethod = \"GET\"\nurl = \"https://api.test/ping\"\n"
        ),
    );
    dir
}

/// Shows the folder of `LIST` in the Document layout and opens its editor.
fn show_list_folder(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{FOLDER}-chevron"));
    click(cx, &format!("tree-row-{LIST}"));
    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    if bounds(cx, "document-editor").is_none() {
        click(cx, "document-text");
    }
    present(cx, "document-editor");
}

/// The text of the open document editor, if one is open.
fn editor_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .document_edit
            .as_ref()
            .map(|edit| edit.input.read(cx).value().to_string())
    })
}

/// The canonical document with `List` pointed at another URL: an unsaved edit.
fn edited() -> String {
    CANONICAL.replace(
        "GET https://api.test/v1/invoices\n",
        "GET https://api.test/v1/invoices?page=2\n",
    )
}

#[gpui::test]
fn switching_folders_and_back_restores_the_edited_text(cx: &mut TestAppContext) {
    let dir = two_folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);
    set_document_text(cx, &shell, &edited());

    click(cx, &format!("tree-row-{PING}"));
    assert_ne!(
        editor_text(cx, &shell).as_deref(),
        Some(edited().as_str()),
        "another folder shows its own document"
    );
    click(cx, &format!("tree-row-{LIST}"));

    assert_eq!(
        editor_text(cx, &shell).as_deref(),
        Some(edited().as_str()),
        "the folder's unsaved text is back"
    );
    present(cx, "document-dirty");
}

#[gpui::test]
fn a_restart_restores_the_edited_text(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);
    set_document_text(cx, &shell, &edited());

    // A second shell over the same workspace and state file finds the text again.
    let (restarted, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);

    assert_eq!(
        editor_text(cx, &restarted).as_deref(),
        Some(edited().as_str()),
        "the unsaved text survives a restart"
    );
    present(cx, "document-dirty");
}

#[gpui::test]
fn discarding_forgets_the_stored_text(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);
    set_document_text(cx, &shell, &edited());
    click(cx, "document-discard");

    let (restarted, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);

    assert_eq!(
        editor_text(cx, &restarted).as_deref(),
        Some(CANONICAL),
        "a discarded text does not come back"
    );
    assert!(bounds(cx, "document-dirty").is_none());
}

#[gpui::test]
fn a_folder_changed_since_the_text_shows_it_is_stale(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);
    set_document_text(cx, &shell, &edited());
    write(
        dir.path(),
        "collections/api/invoices/list.req.toml",
        &format!(
            "id = \"{LIST}\"\nname = \"List\"\nmethod = \"GET\"\n\
             url = \"https://api.test/v2/invoices\"\n"
        ),
    );

    let (restarted, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);

    assert_eq!(
        editor_text(cx, &restarted).as_deref(),
        Some(edited().as_str()),
        "the unsaved text is kept"
    );
    present(cx, "document-stale");
    click(cx, "document-save");
    assert!(
        !std::fs::read_to_string(dir.path().join("collections/api/invoices/list.req.toml"))
            .unwrap_or_default()
            .contains("page"),
        "a stale text does not overwrite the folder"
    );
}

#[gpui::test]
fn escape_cancels_the_send_of_a_section_that_is_not_the_active_tab(cx: &mut TestAppContext) {
    let silent = serve(Server::Silent);
    let dir = send_workspace(closed_port(), silent);
    let (shell, cx) = open_send_editor(cx, &dir);
    // The cursor sits in `Create invoice`; `List` is the active tab.
    set_cursor(cx, &shell, 5);
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, CREATE).as_deref(), Some("Sending"));

    cx.simulate_keystrokes("escape");
    assert_eq!(
        settle(cx, &shell, CREATE).as_deref(),
        Some("Failed: request cancelled")
    );
}

#[gpui::test]
fn stacked_escape_still_cancels_only_the_active_requests_send(cx: &mut TestAppContext) {
    let silent = serve(Server::Silent);
    let dir = send_workspace(closed_port(), silent);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{FOLDER}-chevron"));
    click(cx, &format!("tree-row-{CREATE}"));
    click(cx, "request-send");
    cx.run_until_parked();
    click(cx, &format!("tree-row-{LIST}"));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        response(cx, &shell, CREATE).as_deref(),
        Some("Sending"),
        "Escape in Stacked leaves another tab's send alone"
    );

    click(cx, &format!("doc-tab-{CREATE}"));
    cx.simulate_keystrokes("escape");
    assert_eq!(
        settle(cx, &shell, CREATE).as_deref(),
        Some("Failed: request cancelled"),
        "Escape in Stacked cancels the active request's send"
    );
}

#[gpui::test]
fn retry_after_a_rename_resends_the_renamed_section(cx: &mut TestAppContext) {
    let (refused, count) = refusing_server();
    let dir = send_workspace(closed_port(), refused);
    let (shell, cx) = open_send_editor(cx, &dir);
    set_cursor(cx, &shell, 5);
    cx.simulate_keystrokes("ctrl-enter");
    let failed = settle(cx, &shell, CREATE).unwrap_or_default();
    assert!(failed.starts_with("Failed"), "unexpected {failed}");

    let renamed = section_document(closed_port(), refused)
        .replace("### Create invoice", "### Create draft invoice");
    set_document_text(cx, &shell, &renamed);
    set_cursor(cx, &shell, 5);
    click(cx, "document-response-retry");
    settle(cx, &shell, CREATE);

    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "the retry sent the renamed section"
    );
}

#[gpui::test]
fn the_response_block_stays_after_a_rename(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let refused = closed_port();
    let dir = send_workspace(port, refused);
    let (shell, cx) = open_send_editor(cx, &dir);
    let document = section_document(port, refused);
    set_cursor(
        cx,
        &shell,
        document.find("### List").unwrap_or_default() + 5,
    );
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, LIST).as_deref(), Some("200 OK"));

    let renamed = document.replace("### List", "### List all");
    set_document_text(cx, &shell, &renamed);
    set_cursor(
        cx,
        &shell,
        renamed.find("### List all").unwrap_or_default() + 5,
    );

    present(cx, "document-section-2-response");
    present(cx, "document-response-body");
}

#[gpui::test]
fn retry_of_a_deleted_section_says_so(cx: &mut TestAppContext) {
    let (refused, count) = refusing_server();
    let dir = send_workspace(closed_port(), refused);
    let (shell, cx) = open_send_editor(cx, &dir);
    set_cursor(cx, &shell, 5);
    cx.simulate_keystrokes("ctrl-enter");
    settle(cx, &shell, CREATE);

    let without = format!(
        "### List\nGET http://127.0.0.1:{}/v1/invoices\n",
        closed_port()
    );
    set_document_text(cx, &shell, &without);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.resend_document_section(CREATE, window, cx)
        });
    });
    cx.run_until_parked();

    assert_eq!(count.load(Ordering::SeqCst), 1, "nothing was sent again");
    let toasts = pushed(cx);
    assert!(
        toasts.iter().any(|toast| toast.kind == Kind::Error
            && toast.title == "Cannot retry Create invoice"
            && toast.text == "Its section is no longer in the document."),
        "{toasts:?}"
    );
}

#[gpui::test]
fn a_section_typed_above_another_never_sends_as_it(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let refused = closed_port();
    let dir = send_workspace(port, refused);
    let (shell, cx) = open_send_editor(cx, &dir);
    let document = section_document(port, refused);
    set_cursor(cx, &shell, document.find("### List").unwrap_or_default());
    cx.update(|window, cx| {
        let input = shell
            .read(cx)
            .document_edit
            .as_ref()
            .map(|edit| edit.input.clone());
        if let Some(input) = input {
            input.update(cx, |state, cx| state.focus(window, cx));
        }
    });
    cx.simulate_input("### New");
    cx.simulate_keystrokes("enter");
    cx.simulate_input(&format!("GET http://127.0.0.1:{port}/new"));
    cx.simulate_keystrokes("enter");
    let typed = editor_text(cx, &shell).unwrap_or_default();
    assert!(typed.contains("### New\nGET"), "{typed}");

    set_cursor(cx, &shell, typed.find("### New").unwrap_or_default() + 5);
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| shell.read(cx).sends.contains_key(LIST)),
        "the typed section was not sent as List"
    );

    set_cursor(cx, &shell, typed.find("### List").unwrap_or_default() + 5);
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(
        settle(cx, &shell, LIST).as_deref(),
        Some("200 OK"),
        "List still sends as List"
    );
}

#[gpui::test]
fn a_folder_reloaded_under_a_dirty_editor_marks_it_stale(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    show_list_folder(cx);
    set_document_text(cx, &shell, &edited());
    write(
        dir.path(),
        "collections/api/invoices/list.req.toml",
        &format!(
            "id = \"{LIST}\"\nname = \"List\"\nmethod = \"GET\"\n\
             url = \"https://api.test/v2/invoices\"\n"
        ),
    );
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| shell.reload_workspace(cx));
    });
    cx.run_until_parked();

    assert_eq!(
        editor_text(cx, &shell).as_deref(),
        Some(edited().as_str()),
        "the unsaved text is kept"
    );
    present(cx, "document-stale");
    click(cx, "document-save");
    let saved = std::fs::read_to_string(dir.path().join("collections/api/invoices/list.req.toml"))
        .unwrap_or_default();
    assert!(
        saved.contains("v2/invoices") && !saved.contains("page"),
        "a stale text does not overwrite the folder: {saved}"
    );
}
