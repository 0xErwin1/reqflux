//! Saved examples in the window (v1-features S4, design/examples/01-tree-children): leaves
//! under their request, the read-only example tab with masked values badged, the example
//! row menu, rename in the row, delete with a confirmation, Save as example and writing a
//! body to a file.

use std::path::Path;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use reqflux_engine::{Engine, MASK, Response, ScriptOutcome, Timings};

use super::model::received_response;
use crate::shell::Shell;
use crate::shell::request::{Completed, Received, SendState};
use crate::shell::send::RequestSend;
use crate::shell::sidebar_tests::{HEALTH, TOKEN, sidebar_workspace};
use crate::shell::toasts::Kind;
use crate::shell::toasts::tests::pushed;
use crate::shell::tree_model::example_node_id;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::write;

/// The id of [`OK_EXAMPLE`].
pub(crate) const OK_ID: &str = "01J0000000000000000000000E";

/// An example `OK` of `GET Health`: a 200 whose body holds a masked token and whose
/// `set-cookie` value was masked on save.
pub(crate) const OK_EXAMPLE: &str = "id = \"01J0000000000000000000000E\"
name = \"OK\"

[request]
method = \"GET\"
url = \"{{base_url}}/health\"

[[request.headers]]
key = \"X-Tenant\"
value = \"{{tenant}}\"

[response]
status = 200
reason = \"OK\"

[[response.headers]]
key = \"content-type\"
value = \"application/json\"

[[response.headers]]
key = \"set-cookie\"
value = \"session=\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\"

[response.body]
text = '''
{
  \"status\": \"up\",
  \"token\": \"\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\"
}'''
";

/// Writes the example `slug` with `content` beside the request file `<request>.req.toml`.
pub(crate) fn write_example(root: &Path, request: &str, slug: &str, content: &str) {
    write(
        root,
        &format!("{request}.examples/{slug}.example.toml"),
        content,
    );
}

fn node() -> String {
    example_node_id(HEALTH, OK_ID)
}

fn render<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    cx.update(super::super::bind_keys);
    let workspace = OpenedWorkspace::open(Some(root.to_path_buf()));
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    (shell, cx)
}

fn present(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        .is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let Some(bounds) = cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str())) else {
        panic!("{selector} is missing");
    };
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn right_click(cx: &mut VisualTestContext, selector: &str) {
    let Some(bounds) = cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str())) else {
        panic!("{selector} is missing");
    };
    cx.simulate_mouse_down(bounds.center(), gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(bounds.center(), gpui::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn engine(root: &Path) -> Engine {
    match Engine::open(root) {
        Ok(engine) => engine,
        Err(error) => panic!("the workspace opens: {error}"),
    }
}

fn names(root: &Path) -> Vec<String> {
    match engine(root).examples(HEALTH) {
        Ok(examples) => examples
            .into_iter()
            .map(|saved| saved.example.name)
            .collect(),
        Err(error) => panic!("examples: {error}"),
    }
}

/// Expands `GET Health` so its example shows.
fn expand_health(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{HEALTH}-chevron"));
}

#[gpui::test]
fn a_request_with_examples_gets_a_chevron_and_leaves_with_their_status(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let (_shell, cx) = render(cx, dir.path());

    assert!(present(cx, &format!("tree-row-{HEALTH}-chevron")));
    assert!(present(cx, &format!("tree-row-{HEALTH}-count")));
    assert!(
        !present(cx, &format!("tree-row-{}", node())),
        "collapsed at first"
    );
    assert!(
        !present(cx, &format!("tree-row-{TOKEN}-chevron")),
        "no examples, no chevron"
    );

    expand_health(cx);
    assert!(present(cx, &format!("tree-row-{}", node())));
    assert!(present(cx, &format!("tree-row-{}-icon", node())));
    let Some(trailing) = cx.debug_bounds(Box::leak(
        format!("tree-row-{}-trailing", node()).into_boxed_str(),
    )) else {
        panic!("the example row has a trailing slot");
    };
    let Some(status) = cx.debug_bounds(Box::leak(
        format!("tree-status-{}", node()).into_boxed_str(),
    )) else {
        panic!("the example row shows its status");
    };
    assert!(
        trailing.contains(&status.origin),
        "the status sits in the trailing slot: {status:?} in {trailing:?}"
    );
}

#[gpui::test]
fn opening_an_example_shows_it_read_only_with_masked_values_badged(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let (shell, cx) = render(cx, dir.path());
    expand_health(cx);

    click(cx, &format!("tree-row-{}", node()));
    for selector in [
        "example-tab",
        "example-readonly",
        "example-header",
        "example-request-line",
        "example-status",
        "example-line-1",
        "example-line-3-secret",
        "example-copy",
        "example-save-body",
    ] {
        assert!(present(cx, selector), "{selector} is missing");
    }
    assert!(
        !present(cx, "example-line-2-secret"),
        "a plain line has no badge"
    );
    let active = cx.update(|_, cx| shell.read(cx).active_request().map(str::to_owned));
    assert_eq!(active, None, "no request tab is active");

    click(cx, "example-view-headers");
    assert!(
        present(cx, "example-header-2-secret"),
        "set-cookie was masked"
    );
    assert!(!present(cx, "example-header-1-secret"));

    click(cx, "example-tab-close");
    assert!(!present(cx, "example-tab"));
}

#[gpui::test]
fn enter_on_a_selected_example_opens_it(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let (shell, cx) = render(cx, dir.path());
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());
    cx.update(|window, cx| {
        sidebar.update(cx, |sidebar, cx| {
            sidebar.update_tree(cx, |tree| tree.reveal(&node()));
            sidebar.focus.focus(window, cx);
        });
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(present(cx, "example-readonly"));
}

#[gpui::test]
fn the_example_menu_offers_only_what_an_example_supports(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let (_shell, cx) = render(cx, dir.path());
    expand_health(cx);

    right_click(cx, &format!("tree-row-{}", node()));
    for item in ["open", "rename", "save-body", "delete"] {
        assert!(
            present(cx, &format!("tree-menu-{item}")),
            "{item} is missing"
        );
    }
    for item in [
        "duplicate",
        "move-to",
        "new-request",
        "run",
        "generate-code",
    ] {
        assert!(
            !present(cx, &format!("tree-menu-{item}")),
            "{item} is offered"
        );
    }
}

#[gpui::test]
fn rename_in_the_row_renames_the_file(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let (_shell, cx) = render(cx, dir.path());
    expand_health(cx);

    click(cx, &format!("tree-row-{}", node()));
    click(cx, &format!("tree-row-{}", node()));
    cx.simulate_keystrokes("f2");
    cx.run_until_parked();
    assert!(present(cx, "tree-rename-input"));
    cx.simulate_input("Healthy");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(names(dir.path()), ["Healthy"]);
    assert!(
        dir.path()
            .join("collections/api/health.examples/healthy.example.toml")
            .is_file()
    );
    assert!(dir.path().join("collections/api/health.req.toml").is_file());
}

#[gpui::test]
fn delete_asks_first_and_keeps_the_request(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let request_file = dir.path().join("collections/api/health.req.toml");
    let before = std::fs::read(&request_file).ok();
    let (_shell, cx) = render(cx, dir.path());
    expand_health(cx);

    click(cx, &format!("tree-row-{}", node()));
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert!(present(cx, "example-delete-prompt"));
    assert_eq!(
        names(dir.path()),
        ["OK"],
        "nothing is deleted before the confirmation"
    );

    click(cx, "example-delete-confirm");
    assert!(names(dir.path()).is_empty());
    assert_eq!(
        std::fs::read(&request_file).ok(),
        before,
        "the request is unchanged"
    );
    assert!(!present(cx, &format!("tree-row-{}", node())));
    assert!(
        !present(cx, &format!("tree-row-{HEALTH}-chevron")),
        "no examples left"
    );
}

/// Opens `GET Health` and gives it a received response.
fn received(cx: &mut VisualTestContext, shell: &Entity<Shell>, body: &[u8]) -> Response {
    let response = Response {
        status: 401,
        reason: Some("Unauthorized".to_owned()),
        version: "1.1".to_owned(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: body.to_vec(),
        final_url: "https://example.test".to_owned(),
        elapsed: std::time::Duration::from_millis(12),
        timings: Timings::default(),
    };
    let received = Received::new(response.clone(), Vec::new(), ScriptOutcome::default());
    shell.update(cx, |shell, cx| {
        shell.open_document(HEALTH, cx);
        let mut send = RequestSend::default();
        send.state = SendState::Done(Completed::Response(received));
        shell.sends.insert(HEALTH.to_owned(), send);
    });
    cx.run_until_parked();
    response
}

#[gpui::test]
fn save_as_example_names_it_after_the_reason_and_selects_the_new_leaf(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (shell, cx) = render(cx, dir.path());
    received(cx, &shell, br#"{"error":"token_expired"}"#);

    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_save_example(window, cx)));
    cx.run_until_parked();
    assert!(present(cx, "example-save-dialog"));
    assert!(present(cx, "example-save-summary"));
    let name = cx.update(|_, cx| shell.read(cx).save_example_name(cx));
    assert_eq!(name.as_deref(), Some("Unauthorized"));

    click(cx, "example-save-confirm");
    assert!(!present(cx, "example-save-dialog"));
    assert_eq!(names(dir.path()), ["Unauthorized"]);
    let toasts = pushed(cx);
    let last = toasts
        .last()
        .map(|toast| (toast.kind, toast.title.clone(), toast.text.clone()));
    assert_eq!(
        last,
        Some((
            Kind::Success,
            "Saved example Unauthorized".to_owned(),
            "Health".to_owned()
        ))
    );
    let selected = cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).selected().map(str::to_owned)
    });
    let saved = engine(dir.path())
        .examples(HEALTH)
        .ok()
        .and_then(|examples| examples.into_iter().next())
        .map(|saved| example_node_id(HEALTH, &saved.example.id.to_string()));
    assert_eq!(selected, saved, "the new leaf is selected");
    assert!(!present(cx, "example-readonly"), "and not opened");
}

#[gpui::test]
fn the_response_offers_save_as_example_and_the_export_menu(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (shell, cx) = render(cx, dir.path());
    received(cx, &shell, b"{}");
    assert!(present(cx, "response-save-example"));

    click(cx, "response-export");
    for item in ["save-body", "save-example", "copy"] {
        assert!(
            present(cx, &format!("response-export-{item}")),
            "{item} is missing"
        );
    }
    click(cx, "response-export-save-example");
    assert!(!present(cx, "response-export-menu"));
    assert!(present(cx, "example-save-dialog"));
}

#[gpui::test]
fn cancel_closes_the_dialog_without_saving(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (shell, cx) = render(cx, dir.path());
    received(cx, &shell, b"{}");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_save_example(window, cx)));
    cx.run_until_parked();
    click(cx, "example-save-cancel");
    assert!(!present(cx, "example-save-dialog"));
    assert!(names(dir.path()).is_empty());
}

#[gpui::test]
fn writing_a_body_says_where_or_why_not(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let out = match tempfile::tempdir() {
        Ok(out) => out,
        Err(error) => panic!("temp dir: {error}"),
    };
    let (shell, cx) = render(cx, dir.path());
    let response = received(cx, &shell, &[0x89, 0x50, 0x4e, 0x47, 0xff]);
    let path = out.path().join("body.bin");

    shell.update(cx, |shell, cx| shell.write_body(&response, &path, cx));
    assert_eq!(
        std::fs::read(&path).ok(),
        Some(response.body.clone()),
        "bytes as received"
    );
    let last = pushed(cx)
        .last()
        .map(|toast| (toast.kind, toast.title.clone()));
    assert_eq!(
        last,
        Some((Kind::Success, "Saved the response body".to_owned()))
    );

    let missing = out.path().join("missing/body.bin");
    shell.update(cx, |shell, cx| shell.write_body(&response, &missing, cx));
    let last = pushed(cx)
        .last()
        .map(|toast| (toast.kind, toast.title.clone(), toast.text.clone()));
    assert!(
        matches!(
            last,
            Some((Kind::Error, title, text))
                if title == "Could not save the response body"
                    && text.starts_with("could not write the response body to")
        ),
        "the engine's message as is"
    );
}

#[test]
fn a_received_response_keeps_status_headers_and_body() {
    let received = Received::new(
        Response {
            status: 200,
            reason: Some("OK".to_owned()),
            version: "2".to_owned(),
            headers: vec![("a".to_owned(), "b".to_owned())],
            body: MASK.as_bytes().to_vec(),
            final_url: String::new(),
            elapsed: std::time::Duration::ZERO,
            timings: Timings::default(),
        },
        Vec::new(),
        ScriptOutcome::default(),
    );
    let response = received_response(&received);
    assert_eq!(response.status, 200);
    assert_eq!(response.reason.as_deref(), Some("OK"));
    assert_eq!(response.headers, [("a".to_owned(), "b".to_owned())]);
    assert_eq!(response.body, MASK.as_bytes());
}
