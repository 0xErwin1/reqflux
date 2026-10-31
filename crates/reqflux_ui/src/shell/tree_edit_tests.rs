//! Tree editing from the sidebar (tree-crud S6, S8, S9): the `+` collection button, the row
//! menu, inline rename, duplicate, the delete confirmation and what follows a delete.

use std::path::Path;
use std::sync::Arc;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::sidebar_edit::delete_question;
use super::sidebar_tests::{API, AUTH, HEALTH, TOKEN, USERS, sidebar_workspace};
use super::tree_model::{Node, NodeKind};
use crate::workspace::OpenedWorkspace;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

/// Every node of the shell's trees, depth first.
fn all_nodes(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<Node> {
    fn walk(nodes: &[Node], out: &mut Vec<Node>) {
        for node in nodes {
            out.push(node.clone());
            walk(&node.children, out);
        }
    }
    cx.update(|_, cx| {
        let mut out = Vec::new();
        walk(&shell.read(cx).roots, &mut out);
        out
    })
}

fn node_named(cx: &mut VisualTestContext, shell: &Entity<Shell>, name: &str) -> Option<Node> {
    all_nodes(cx, shell)
        .into_iter()
        .find(|node| node.name == name)
}

fn node(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Node {
    match all_nodes(cx, shell).into_iter().find(|node| node.id == id) {
        Some(node) => node,
        None => panic!("node {id} is missing"),
    }
}

fn sidebar_error(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).error().map(str::to_owned)
    })
}

fn selected(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).selected().map(str::to_owned)
    })
}

/// The text of the rename field, when it is open.
fn rename_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let sidebar = shell.read(cx).sidebar.clone();
        sidebar.read(cx).rename_text(cx)
    })
}

/// Opens the row menu of `id` through its `…` button, selecting the row first.
fn open_menu(cx: &mut VisualTestContext, id: &str) {
    click(cx, &format!("tree-row-{id}"));
    click(cx, &format!("tree-row-menu-{id}"));
}

/// Types `text` into the focused field, replacing what it holds.
fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn exists(root: &Path, relative: &str) -> bool {
    root.join(relative).exists()
}

#[gpui::test]
fn plus_creates_a_collection_and_opens_rename(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, "sidebar-new-collection");

    assert!(exists(
        dir.path(),
        "collections/new-collection/collection.toml"
    ));
    let created = match node_named(cx, &shell, "New collection") {
        Some(node) => node,
        None => panic!("New collection is not in the tree"),
    };
    assert_eq!(created.kind, NodeKind::Collection);
    assert_eq!(selected(cx, &shell).as_deref(), Some(created.id.as_str()));
    present(cx, "tree-rename-input");
    assert_eq!(rename_text(cx, &shell).as_deref(), Some("New collection"));
    assert_eq!(sidebar_error(cx, &shell), None);
}

#[gpui::test]
fn new_request_from_the_menu_is_last_selected_and_on_disk(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, API);
    present(cx, "tree-menu-new-folder");
    present(cx, "tree-menu-rename");
    present(cx, "tree-menu-delete");
    assert!(
        bounds(cx, "tree-menu-duplicate").is_none(),
        "a collection has no Duplicate"
    );
    click(cx, "tree-menu-new-request");

    assert!(exists(dir.path(), "collections/api/new-request.req.toml"));
    let api = node(cx, &shell, API);
    let last = match api.children.last() {
        Some(last) => last.clone(),
        None => panic!("API has no children"),
    };
    assert_eq!(last.name, "New request");
    assert_eq!(selected(cx, &shell).as_deref(), Some(last.id.as_str()));
    assert!(bounds(cx, "tree-menu").is_none(), "the menu closes");
    present(cx, "tree-rename-input");
}

#[gpui::test]
fn new_folder_in_a_folder_expands_it(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, USERS);
    click(cx, "tree-menu-new-folder");

    assert!(exists(
        dir.path(),
        "collections/api/users/new-folder/folder.toml"
    ));
    let created = match node_named(cx, &shell, "New folder") {
        Some(node) => node,
        None => panic!("New folder is not in the tree"),
    };
    present(cx, &format!("tree-row-{}", created.id));
    assert_eq!(selected(cx, &shell).as_deref(), Some(created.id.as_str()));
}

#[gpui::test]
fn rename_moves_the_file_and_keeps_the_open_tab(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    // Opening the request through its row keeps it as the active tab.
    click(cx, &format!("tree-row-{HEALTH}"));
    present(cx, &format!("doc-tab-{HEALTH}"));
    click(cx, &format!("tree-row-menu-{HEALTH}"));
    click(cx, "tree-menu-rename");
    assert_eq!(rename_text(cx, &shell).as_deref(), Some("Health"));
    type_text(cx, "Status");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(exists(dir.path(), "collections/api/status.req.toml"));
    assert!(!exists(dir.path(), "collections/api/health.req.toml"));
    assert!(bounds(cx, "tree-rename-input").is_none());
    assert_eq!(node(cx, &shell, HEALTH).name, "Status");
    let active = cx.update(|_, cx| shell.read(cx).documents.active().map(str::to_owned));
    assert_eq!(
        active.as_deref(),
        Some(HEALTH),
        "the tab survives the rename"
    );
    let label = cx.update(|_, cx| {
        let shell = shell.read(cx);
        super::documents::locate(&shell.roots, HEALTH).map(|location| location.name)
    });
    assert_eq!(
        label.as_deref(),
        Some("Status"),
        "the tab shows the new name"
    );
    assert!(
        cx.update(|_, cx| shell.read(cx).requests.contains_key(HEALTH)),
        "the request is still inspected"
    );
}

#[gpui::test]
fn escape_cancels_a_rename(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, USERS);
    click(cx, "tree-menu-rename");
    type_text(cx, "people");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(bounds(cx, "tree-rename-input").is_none());
    assert!(exists(dir.path(), "collections/api/users/folder.toml"));
    assert_eq!(node(cx, &shell, USERS).name, "users");
}

#[gpui::test]
fn rename_to_a_taken_name_shows_the_error_and_changes_nothing(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, API);
    click(cx, "tree-menu-new-request");
    type_text(cx, "Health");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    present(cx, "sidebar-error");
    let error = sidebar_error(cx, &shell).unwrap_or_default();
    assert!(!error.is_empty(), "the error names the reason");
    assert!(exists(dir.path(), "collections/api/new-request.req.toml"));
    assert!(node_named(cx, &shell, "New request").is_some());
    assert_eq!(
        all_nodes(cx, &shell)
            .iter()
            .filter(|node| node.name == "Health")
            .count(),
        1
    );

    // Escape clears the error.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "sidebar-error").is_none());
}

#[gpui::test]
fn duplicate_creates_a_copy_next_to_the_original(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, HEALTH);
    click(cx, "tree-menu-duplicate");

    assert!(exists(dir.path(), "collections/api/health-copy.req.toml"));
    let copy = match node_named(cx, &shell, "Health copy") {
        Some(copy) => copy,
        None => panic!("Health copy is not in the tree"),
    };
    assert_ne!(copy.id, HEALTH);
    assert_eq!(selected(cx, &shell).as_deref(), Some(copy.id.as_str()));
}

#[gpui::test]
fn delete_asks_and_only_the_confirm_deletes(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{TOKEN}"));
    shell.update(cx, |shell, cx| {
        shell.edit_draft(
            TOKEN,
            |request| request.url = "https://edited.test".into(),
            cx,
        );
    });
    let stored = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let shell = shell.read(cx);
            match (shell.state.as_ref(), shell.workspace_id.as_deref()) {
                (Some(state), Some(workspace)) => state
                    .drafts(workspace)
                    .map(|drafts| drafts.iter().any(|(id, _)| id == TOKEN))
                    .unwrap_or(false),
                _ => false,
            }
        })
    };
    assert!(stored(cx), "the draft is stored");

    open_menu(cx, AUTH);
    click(cx, "tree-menu-delete");
    present(cx, "tree-delete-prompt");
    click(cx, "tree-delete-cancel");
    assert!(bounds(cx, "tree-delete-prompt").is_none());
    assert!(exists(dir.path(), "collections/auth/token.req.toml"));
    present(cx, &format!("doc-tab-{TOKEN}"));

    open_menu(cx, AUTH);
    click(cx, "tree-menu-delete");
    click(cx, "tree-delete-confirm");

    assert!(!exists(dir.path(), "collections/auth"));
    assert!(bounds(cx, &format!("tree-row-{AUTH}")).is_none());
    assert!(
        bounds(cx, &format!("doc-tab-{TOKEN}")).is_none(),
        "its tab closes"
    );
    assert!(
        cx.update(|_, cx| !shell.read(cx).drafts.contains_key(TOKEN)),
        "its draft is dropped"
    );
    assert!(!stored(cx), "its stored draft is deleted");
}

#[test]
fn the_delete_question_names_the_item_and_its_requests() {
    assert_eq!(
        delete_question(&NodeKind::Folder, "Invoices", 3),
        "Delete folder \"Invoices\" and its 3 requests?"
    );
    assert_eq!(
        delete_question(&NodeKind::Collection, "Auth", 1),
        "Delete collection \"Auth\" and its 1 request?"
    );
    assert_eq!(
        delete_question(&NodeKind::Folder, "Empty", 0),
        "Delete folder \"Empty\"?"
    );
    assert_eq!(
        delete_question(
            &NodeKind::Request {
                method: "GET".into()
            },
            "Health",
            1
        ),
        "Delete request \"Health\"?"
    );
}

#[gpui::test]
fn an_operation_while_a_send_prepares_shows_the_busy_error(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    // A send's prepare task holds a clone of the engine while it runs.
    let held = cx.update(|_, cx| match &shell.read(cx).workspace {
        OpenedWorkspace::Loaded(engine) => Some(Arc::clone(engine)),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
    });
    assert!(held.is_some());

    click(cx, "sidebar-new-collection");

    assert_eq!(
        sidebar_error(cx, &shell).as_deref(),
        Some("Wait for the request being sent, then try again")
    );
    present(cx, "sidebar-error");
    assert!(!exists(dir.path(), "collections/new-collection"));
    assert!(bounds(cx, "tree-rename-input").is_none());
    drop(held);
}

#[gpui::test]
fn right_click_opens_the_row_menu(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    let row = present(cx, &format!("tree-row-{USERS}"));
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    present(cx, "tree-menu");
    present(cx, "tree-menu-new-folder");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "tree-menu").is_none(), "Escape closes the menu");
}

#[gpui::test]
fn clicking_a_request_row_still_opens_it_and_keys_still_navigate(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{HEALTH}"));
    present(cx, &format!("doc-tab-{HEALTH}"));
    cx.simulate_keystrokes("up");
    assert_eq!(selected(cx, &shell).as_deref(), Some(USERS));
    cx.simulate_keystrokes("down down");
    assert_eq!(selected(cx, &shell).as_deref(), Some(AUTH));
}

#[gpui::test]
fn a_click_outside_commits_a_new_collection_name(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, "sidebar-new-collection");
    type_text(cx, "Billing");
    click(cx, "sidebar-footer");

    assert!(
        bounds(cx, "tree-rename-input").is_none(),
        "the field closes"
    );
    let created = match node_named(cx, &shell, "Billing") {
        Some(node) => node,
        None => panic!("Billing is not in the tree"),
    };
    assert!(exists(dir.path(), "collections/billing/collection.toml"));
    present(cx, &format!("tree-row-{}", created.id));
    assert_eq!(rename_text(cx, &shell), None);
}

#[gpui::test]
fn a_click_on_another_row_commits_a_rename(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_menu(cx, USERS);
    click(cx, "tree-menu-rename");
    type_text(cx, "people");
    click(cx, &format!("tree-row-{API}"));

    assert!(bounds(cx, "tree-rename-input").is_none());
    assert_eq!(node(cx, &shell, USERS).name, "people");
    assert!(exists(dir.path(), "collections/api/people/folder.toml"));
}

/// Right-clicks the row `id` at `offset` from its top-left corner; returns the point.
fn right_click_at(
    cx: &mut VisualTestContext,
    id: &str,
    offset: gpui::Point<gpui::Pixels>,
) -> gpui::Point<gpui::Pixels> {
    let row = present(cx, &format!("tree-row-{id}"));
    let at = row.origin + offset;
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    at
}

fn inside_window(cx: &mut VisualTestContext, menu: gpui::Bounds<gpui::Pixels>) {
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!(menu.origin.x >= gpui::px(0.) && menu.origin.y >= gpui::px(0.));
    assert!(
        menu.bottom_right().x <= viewport.width && menu.bottom_right().y <= viewport.height,
        "the menu {menu:?} leaves the window {viewport:?}"
    );
}

#[gpui::test]
fn right_click_opens_the_menu_at_the_pointer(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    let at = right_click_at(cx, USERS, gpui::point(gpui::px(40.), gpui::px(6.)));
    let menu = present(cx, "tree-menu");
    assert_eq!(menu.origin, at);
    inside_window(cx, menu);

    click(cx, "tree-menu-move-to");
    assert_eq!(
        present(cx, "tree-menu").origin,
        at,
        "Move to… stays where the menu opened"
    );
}

#[gpui::test]
fn the_row_button_opens_the_menu_at_the_button(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{USERS}"));
    let button = present(cx, &format!("tree-row-menu-{USERS}"));
    click(cx, &format!("tree-row-menu-{USERS}"));
    let menu = present(cx, "tree-menu");
    assert_eq!(menu.origin, button.center());
    inside_window(cx, menu);
}

#[gpui::test]
fn a_menu_near_the_window_edge_stays_inside(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    // The menu opened on the row at full size, then the window shrinks under it.
    let at = right_click_at(cx, API, gpui::point(gpui::px(40.), gpui::px(6.)));
    let menu = present(cx, "tree-menu");
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.simulate_resize(gpui::size(viewport.width, at.y + menu.size.height / 2.));
    cx.run_until_parked();

    let menu = present(cx, "tree-menu");
    assert!(menu.origin.y < at.y, "the menu moves up into the window");
    inside_window(cx, menu);
}
