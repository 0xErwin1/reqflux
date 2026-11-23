//! Copying from the history tab (history-copy-and-scratch S1): the recorded URL, the Params
//! and Headers keys and values and the Body can be focused, selected with the keyboard and
//! copied, and nothing typed into them changes the record.

use gpui::{TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::draft_tests::{click, present, render, state_file};
use super::history_tests::{history_workspace, open_top_row};
use super::send_tests::{Server, serve, settle};
use crate::workspace_tests::write;

const POST: &str = "01J00000000000000000000086";

/// The history workspace with `POST Post` at `/post?page=2&q=tea`, the header
/// `X-Trace: abc-123` and the raw body `{"name":"tea"}`.
fn copy_workspace(port: u16) -> TempDir {
    let dir = history_workspace(port);
    write(
        dir.path(),
        "collections/api/post.req.toml",
        &format!(
            "id = \"{POST}\"\nname = \"Post\"\nmethod = \"POST\"\n\
             url = \"http://127.0.0.1:{port}/post?page=2&q=tea\"\n\n\
             [[headers]]\nkey = \"X-Trace\"\nvalue = \"abc-123\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"name\":\"tea\"}}'\n"
        ),
    );
    dir
}

/// Clicks `selector`, selects all its text and copies it; returns the clipboard.
fn copy_of(cx: &mut VisualTestContext, selector: &str) -> Option<String> {
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    click(cx, selector);
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.run_until_parked();
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui::test]
fn every_recorded_value_can_be_selected_and_copied(cx: &mut TestAppContext) {
    let dir = copy_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POST}"));
    click(cx, "request-send");
    assert!(settle(cx, &shell, POST).is_some());
    open_top_row(cx);
    present(cx, "history-readonly");

    let url = copy_of(cx, "history-url").unwrap_or_default();
    assert!(url.ends_with("/post?page=2&q=tea"), "copied {url:?}");

    click(cx, "history-tab-params");
    assert_eq!(copy_of(cx, "history-grid-key-2").as_deref(), Some("q"));
    assert_eq!(copy_of(cx, "history-grid-value-1").as_deref(), Some("2"));

    click(cx, "history-tab-headers");
    let headers: Vec<(String, String)> = (1..=12)
        .map_while(|n| {
            let key = format!("history-grid-key-{n}");
            super::draft_tests::bounds(cx, &key)?;
            Some((
                copy_of(cx, &key).unwrap_or_default(),
                copy_of(cx, &format!("history-grid-value-{n}")).unwrap_or_default(),
            ))
        })
        .collect();
    assert!(
        headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("x-trace") && value == "abc-123"),
        "copied headers {headers:?}"
    );

    click(cx, "history-tab-body");
    assert_eq!(
        copy_of(cx, "history-body").as_deref(),
        Some("{\"name\":\"tea\"}")
    );

    // Typing into a recorded value changes nothing.
    click(cx, "history-body");
    cx.simulate_keystrokes("ctrl-a backspace x");
    cx.run_until_parked();
    assert_eq!(
        copy_of(cx, "history-body").as_deref(),
        Some("{\"name\":\"tea\"}")
    );
    assert!(cx.update(|_, cx| shell.read(cx).drafts.is_empty()));
}

#[gpui::test]
fn typing_into_the_url_or_a_cell_leaves_the_record_unchanged(cx: &mut TestAppContext) {
    let dir = copy_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POST}"));
    click(cx, "request-send");
    assert!(settle(cx, &shell, POST).is_some());
    open_top_row(cx);
    present(cx, "history-readonly");
    let url = copy_of(cx, "history-url").unwrap_or_default();
    click(cx, "history-tab-params");
    let cell = copy_of(cx, "history-grid-value-1").unwrap_or_default();
    let sent = cx.update(|_, cx| shell.read(cx).orphan.as_ref().map(|tab| tab.sent.clone()));

    click(cx, "history-url");
    cx.simulate_keystrokes("ctrl-a backspace x y");
    click(cx, "history-grid-value-1");
    cx.simulate_keystrokes("ctrl-a backspace z");
    cx.run_until_parked();

    assert_eq!(copy_of(cx, "history-url").as_deref(), Some(url.as_str()));
    assert_eq!(
        copy_of(cx, "history-grid-value-1").as_deref(),
        Some(cell.as_str())
    );
    let after = cx.update(|_, cx| shell.read(cx).orphan.as_ref().map(|tab| tab.sent.clone()));
    assert_eq!(after, sent);
    assert!(cx.update(|_, cx| shell.read(cx).drafts.is_empty()));
}
