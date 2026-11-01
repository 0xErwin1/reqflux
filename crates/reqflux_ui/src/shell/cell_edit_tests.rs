//! No jump between showing and editing (S9 of request-pane-feedback): every inline input
//! keeps the place of its read-only value, and its text starts where the read-only text did.

use gpui::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext};
use gpui_component::input::InputState;
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{click, present, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J000000000000000000000Q3";
const INLINE: &str = "01J000000000000000000000Q4";

/// A workspace holding `GET Inline` with the param `a=1`, bearer auth with the token `abc`
/// and a 5000 ms timeout.
fn inline_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/inline.req.toml",
        &format!(
            "id = \"{INLINE}\"\nname = \"Inline\"\nmethod = \"GET\"\nurl = \"https://h/inline\"\n\n\
             [[params]]\nkey = \"a\"\nvalue = \"1\"\n\n\
             [auth]\ntype = \"bearer\"\ntoken = \"abc\"\n\n\
             [settings]\ntimeout_ms = 5000\n"
        ),
    );
    dir
}

/// The bounds of the text of the open field `input`, as it was last painted.
fn input_text(cx: &mut VisualTestContext, input: Option<Entity<InputState>>) -> Bounds<Pixels> {
    let Some(input) = input else {
        panic!("no field is open");
    };
    match cx.update(|_, cx| input.read(cx).text_bounds()) {
        Some(bounds) => bounds,
        None => panic!("the field was not painted"),
    }
}

fn cell_input(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Entity<InputState>> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .cell_edit
            .as_ref()
            .map(|edit| edit.input.clone())
    })
}

fn url_input(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Entity<InputState>> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .url_edit
            .as_ref()
            .map(|edit| edit.input.clone())
    })
}

/// Asserts that the field box did not move or resize, and that the text starts at the same
/// x and is centered on the same line.
fn assert_no_jump(
    what: &str,
    (field, text): (Bounds<Pixels>, Bounds<Pixels>),
    (editing_field, editing_text): (Bounds<Pixels>, Bounds<Pixels>),
) {
    assert_eq!(field, editing_field, "{what}: the field box moved");
    assert_eq!(
        text.origin.x, editing_text.origin.x,
        "{what}: the text moved sideways"
    );
    let center = |bounds: Bounds<Pixels>| f32::from(bounds.center().y);
    assert!(
        (center(text) - center(editing_text)).abs() < 0.5,
        "{what}: the text moved vertically: {text:?} vs {editing_text:?}"
    );
}

#[gpui::test]
fn the_url_field_opens_where_the_url_is(cx: &mut TestAppContext) {
    let dir = inline_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{INLINE}"));
    let before = (present(cx, "request-url"), present(cx, "request-url-text"));

    click(cx, "request-url");
    let input = url_input(cx, &shell);
    let after = (present(cx, "request-url"), input_text(cx, input));
    assert_no_jump("URL", before, after);
}

#[gpui::test]
fn a_grid_cell_field_opens_where_the_cell_text_is(cx: &mut TestAppContext) {
    let dir = inline_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{INLINE}"));
    click(cx, "request-tab-params");
    let row = present(cx, "request-params-row-1");
    let before = (
        present(cx, "request-params-value-1"),
        present(cx, "request-params-value-1-text"),
    );

    click(cx, "request-params-value-1");
    let input = cell_input(cx, &shell);
    let after = (present(cx, "request-params-value-1"), input_text(cx, input));
    assert_no_jump("grid cell", before, after);
    assert_eq!(row, present(cx, "request-params-row-1"), "the row resized");
}

#[gpui::test]
fn the_timeout_field_opens_where_the_timeout_is(cx: &mut TestAppContext) {
    let dir = inline_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{INLINE}"));
    click(cx, "request-tab-settings");
    let before = (
        present(cx, "request-setting-timeout"),
        present(cx, "request-setting-timeout-text"),
    );

    click(cx, "request-setting-timeout");
    let input = cell_input(cx, &shell);
    let after = (
        present(cx, "request-setting-timeout-input"),
        input_text(cx, input),
    );
    assert_no_jump("timeout", before, after);
}

#[gpui::test]
fn an_auth_field_opens_where_its_value_is(cx: &mut TestAppContext) {
    let dir = inline_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{INLINE}"));
    click(cx, "request-tab-auth");
    let before = (
        present(cx, "request-auth-well-token"),
        present(cx, "request-auth-well-token-text"),
    );

    click(cx, "request-auth-well-token");
    let input = cell_input(cx, &shell);
    let after = (
        present(cx, "request-auth-well-token"),
        input_text(cx, input),
    );
    assert_no_jump("auth token", before, after);
}
