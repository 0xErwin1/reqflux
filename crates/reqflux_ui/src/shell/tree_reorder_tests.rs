//! Reordering and moving tree items from the sidebar (tree-crud S8, S10): Alt+Up and
//! Alt+Down among siblings, and drag and drop onto a row or into a collection or folder.

use std::path::Path;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::sidebar_tests::{API, AUTH, HEALTH, LIST, TOKEN, USERS, sidebar_workspace};
use super::tree_model::{self, Node};
use crate::workspace_tests::write;

const INNER: &str = "01J0000000000000000000000A";

fn read(root: &Path, relative: &str) -> String {
    match std::fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("read {relative}: {error}"),
    }
}

/// The `order = [...]` line of a `collection.toml` or `folder.toml`, if it has one.
fn order_line(root: &Path, relative: &str) -> Option<String> {
    read(root, relative)
        .lines()
        .find(|line| line.starts_with("order"))
        .map(str::to_owned)
}

fn node(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Node {
    match cx.update(|_, cx| tree_model::find(&shell.read(cx).roots, id).cloned()) {
        Some(node) => node,
        None => panic!("node {id} is missing"),
    }
}

fn child_ids(node: &Node) -> Vec<&str> {
    node.children
        .iter()
        .map(|child| child.id.as_str())
        .collect()
}

fn selected(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).selected().map(str::to_owned)
    })
}

fn sidebar_error(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).error().map(str::to_owned)
    })
}

/// Drags the row `from` and drops it on the element `onto`.
fn drag(cx: &mut VisualTestContext, from: &str, onto: &str) {
    let start = present(cx, &format!("tree-row-{from}")).center();
    let end = present(cx, onto).center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x + px(8.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn alt_down_moves_the_selected_row_one_place(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{USERS}"));
    cx.simulate_keystrokes("alt-down");
    cx.run_until_parked();

    assert_eq!(
        order_line(dir.path(), "collections/api/collection.toml").as_deref(),
        Some("order = [\"health\", \"users\"]")
    );
    assert_eq!(child_ids(&node(cx, &shell, API)), vec![HEALTH, USERS]);
    assert_eq!(selected(cx, &shell).as_deref(), Some(USERS));
    present(cx, &format!("tree-row-{USERS}"));
    assert_eq!(sidebar_error(cx, &shell), None);

    cx.simulate_keystrokes("alt-up");
    cx.run_until_parked();
    assert_eq!(child_ids(&node(cx, &shell, API)), vec![USERS, HEALTH]);
}

#[gpui::test]
fn alt_up_at_the_first_place_writes_nothing(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let before = read(dir.path(), "collections/api/collection.toml");

    click(cx, &format!("tree-row-{USERS}"));
    cx.simulate_keystrokes("alt-up");
    cx.run_until_parked();
    // A collection is not reordered either.
    click(cx, &format!("tree-row-{AUTH}"));
    cx.simulate_keystrokes("alt-up");
    cx.run_until_parked();

    assert_eq!(read(dir.path(), "collections/api/collection.toml"), before);
    assert_eq!(
        order_line(dir.path(), "collections/auth/collection.toml"),
        None
    );
    assert_eq!(selected(cx, &shell).as_deref(), Some(AUTH));
    assert_eq!(sidebar_error(cx, &shell), None);
    assert!(bounds(cx, "sidebar-error").is_none());
}

#[gpui::test]
fn dragging_onto_a_row_in_another_collection_moves_it_before_that_row(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{TOKEN}"));
    present(cx, &format!("doc-tab-{TOKEN}"));

    drag(cx, TOKEN, &format!("tree-method-{HEALTH}"));

    assert!(dir.path().join("collections/api/token.req.toml").exists());
    assert!(!dir.path().join("collections/auth/token.req.toml").exists());
    assert_eq!(
        child_ids(&node(cx, &shell, API)),
        vec![USERS, TOKEN, HEALTH]
    );
    assert!(node(cx, &shell, AUTH).children.is_empty());
    assert_eq!(selected(cx, &shell).as_deref(), Some(TOKEN));
    present(cx, &format!("doc-tab-{TOKEN}"));
    let active = cx.update(|_, cx| shell.read(cx).documents.active().map(str::to_owned));
    assert_eq!(active.as_deref(), Some(TOKEN), "the open tab stays open");
}

#[gpui::test]
fn dropping_on_a_folder_name_puts_it_inside_at_the_end(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    drag(cx, HEALTH, &format!("tree-name-{USERS}"));

    assert!(
        dir.path()
            .join("collections/api/users/health.req.toml")
            .exists()
    );
    assert!(!dir.path().join("collections/api/health.req.toml").exists());
    assert_eq!(child_ids(&node(cx, &shell, USERS)), vec![LIST, HEALTH]);
    assert_eq!(selected(cx, &shell).as_deref(), Some(HEALTH));
    // The new parent expands, so the moved row shows.
    present(cx, &format!("tree-row-{HEALTH}"));
}

#[gpui::test]
fn dropping_a_folder_into_its_own_child_shows_the_error(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    write(
        dir.path(),
        "collections/api/users/inner/folder.toml",
        &format!("id = \"{INNER}\"\nname = \"inner\"\n"),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{USERS}-chevron"));
    let before = read(dir.path(), "collections/api/collection.toml");

    drag(cx, USERS, &format!("tree-name-{INNER}"));

    present(cx, "sidebar-error");
    assert!(!sidebar_error(cx, &shell).unwrap_or_default().is_empty());
    assert!(
        dir.path()
            .join("collections/api/users/folder.toml")
            .exists()
    );
    assert!(
        dir.path()
            .join("collections/api/users/inner/folder.toml")
            .exists()
    );
    assert_eq!(read(dir.path(), "collections/api/collection.toml"), before);
}

#[gpui::test]
fn a_collection_row_cannot_be_dragged(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    drag(cx, AUTH, &format!("tree-name-{USERS}"));

    assert!(dir.path().join("collections/auth/collection.toml").exists());
    assert_eq!(child_ids(&node(cx, &shell, USERS)), vec![LIST]);
    assert_eq!(sidebar_error(cx, &shell), None);
}

#[gpui::test]
fn plain_arrows_still_select_and_enter_still_opens(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let before = read(dir.path(), "collections/api/collection.toml");

    click(cx, &format!("tree-row-{USERS}"));
    cx.simulate_keystrokes("down");
    assert_eq!(selected(cx, &shell).as_deref(), Some(HEALTH));
    cx.simulate_keystrokes("up");
    assert_eq!(selected(cx, &shell).as_deref(), Some(USERS));
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();

    present(cx, &format!("doc-tab-{HEALTH}"));
    assert_eq!(read(dir.path(), "collections/api/collection.toml"), before);
}
