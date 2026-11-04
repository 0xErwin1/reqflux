//! Adding and deleting grid rows (S6) and their persistence (S8): the `Add` row under the
//! Params, Headers and body grids appends an empty enabled row and opens its key cell, and
//! the delete control of a row removes it from the draft.

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{Body, KeyValue, Request};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000091";
const ROWS: &str = "01J00000000000000000000092";
const FILE: &str = "collections/api/rows.req.toml";

/// A workspace holding `POST Rows` with the params `a`, `b`, `c`, no headers and a
/// urlencoded body with the field `x`.
fn rows_workspace() -> TempDir {
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
            "id = \"{ROWS}\"\nname = \"Rows\"\nmethod = \"POST\"\nurl = \"http://h/rows\"\n\n\
             [[params]]\nkey = \"a\"\nvalue = \"1\"\n\n\
             [[params]]\nkey = \"b\"\nvalue = \"2\"\n\n\
             [[params]]\nkey = \"c\"\nvalue = \"3\"\n\n\
             [body]\nmode = \"urlencoded\"\n\n[[body.fields]]\nkey = \"x\"\nvalue = \"9\"\n"
        ),
    );
    dir
}

/// Opens `POST Rows` on the sub-tab `tab`.
fn open(cx: &mut VisualTestContext, tab: &str) {
    click(cx, &format!("tree-row-{ROWS}"));
    click(cx, &format!("request-tab-{tab}"));
}

fn draft(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<Request> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(ROWS)
            .map(|draft| draft.request.clone())
    })
}

fn keys(rows: &[KeyValue]) -> Vec<&str> {
    rows.iter().map(|row| row.key.as_str()).collect()
}

/// The table, the 1-based row and the text of the open cell field, if one is open.
fn open_cell(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<(String, usize, String)> {
    cx.update(|cx| {
        shell.read(cx).cell_edit.as_ref().map(|edit| {
            (
                edit.table.clone(),
                edit.n,
                edit.input.read(cx).value().to_string(),
            )
        })
    })
}

/// Replaces the text of the open field with `text`: selects all and types.
fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("ctrl-a");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

fn enter(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn add_in_params_opens_the_new_key_cell_and_enter_stores_it(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-add");
    assert!(marker(cx, ROWS), "the add marks the tab");
    let added = draft(cx, &shell).expect("the add created the draft");
    assert_eq!(keys(&added.params), ["a", "b", "c", ""]);
    assert!(added.params[3].enabled, "the new row is enabled");
    assert_eq!(
        open_cell(cx, &shell),
        Some(("params".to_owned(), 4, String::new())),
        "the key cell of the new row is open"
    );

    type_text(cx, "d");
    enter(cx);
    let edited = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&edited.params), ["a", "b", "c", "d"]);
    assert_eq!(edited.headers, [], "the other grid is untouched");
}

#[gpui::test]
fn add_under_an_empty_headers_grid(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "headers");
    assert!(
        bounds(cx, "request-grid-empty").is_none(),
        "the add row replaces the empty state"
    );
    assert!(
        bounds(cx, "request-headers-header").is_some(),
        "the grid shows its columns"
    );

    click(cx, "request-headers-add");
    let added = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&added.headers), [""]);
    assert_eq!(keys(&added.params), ["a", "b", "c"], "params untouched");
    assert!(
        bounds(cx, "request-headers-delete-1").is_some(),
        "the new row"
    );
}

#[gpui::test]
fn a_new_row_left_empty_stays(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-add");
    enter(cx);
    let added = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&added.params), ["a", "b", "c", ""], "no auto-prune");
}

#[gpui::test]
fn deleting_the_second_of_three_rows_keeps_the_first_and_third(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-delete-2");
    assert!(marker(cx, ROWS), "the delete marks the tab");
    let deleted = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&deleted.params), ["a", "c"]);
    assert!(bounds(cx, "request-params-delete-3").is_none(), "two rows");
}

#[gpui::test]
fn deleting_while_a_later_cell_is_open_commits_it_into_its_row(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-value-3");
    type_text(cx, "7");
    click(cx, "request-params-delete-1");

    let deleted = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&deleted.params), ["b", "c"]);
    assert_eq!(deleted.params[0].value, "2", "row b is not written to");
    assert_eq!(
        deleted.params[1].value, "7",
        "the typed text landed in row c"
    );
    assert_eq!(open_cell(cx, &shell), None, "no field over a shifted row");
}

#[gpui::test]
fn body_urlencoded_rows_add_and_delete(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "body");

    click(cx, "request-body-add");
    type_text(cx, "y");
    enter(cx);
    click(cx, "request-body-delete-1");

    let edited = draft(cx, &shell).expect("the draft");
    assert_eq!(
        edited.body,
        Body::Urlencoded {
            fields: vec![KeyValue::new("y", "")]
        }
    );
    assert_eq!(keys(&edited.params), ["a", "b", "c"], "params untouched");
}

#[gpui::test]
fn row_edits_survive_a_restart_and_ctrl_s_writes_them(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");
    click(cx, "request-params-delete-2");
    click(cx, "request-params-add");
    type_text(cx, "z");
    enter(cx);

    let (shell_2, cx) = render(cx, &dir, &state_path);
    let restored = draft(cx, &shell_2).expect("the draft was restored");
    assert_eq!(keys(&restored.params), ["a", "c", "z"]);

    open(cx, "params");
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let file = std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default();
    assert!(
        file.contains("key = \"z\"") && !file.contains("key = \"b\""),
        "the file was written: {file}"
    );
}

#[gpui::test]
fn toggling_and_editing_an_existing_row_still_writes_that_row(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-enabled-2");
    click(cx, "request-params-key-3");
    type_text(cx, "q");
    enter(cx);

    let edited = draft(cx, &shell).expect("the draft");
    assert_eq!(keys(&edited.params), ["a", "b", "q"]);
    assert!(!edited.params[1].enabled, "row 2 toggled");
    assert!(edited.params[0].enabled && edited.params[2].enabled);
}

#[gpui::test]
fn the_empty_value_cell_of_a_new_row_opens_on_click(cx: &mut TestAppContext) {
    let dir = rows_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, "params");

    click(cx, "request-params-add");
    type_text(cx, "d");
    enter(cx);
    let cell = bounds(cx, "request-params-value-4").expect("the value cell is drawn");
    assert!(
        cell.size.height > gpui::px(0.),
        "an empty cell keeps the row's height"
    );
    click(cx, "request-params-value-4");
    assert_eq!(
        open_cell(cx, &shell),
        Some(("params".to_owned(), 4, String::new())),
        "the empty value cell opens its field"
    );
    type_text(cx, "4");
    enter(cx);
    let edited = draft(cx, &shell).expect("the draft");
    assert_eq!(edited.params[3].value, "4");
}
