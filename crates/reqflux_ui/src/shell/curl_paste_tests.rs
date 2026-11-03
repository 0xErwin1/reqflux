//! A curl command pasted into the URL field (v1-features S2) replaces the draft and reports
//! it with Undo; anything else pastes as URL text.

use gpui::{ClipboardItem, Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{Auth, Body, KeyValue, Method, RawLanguage, Request};

use super::REPLACED;
use crate::shell::Shell;
use crate::shell::draft_tests::{PLAIN, click, draft_workspace, edit_url, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::toasts::tests::{
    click_node, error, pushed, shown_toasts, success, tree_label, warning,
};

const CURL: &str = "curl -X PATCH 'https://api.test/v1/customers/c_1' \\\n  \
                    -H 'Content-Type: application/json' -H 'Accept: application/json' \\\n  \
                    -u ann:s3cret --data '{\"name\":\"Ann\"}'";

fn draft(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Request> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(PLAIN)
            .map(|draft| draft.request.clone())
    })
}

fn field_open(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).url_edit.is_some())
}

/// Opens the URL field of `Plain`, selects its text and pastes `text`.
fn paste(cx: &mut VisualTestContext, text: &str) {
    click(cx, "request-url");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned())));
    cx.simulate_keystrokes("ctrl-a ctrl-v");
    cx.run_until_parked();
}

#[gpui::test]
fn a_pasted_curl_command_replaces_the_request(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    paste(cx, CURL);

    let Some(request) = draft(cx, &shell) else {
        panic!("no draft after the paste");
    };
    assert_eq!(request.name, "Plain", "the name is kept");
    assert_eq!(request.method, Method::PATCH);
    assert_eq!(request.url, "https://api.test/v1/customers/c_1");
    assert_eq!(
        request.headers,
        vec![
            KeyValue::new("Content-Type", "application/json"),
            KeyValue::new("Accept", "application/json"),
        ]
    );
    assert_eq!(
        request.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: "{\"name\":\"Ann\"}".to_owned(),
        }
    );
    assert_eq!(
        request.auth,
        Auth::Basic {
            username: "ann".to_owned(),
            password: "s3cret".to_owned(),
        }
    );
    assert!(!field_open(cx, &shell), "the field closes");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).shown_url(PLAIN)),
        "https://api.test/v1/customers/c_1"
    );
    assert_eq!(
        pushed(cx),
        vec![success(
            REPLACED,
            "Method, URL, 2 headers, JSON body, basic auth"
        )]
    );
}

#[gpui::test]
fn ignored_options_make_a_warning_that_lists_them(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    paste(cx, "curl --frobnicate https://api.test/ping");

    assert_eq!(
        draft(cx, &shell).map(|request| request.url),
        Some("https://api.test/ping".to_owned())
    );
    assert_eq!(
        pushed(cx),
        vec![warning(
            REPLACED,
            "Method, URL · 1 option ignored\nunknown option --frobnicate was ignored"
        )]
    );
}

#[gpui::test]
fn undo_drops_the_draft_the_paste_made(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    paste(cx, CURL);
    assert!(draft(cx, &shell).is_some());

    click_node(cx, "toast-action-1-0");

    assert_eq!(draft(cx, &shell), None, "the request is as saved again");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).shown_url(PLAIN)),
        "{{base}}/ok"
    );
    let toasts = pushed(cx);
    assert_eq!(
        toasts.last(),
        Some(&success("Paste undone", "GET Plain is back as it was"))
    );
}

#[gpui::test]
fn the_undo_button_shows_the_undo_paste_key(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    paste(cx, CURL);

    assert_eq!(
        tree_label(cx, "toast-action-1-0-key").as_deref(),
        Some("Ctrl Alt Z")
    );
    assert_eq!(
        tree_label(cx, "toast-action-1-0").as_deref(),
        Some("Undo"),
        "the keycap stays out of the button's name"
    );
}

#[gpui::test]
fn the_undo_paste_key_takes_the_paste_back_and_closes_its_toast(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    paste(cx, CURL);
    assert!(draft(cx, &shell).is_some());

    cx.simulate_keystrokes("ctrl-alt-z");
    cx.run_until_parked();

    assert_eq!(draft(cx, &shell), None, "the request is as saved again");
    assert_eq!(
        pushed(cx).last(),
        Some(&success("Paste undone", "GET Plain is back as it was"))
    );
    assert!(
        !shown_toasts(cx).contains(&"toast-1".to_owned()),
        "the paste's toast closes with its Undo"
    );
}

#[gpui::test]
fn the_undo_paste_key_does_nothing_without_a_paste(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, "{{base}}/edited");
    let before = draft(cx, &shell);
    let toasts = pushed(cx).len();

    cx.simulate_keystrokes("ctrl-alt-z");
    cx.run_until_parked();

    assert_eq!(draft(cx, &shell), before);
    assert_eq!(pushed(cx).len(), toasts, "no toast without a paste to undo");
}

#[gpui::test]
fn undo_restores_the_draft_from_before_the_paste(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, "{{base}}/edited");
    let before = draft(cx, &shell);
    assert_eq!(
        before.as_ref().map(|request| request.url.as_str()),
        Some("{{base}}/edited")
    );

    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(CURL.to_owned())));
    cx.simulate_keystrokes("ctrl-a ctrl-v");
    cx.run_until_parked();
    assert_eq!(
        draft(cx, &shell).map(|request| request.method),
        Some(Method::PATCH)
    );

    click_node(cx, "toast-action-1-0");
    assert_eq!(draft(cx, &shell), before);
}

#[gpui::test]
fn a_paste_that_is_not_curl_is_url_text(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    paste(cx, "https://other.test/x");

    assert!(field_open(cx, &shell), "the field stays open");
    let field = cx.update(|_, cx| {
        shell
            .read(cx)
            .url_edit
            .as_ref()
            .map(|edit| edit.input.read(cx).value().to_string())
    });
    assert_eq!(field.as_deref(), Some("https://other.test/x"));
    let request = draft(cx, &shell);
    assert_eq!(
        request.as_ref().map(|request| request.url.as_str()),
        Some("https://other.test/x")
    );
    assert_eq!(request.map(|request| request.method), Some(Method::GET));
    assert!(pushed(cx).is_empty(), "{:?}", pushed(cx));
}

#[gpui::test]
fn a_broken_curl_command_says_why_and_changes_nothing(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    paste(cx, "curl 'https://api.test/ping");

    assert_eq!(draft(cx, &shell), None);
    assert_eq!(
        pushed(cx),
        vec![error(
            "Cannot read the curl command",
            "the curl command has an unterminated quote or escape"
        )]
    );
}
