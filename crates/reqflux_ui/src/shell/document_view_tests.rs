//! The Document layout's view (v1-features S7, design/layouts/02): the active request's
//! folder as one read-only `.http` document, the empty state without an open request, and
//! the switch Ctrl+Alt+3 makes: the split gives way to the document and comes back with
//! Ctrl+Alt+1. Stacked behavior itself stays `split_tests`'s and `layout_tests`'s.

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{
    PLAIN, bounds, click, draft_workspace, present, render, state_file,
};
use crate::shell::send_tests::{Server, closed_port, serve, settle};
use crate::shell::variables_tests::{COLLECTION as API_COLLECTION, USE, variables_workspace};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01JF0000000000000000000001";
const FOLDER: &str = "01JF0000000000000000000002";
const CREATE: &str = "01JF0000000000000000000003";
const LIST: &str = "01JF0000000000000000000004";

/// The canonical document the folder workspace shows: `Create invoice` and `List`.
const CANONICAL: &str = "### Create invoice\nPOST https://api.test/v1/invoices\nX-Api-Key: k\n\n{\"total\": 42}\n\n### List\nGET https://api.test/v1/invoices\n";

/// The text of `relative` under the workspace `dir`.
fn read_file(dir: &TempDir, relative: &str) -> String {
    match std::fs::read_to_string(dir.path().join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("read {relative}: {error}"),
    }
}

/// Opens the folder of `LIST` in the Document layout with the editor open.
fn open_editor<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, dir, &state_path);
    click(cx, &format!("tree-chevron-{FOLDER}"));
    click(cx, &format!("tree-row-{LIST}"));
    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    click(cx, "document-text");
    present(cx, "document-editor");
    (shell, cx)
}

/// Puts `text` into the open document editor, the way typing does.
fn set_document_text(cx: &mut VisualTestContext, shell: &Entity<Shell>, text: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.set_document_text(text, window, cx));
    });
}

/// A workspace whose collection `API` holds the folder `invoices` with `POST Create
/// invoice` (one header, a JSON body) and `GET List`, in store order.
fn folder_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/invoices/folder.toml",
        &format!("id = \"{FOLDER}\"\nname = \"invoices\"\n"),
    );
    write(
        root,
        "collections/api/invoices/create.req.toml",
        &format!(
            "id = \"{CREATE}\"\nname = \"Create invoice\"\nmethod = \"POST\"\n\
             url = \"https://api.test/v1/invoices\"\n\n\
             [[headers]]\nkey = \"X-Api-Key\"\nvalue = \"k\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{\\\"total\\\": 42}}\"\n"
        ),
    );
    write(
        root,
        "collections/api/invoices/list.req.toml",
        &format!(
            "id = \"{LIST}\"\nname = \"List\"\nmethod = \"GET\"\n\
             url = \"https://api.test/v1/invoices\"\n"
        ),
    );
    dir
}

/// The accessible label of element `id`, if it is shown.
fn label_of(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    bounds(cx, id)?;
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

#[gpui::test]
fn the_document_shows_the_active_requests_folder(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{USE}"));

    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    present(cx, "document-view");
    assert_eq!(
        label_of(cx, "document-line-1").as_deref(),
        Some("### Use"),
        "the section line carries the request's name"
    );
    assert_eq!(
        label_of(cx, "document-line-2").as_deref(),
        Some("GET {{base}}/use/{{greeting}}"),
        "the method line carries the method and the URL as written"
    );
    assert!(bounds(cx, "document-section-1").is_some(), "one section");
}

#[gpui::test]
fn a_folder_shows_its_requests_in_tree_order(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-chevron-{FOLDER}"));
    click(cx, &format!("tree-row-{LIST}"));

    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    present(cx, "document-view");
    assert_eq!(
        label_of(cx, "document-line-1").as_deref(),
        Some("### Create invoice"),
        "the folder's first request opens the document"
    );
    assert_eq!(
        label_of(cx, "document-line-2").as_deref(),
        Some("POST https://api.test/v1/invoices")
    );
    assert_eq!(
        label_of(cx, "document-line-3").as_deref(),
        Some("X-Api-Key: k")
    );
    assert_eq!(
        label_of(cx, "document-line-5").as_deref(),
        Some("{\"total\": 42}"),
        "the body follows the blank line"
    );
    assert_eq!(
        label_of(cx, "document-line-7").as_deref(),
        Some("### List"),
        "the second request follows in tree order"
    );
    assert_eq!(
        label_of(cx, "document-line-8").as_deref(),
        Some("GET https://api.test/v1/invoices")
    );
    assert!(bounds(cx, "document-section-1").is_some(), "section one");
    assert!(bounds(cx, "document-section-2").is_some(), "section two");
}

#[gpui::test]
fn without_a_request_the_document_shows_an_empty_state(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    // Focus a tree row first: the keystroke reaches the window like any other click does.
    click(cx, &format!("tree-row-{API_COLLECTION}"));

    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    present(cx, "document-view");
    assert!(
        bounds(cx, "document-line-1").is_none(),
        "no lines without an open request"
    );
}

#[gpui::test]
fn ctrl_alt_3_shows_the_document_and_hides_the_split(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    cx.simulate_keystrokes("ctrl-enter");
    assert!(settle(cx, &shell, PLAIN).is_some(), "the send finished");
    assert!(
        bounds(cx, "request-response-split").is_some(),
        "the split is there in Stacked"
    );

    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    present(cx, "document-view");
    assert!(
        bounds(cx, "document-line-1").is_some(),
        "the document shows the folder"
    );
    assert!(
        bounds(cx, "request-response-split").is_none(),
        "the gutter is gone"
    );
    assert!(
        bounds(cx, "response-area").is_none(),
        "the response is not shown at all"
    );
}

#[gpui::test]
fn ctrl_alt_1_returns_to_the_split(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    cx.simulate_keystrokes("ctrl-enter");
    assert!(settle(cx, &shell, PLAIN).is_some(), "the send finished");

    cx.simulate_keystrokes("ctrl-alt-3");
    cx.run_until_parked();
    present(cx, "document-view");

    cx.simulate_keystrokes("ctrl-alt-1");
    cx.run_until_parked();
    present(cx, "request-response-split");
    assert!(
        bounds(cx, "response-area").is_some(),
        "the response is back"
    );
    assert!(
        bounds(cx, "document-view").is_none(),
        "the document is gone"
    );
}

#[gpui::test]
fn stacked_still_shows_the_request_pane_alone_before_a_send(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{USE}"));

    present(cx, "request-area");
    assert!(
        bounds(cx, "request-response-split").is_none(),
        "a request never sent shows no gutter"
    );
    assert!(
        bounds(cx, "response-area").is_none(),
        "a request never sent shows no response"
    );
    assert!(
        bounds(cx, "document-view").is_none(),
        "Stacked does not show the document"
    );
}

#[gpui::test]
fn editing_a_url_and_saving_rewrites_the_requests_file(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (shell, cx) = open_editor(cx, &dir);
    assert!(
        bounds(cx, "document-dirty").is_none(),
        "the seeded text is not dirty"
    );
    let edited = CANONICAL.replace(
        "### List\nGET https://api.test/v1/invoices\n",
        "### List\nGET https://api.test/v1/invoices?limit=50\n",
    );
    set_document_text(cx, &shell, &edited);
    present(cx, "document-dirty");

    click(cx, "document-save");
    cx.run_until_parked();
    let file = read_file(&dir, "collections/api/invoices/list.req.toml");
    assert!(
        file.contains("url = \"https://api.test/v1/invoices\"\n"),
        "the URL is stored without its query: {file}"
    );
    assert!(!file.contains("limit=50"), "the query left the URL");
    assert!(
        file.contains("key = \"limit\"\nvalue = \"50\""),
        "the query string became a param: {file}"
    );
    assert!(
        bounds(cx, "document-dirty").is_none() && bounds(cx, "document-editor").is_none(),
        "saving clears the edit"
    );
}

#[gpui::test]
fn a_new_section_creates_a_request(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (shell, cx) = open_editor(cx, &dir);
    let edited = format!("{CANONICAL}\n### Health\nGET https://api.test/health\n");
    set_document_text(cx, &shell, &edited);

    click(cx, "document-save");
    cx.run_until_parked();
    let file = read_file(&dir, "collections/api/invoices/health.req.toml");
    assert!(
        file.contains("name = \"Health\""),
        "the section names the request"
    );
    assert!(file.contains("method = \"GET\""));
    assert!(
        bounds(cx, "document-editor").is_none(),
        "saving clears the edit"
    );
}

#[gpui::test]
fn a_broken_section_blocks_the_save_and_shows_the_warning(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (shell, cx) = open_editor(cx, &dir);
    let broken = CANONICAL.replace("POST https://api.test/v1/invoices\n", "");
    set_document_text(cx, &shell, &broken);

    click(cx, "document-save");
    cx.run_until_parked();
    present(cx, "document-warnings");
    let warning = label_of(cx, "document-warnings").unwrap_or_default();
    assert!(
        warning.contains("Create invoice") && warning.contains("no method or URL line"),
        "the warning names the broken section: {warning}"
    );
    let file = read_file(&dir, "collections/api/invoices/create.req.toml");
    assert!(file.contains("total"), "the save wrote nothing: {file}");

    set_document_text(cx, &shell, CANONICAL);
    assert!(
        bounds(cx, "document-warnings").is_none(),
        "a parseable text clears the warnings"
    );
}

#[gpui::test]
fn a_removed_section_asks_once_and_deletes_after_confirming(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (shell, cx) = open_editor(cx, &dir);
    set_document_text(cx, &shell, "### List\nGET https://api.test/v1/invoices\n");

    click(cx, "document-save");
    cx.run_until_parked();
    present(cx, "document-confirm-delete");
    let listed = label_of(cx, "document-confirm-delete").unwrap_or_default();
    assert!(
        listed.contains("Create invoice"),
        "the modal lists the removed request: {listed}"
    );
    assert!(
        dir.path()
            .join("collections/api/invoices/create.req.toml")
            .exists(),
        "nothing is deleted before the confirmation"
    );

    click(cx, "document-confirm-save");
    cx.run_until_parked();
    assert!(
        !dir.path()
            .join("collections/api/invoices/create.req.toml")
            .exists(),
        "confirming deletes the request's file"
    );
    assert!(
        bounds(cx, "document-confirm-delete").is_none(),
        "the confirmation is asked once"
    );
    let file = read_file(&dir, "collections/api/invoices/list.req.toml");
    assert!(
        file.contains("name = \"List\""),
        "the kept request is untouched"
    );
}

#[gpui::test]
fn cancelling_the_confirmation_deletes_nothing(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (shell, cx) = open_editor(cx, &dir);
    set_document_text(cx, &shell, "### List\nGET https://api.test/v1/invoices\n");

    click(cx, "document-save");
    cx.run_until_parked();
    present(cx, "document-confirm-delete");
    click(cx, "document-confirm-cancel");
    cx.run_until_parked();
    assert!(
        bounds(cx, "document-confirm-delete").is_none(),
        "the modal closes"
    );
    assert!(
        dir.path()
            .join("collections/api/invoices/create.req.toml")
            .exists(),
        "cancelling deletes nothing"
    );
    present(cx, "document-editor");
}

#[gpui::test]
fn stacked_and_side_by_side_never_show_the_document_editor(cx: &mut TestAppContext) {
    let dir = folder_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-chevron-{FOLDER}"));
    click(cx, &format!("tree-row-{LIST}"));

    present(cx, "request-area");
    assert!(
        bounds(cx, "document-view").is_none(),
        "Stacked shows no document"
    );

    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();
    present(cx, "request-area");
    assert!(
        bounds(cx, "document-view").is_none(),
        "Side by side shows no document"
    );
    assert!(bounds(cx, "document-editor").is_none());
}
