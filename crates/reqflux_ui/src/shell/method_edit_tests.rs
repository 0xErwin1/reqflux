//! The method chip edits the draft (S7): the verb menu, the custom verb field, and their
//! persistence, send and save (S8).

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, render, state_file};
use super::send_tests::closed_port;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000081";
const VERBED: &str = "01J00000000000000000000082";
const FILE: &str = "collections/api/verbed.req.toml";

/// A workspace holding `GET Verbed` at a local port nothing listens on.
fn method_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        FILE,
        &format!(
            "id = \"{VERBED}\"\nname = \"Verbed\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{}/v\"\n",
            closed_port()
        ),
    );
    dir
}

fn draft_method(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(VERBED)
            .map(|draft| draft.request.method.to_string())
    })
}

/// The verb the chip shows.
fn chip(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        shell
            .requests
            .get(VERBED)
            .map(|state| {
                shell
                    .displayed_request(VERBED, &state.inspection.request.request)
                    .method
                    .to_string()
            })
            .unwrap_or_default()
    })
}

/// Opens `Custom…`, replaces the field's text with `text` and presses `key`.
fn type_custom(cx: &mut VisualTestContext, text: &str, key: &str) {
    click(cx, "request-method");
    click(cx, "method-custom");
    assert!(
        bounds(cx, "request-method-input").is_some(),
        "the field opened"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    for character in text.chars() {
        let key = if character == ' ' {
            "space".to_owned()
        } else {
            character.to_string()
        };
        cx.simulate_keystrokes(&key);
    }
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
}

fn file(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default()
}

#[gpui::test]
fn the_menu_picks_a_verb_into_the_draft(cx: &mut TestAppContext) {
    let dir = method_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{VERBED}"));

    click(cx, "request-method");
    for verb in [
        "get", "post", "put", "patch", "delete", "head", "options", "custom",
    ] {
        assert!(bounds(cx, &format!("method-{verb}")).is_some(), "{verb}");
    }
    click(cx, "method-get");
    assert!(bounds(cx, "method-get").is_none(), "the pick closed it");
    assert!(
        draft_method(cx, &shell).is_none(),
        "the same verb is a no-op"
    );
    assert!(!marker(cx, VERBED));

    click(cx, "request-method");
    click(cx, "method-post");
    assert_eq!(draft_method(cx, &shell).as_deref(), Some("POST"));
    assert_eq!(chip(cx, &shell), "POST", "the chip shows the draft's verb");
    assert!(marker(cx, VERBED), "the edit marks the tab");

    click(cx, "request-method");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "method-put").is_none(), "Escape closes it");

    let (shell_2, cx) = render(cx, &dir, &state_path);
    assert_eq!(
        draft_method(cx, &shell_2).as_deref(),
        Some("POST"),
        "the draft was restored"
    );
}

#[gpui::test]
fn the_custom_field_opens_with_the_verb_selected_so_typing_replaces_it(cx: &mut TestAppContext) {
    let dir = method_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{VERBED}"));

    click(cx, "request-method");
    click(cx, "method-custom");
    cx.simulate_input("PURGE");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(draft_method(cx, &shell).as_deref(), Some("PURGE"));
}

#[gpui::test]
fn a_custom_verb_lands_in_the_draft_and_is_saved(cx: &mut TestAppContext) {
    let dir = method_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{VERBED}"));

    type_custom(cx, "BAD VERB", "enter");
    assert!(
        draft_method(cx, &shell).is_none(),
        "an invalid verb is dropped"
    );

    type_custom(cx, "PURGE", "escape");
    assert!(
        bounds(cx, "request-method-input").is_none(),
        "Escape closed it"
    );
    assert!(draft_method(cx, &shell).is_none(), "Escape discards");

    type_custom(cx, "purge", "enter");
    assert!(
        bounds(cx, "request-method-input").is_none(),
        "Enter closed it"
    );
    assert_eq!(draft_method(cx, &shell).as_deref(), Some("PURGE"));

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        file(&dir).contains("method = \"PURGE\""),
        "the file was written: {}",
        file(&dir)
    );
    assert!(!marker(cx, VERBED), "saving clears the marker");
}

#[gpui::test]
fn ctrl_enter_sends_the_drafted_verb(cx: &mut TestAppContext) {
    let dir = method_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{VERBED}"));

    click(cx, "request-method");
    click(cx, "method-delete");
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    let sent = cx.update(|_, cx| {
        shell
            .read(cx)
            .last_send
            .as_ref()
            .map(|last| last.method.clone())
    });
    assert_eq!(sent.as_deref(), Some("DELETE"), "the drafted verb was sent");
}

/// The URL field still opens and commits after the request line learned the method menu.
#[gpui::test]
fn the_url_field_still_commits(cx: &mut TestAppContext) {
    let dir = method_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{VERBED}"));

    click(cx, "request-url");
    let open = |cx: &mut VisualTestContext| cx.update(|_, cx| shell.read(cx).url_edit.is_some());
    assert!(open(cx), "the field opened");
    cx.simulate_keystrokes("x enter");
    cx.run_until_parked();
    assert!(!open(cx), "Enter closed it");
    let url = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(VERBED)
            .map(|draft| draft.request.url.clone())
    });
    assert!(
        url.is_some_and(|url| url.ends_with('x')),
        "the URL was committed"
    );
}
