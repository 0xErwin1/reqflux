//! The Document layout's view (v1-features S7, design/layouts/02): the active request's
//! folder as one read-only `.http` document, the empty state without an open request, and
//! the switch Ctrl+Alt+3 makes: the split gives way to the document and comes back with
//! Ctrl+Alt+1. Stacked behavior itself stays `split_tests`'s and `layout_tests`'s.

use gpui::{TestAppContext, VisualTestContext};
use tempfile::TempDir;

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
