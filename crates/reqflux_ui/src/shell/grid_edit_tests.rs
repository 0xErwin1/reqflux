//! The enable boxes of the Params and Headers grids (S11): a click toggles the row's
//! `enabled` state in the draft on the spot, marks the tab and persists, the grid shows
//! the draft's rows when a draft exists, and a secret value stays masked.

use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::Request;
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000051";
const GRID: &str = "01J00000000000000000000052";
const WORKSPACE: &str = "01J00000000000000000000900";

/// A workspace whose collection `API` declares `base` and the valueless secret `token`,
/// holding `GET Grid` with two params and two headers, the first of which has a description and
/// the second of which references the secret.
pub(super) fn grid_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"https://h\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/api/grid.req.toml",
        &format!(
            "id = \"{GRID}\"\nname = \"Grid\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/grid\"\n\n\
             [[params]]\nkey = \"page\"\nvalue = \"1\"\n\n\
             [[params]]\nkey = \"q\"\nvalue = \"reqflux\"\n\n\
             [[headers]]\nkey = \"X-One\"\nvalue = \"1\"\ndescription = \"first header\"\n\n\
             [[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{token}}}}\"\n"
        ),
    );
    dir
}

/// Opens `GET Grid` in a tab, which lands on the Headers sub-tab.
pub(super) fn open_grid(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{GRID}"));
}

/// The request the draft of `id` holds, if it has one.
fn draft_request(cx: &mut TestAppContext, shell: &Entity<Shell>, id: &str) -> Option<Request> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(id)
            .map(|draft| draft.request.clone())
    })
}

/// The request the app state file stores as the draft of `id`, if it has one.
fn stored_request(state_path: &Path, id: &str) -> Option<Request> {
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    state
        .drafts(WORKSPACE)
        .unwrap_or_default()
        .into_iter()
        .find(|(entity, _)| entity == id)
        .map(
            |(_, stored)| match toml::from_str::<Request>(&stored.payload) {
                Ok(request) => request,
                Err(error) => panic!("the stored draft did not parse: {error}"),
            },
        )
}

#[gpui::test]
fn clicking_a_header_enable_box_marks_the_tab_and_persists_the_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    assert!(!marker(cx, GRID), "no marker without a draft");
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft yet");

    click(cx, "request-headers-enabled-2");
    assert!(marker(cx, GRID), "the toggle marks the draft");
    let draft = draft_request(cx, &shell, GRID).expect("the toggle created the draft");
    assert!(!draft.headers[1].enabled, "the row is unchecked");
    assert!(draft.headers[0].enabled, "the other row is untouched");

    // A second shell over the same workspace and state file finds the draft again.
    let (shell_2, cx) = render(cx, &dir, &state_path);
    let stored = stored_request(&state_path, GRID).expect("the draft was persisted");
    assert!(!stored.headers[1].enabled, "the store has the row disabled");
    let restored = draft_request(cx, &shell_2, GRID).expect("the draft was restored");
    assert!(!restored.headers[1].enabled);
    assert!(restored.headers[0].enabled);
}

#[gpui::test]
fn clicking_the_box_again_re_enables_the_row(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-enabled-2");
    click(cx, "request-headers-enabled-2");
    assert!(marker(cx, GRID), "the draft stays");
    let draft = draft_request(cx, &shell, GRID).expect("the draft stays");
    assert!(draft.headers[1].enabled, "the row is checked again");
    let stored = stored_request(&state_path, GRID).expect("the draft was persisted");
    assert!(stored.headers[1].enabled, "the store agrees");
}

#[gpui::test]
fn a_param_and_a_header_toggle_share_one_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-enabled-1");
    click(cx, "request-tab-params");
    click(cx, "request-params-enabled-2");
    let draft = draft_request(cx, &shell, GRID).expect("one draft holds both edits");
    assert!(!draft.headers[0].enabled, "the header edit is kept");
    assert!(!draft.params[1].enabled, "the param edit landed too");
    assert!(draft.params[0].enabled, "the other param is untouched");
}

#[gpui::test]
fn the_first_toggle_creates_the_draft_from_the_file(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    assert!(draft_request(cx, &shell, GRID).is_none());

    click(cx, "request-headers-enabled-2");
    let draft = draft_request(cx, &shell, GRID).expect("the toggle created the draft");
    assert_eq!(draft.url, "{{base}}/grid", "the file's URL");
    assert_eq!(draft.params.len(), 2, "the file's params");
    assert!(
        draft.params[0].enabled && draft.params[1].enabled,
        "the file's params"
    );
    assert_eq!(draft.headers.len(), 2, "the file's headers");
    assert!(draft.headers[0].enabled, "the file's first header");
    assert!(!draft.headers[1].enabled, "plus the change");
    let seeded = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(GRID)
            .map(|draft| draft.base_hash.is_some())
    });
    assert_eq!(
        seeded,
        Some(true),
        "seeded from the file, as the URL field does"
    );
}

#[gpui::test]
fn toggling_a_row_keeps_the_secret_masked(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    assert!(
        bounds(cx, "request-headers-secret-2").is_some(),
        "the secret value is masked before the toggle"
    );

    click(cx, "request-headers-enabled-1");
    assert!(
        bounds(cx, "request-headers-secret-2").is_some(),
        "the secret value is still masked after the toggle"
    );
    let draft = draft_request(cx, &shell, GRID).expect("the draft");
    assert_eq!(
        draft.headers[1].value, "Bearer {{token}}",
        "the stored reference is untouched"
    );
}

#[gpui::test]
fn the_body_grid_has_no_enable_boxes(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    click(cx, "request-tab-body");
    assert!(draftless_body_has_no_rows(cx), "the body grid has rows");
    assert!(
        bounds(cx, "request-body-enabled-1").is_none(),
        "body rows are not editable in this slice"
    );
}

/// The body of `GET Grid` shows its empty state, so the body grid never renders.
fn draftless_body_has_no_rows(cx: &mut VisualTestContext) -> bool {
    bounds(cx, "request-body-empty").is_some()
}

/// The text the open cell field holds, if one is open.
fn field_text(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|cx| {
        shell
            .read(cx)
            .cell_edit
            .as_ref()
            .map(|edit| edit.input.read(cx).value().to_string())
    })
}

/// Replaces the text of the open cell field with `text`: selects all and types.
fn type_cell(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("ctrl-a");
    let typed = text
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

#[gpui::test]
fn clicking_a_header_value_cell_opens_the_field_and_enter_persists(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft yet");

    click(cx, "request-headers-value-1");
    assert!(field_text(cx, &shell).is_some(), "the field is open");
    assert_eq!(
        field_text(cx, &shell).as_deref(),
        Some("1"),
        "seeded with the stored text"
    );

    type_cell(cx, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(field_text(cx, &shell).is_none(), "the field closed");
    assert!(marker(cx, GRID), "Enter marks the tab dirty");
    let draft = draft_request(cx, &shell, GRID).expect("Enter created the draft");
    assert_eq!(draft.headers[0].value, "42");
    assert_eq!(draft.headers[0].key, "X-One", "the key is untouched");

    // The grid shows the new value: clicking the cell again seeds it from the draft.
    click(cx, "request-headers-value-1");
    assert_eq!(field_text(cx, &shell).as_deref(), Some("42"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // A second shell over the same workspace and state file finds it persisted.
    let (_shell_2, _cx_2) = render(cx, &dir, &state_path);
    let stored = stored_request(&state_path, GRID).expect("the draft was persisted");
    assert_eq!(stored.headers[0].value, "42");
}

#[gpui::test]
fn a_param_key_edit_lands_in_the_same_draft_as_a_header_value_edit(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-value-1");
    type_cell(cx, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    click(cx, "request-tab-params");
    click(cx, "request-params-key-1");
    assert_eq!(
        field_text(cx, &shell).as_deref(),
        Some("page"),
        "seeded with the stored key"
    );
    type_cell(cx, "page_size");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let draft = draft_request(cx, &shell, GRID).expect("one draft holds both edits");
    assert_eq!(draft.headers[0].value, "42", "the header edit is kept");
    assert_eq!(draft.params[0].key, "page_size");
    assert_eq!(draft.params[0].value, "1", "the value is untouched");
}

#[gpui::test]
fn escape_in_the_cell_drops_the_text_and_creates_no_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-value-1");
    type_cell(cx, "junk");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(field_text(cx, &shell).is_none(), "the field closed");
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft");
    assert!(!marker(cx, GRID), "no marker without a draft");

    // The stored value is unchanged: the cell seeds from the file again.
    click(cx, "request-headers-value-1");
    assert_eq!(field_text(cx, &shell).as_deref(), Some("1"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui::test]
fn clicking_a_description_cell_opens_the_field_and_enter_persists(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-1");
    assert_eq!(
        field_text(cx, &shell).as_deref(),
        Some("first header"),
        "seeded with the stored description"
    );
    type_cell(cx, "renamed");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(field_text(cx, &shell).is_none(), "the field closed");
    let draft = draft_request(cx, &shell, GRID).expect("Enter created the draft");
    assert_eq!(draft.headers[0].description.as_deref(), Some("renamed"));
    assert_eq!(draft.headers[0].key, "X-One", "the key is untouched");
    assert_eq!(draft.headers[0].value, "1", "the value is untouched");

    click(cx, "request-headers-description-1");
    assert_eq!(field_text(cx, &shell).as_deref(), Some("renamed"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let (_shell_2, _cx_2) = render(cx, &dir, &state_path);
    let stored = stored_request(&state_path, GRID).expect("the draft was persisted");
    assert_eq!(stored.headers[0].description.as_deref(), Some("renamed"));
}

#[gpui::test]
fn a_row_without_a_description_opens_an_empty_field(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-2");
    assert_eq!(field_text(cx, &shell).as_deref(), Some(""));
    type_cell(cx, "bearer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let draft = draft_request(cx, &shell, GRID).expect("Enter created the draft");
    assert_eq!(draft.headers[1].description.as_deref(), Some("bearer"));
    assert_eq!(
        draft.headers[1].value, "Bearer {{token}}",
        "the secret reference is untouched"
    );
}

#[gpui::test]
fn escape_in_the_description_drops_the_text_and_creates_no_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-1");
    type_cell(cx, "junk");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(field_text(cx, &shell).is_none(), "the field closed");
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft");
    click(cx, "request-headers-description-1");
    assert_eq!(field_text(cx, &shell).as_deref(), Some("first header"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui::test]
fn committing_an_unchanged_description_creates_no_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-1");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(field_text(cx, &shell).is_none(), "the field closed");
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft");
}

#[gpui::test]
fn an_emptied_description_is_removed_from_the_row(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-1");
    cx.simulate_keystrokes("ctrl-a backspace enter");
    cx.run_until_parked();
    let draft = draft_request(cx, &shell, GRID).expect("the commit created the draft");
    assert_eq!(draft.headers[0].description, None);
}

#[gpui::test]
fn clicking_another_cell_commits_the_open_description(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-description-1");
    type_cell(cx, "kept");
    click(cx, "request-headers-key-2");
    assert_eq!(field_text(cx, &shell).as_deref(), Some("Authorization"));
    let draft = draft_request(cx, &shell, GRID).expect("the click committed the description");
    assert_eq!(draft.headers[0].description.as_deref(), Some("kept"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui::test]
fn a_secret_value_cell_opens_empty_and_keeps_the_reference(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-value-2");
    assert!(field_text(cx, &shell).is_some(), "the field is open");
    assert_eq!(
        field_text(cx, &shell).as_deref(),
        Some(""),
        "a secret value is never seeded"
    );
    assert!(
        bounds(cx, "request-headers-secret-2").is_some(),
        "the badge"
    );

    // Committing without typing keeps the stored reference and creates no draft.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(field_text(cx, &shell).is_none(), "the field closed");
    assert!(draft_request(cx, &shell, GRID).is_none(), "no draft");
    assert!(bounds(cx, "request-headers-secret-2").is_some(), "masked");
    let stored = stored_request(&state_path, GRID);
    assert!(stored.is_none(), "nothing was persisted");

    // Typing a replacement commits it.
    click(cx, "request-headers-value-2");
    type_cell(cx, "x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let draft = draft_request(cx, &shell, GRID).expect("the replacement");
    assert_eq!(draft.headers[1].value, "x");
}

#[gpui::test]
fn clicking_another_cell_commits_the_open_edit(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    click(cx, "request-headers-value-1");
    type_cell(cx, "42");
    click(cx, "request-headers-key-1");

    let draft = draft_request(cx, &shell, GRID).expect("the first edit landed");
    assert_eq!(draft.headers[0].value, "42", "no lost edit");
    assert_eq!(
        field_text(cx, &shell).as_deref(),
        Some("X-One"),
        "the key cell is open now"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui::test]
fn the_sub_tab_count_follows_the_draft(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{GRID}"));
    let shown = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let shell = shell.read(cx);
            let state = shell.requests.get(GRID)?;
            super::request::shown_rows(GRID, state, shell, super::request::SubTab::Headers)
        })
    };
    assert_eq!(shown(cx), Some(2), "both headers are enabled in the file");
    assert!(
        bounds(cx, "request-tab-headers-count").is_some(),
        "the count is addressable"
    );

    click(cx, "request-headers-enabled-2");
    assert_eq!(
        shown(cx),
        Some(1),
        "the number the sub-tab shows reads the draft"
    );
}
