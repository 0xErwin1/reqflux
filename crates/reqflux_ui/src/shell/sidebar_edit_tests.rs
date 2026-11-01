//! The tree editing of design/tree-editing/01-context-menu-modal: the folder and request
//! menus with their keys and Move to…, duplicate opening into rename, the rename hints, the
//! delete dialog that lists what it removes and flags unsaved drafts, and the tree keys.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::super::Shell;
use super::super::draft_tests::{bounds, click, edit_url, present, render, state_file};
use super::super::sidebar::Sidebar;
use super::super::sidebar_tests::{API, AUTH, HEALTH, LIST, TOKEN, USERS, sidebar_workspace};
use super::super::tree_model::NodeKind;
use super::{DeletePrompted, delete_body, delete_button, delete_note, delete_title};

fn sidebar(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Entity<Sidebar> {
    cx.update(|_, cx| shell.read(cx).sidebar.clone())
}

fn rename_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    let sidebar = sidebar(cx, shell);
    cx.update(|_, cx| sidebar.read(cx).rename_text(cx))
}

fn rename_hint(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<&'static str> {
    let sidebar = sidebar(cx, shell);
    cx.update(|_, cx| sidebar.read(cx).rename_hint())
}

/// Opens the row menu of `id` with a right click, which selects the row without opening it.
fn right_click(cx: &mut VisualTestContext, id: &str) {
    let row = present(cx, &format!("tree-row-{id}"));
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn absent(cx: &mut VisualTestContext, selector: &str) {
    assert!(bounds(cx, selector).is_none(), "{selector} is shown");
}

fn exists(root: &Path, relative: &str) -> bool {
    root.join(relative).exists()
}

fn names(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    fn walk(nodes: &[super::super::tree_model::Node], out: &mut Vec<String>) {
        for node in nodes {
            out.push(node.name.clone());
            walk(&node.children, out);
        }
    }
    cx.update(|_, cx| {
        let mut out = Vec::new();
        walk(&shell.read(cx).roots, &mut out);
        out
    })
}

#[gpui::test]
fn the_folder_menu_follows_the_artboard(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, USERS);
    for item in [
        "new-request",
        "new-folder",
        "rename",
        "duplicate",
        "move-to",
        "delete",
    ] {
        present(cx, &format!("tree-menu-{item}"));
    }
    absent(cx, "tree-menu-open");
    present(cx, "tree-menu-separator-0");
    present(cx, "tree-menu-separator-1");
    for key in ["new-request", "rename", "duplicate", "delete"] {
        present(cx, &format!("tree-menu-key-{key}"));
    }

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    right_click(cx, API);
    present(cx, "tree-menu-new-folder");
    absent(cx, "tree-menu-duplicate");
    absent(cx, "tree-menu-move-to");
}

#[gpui::test]
fn the_request_menu_opens_renames_duplicates_moves_and_deletes(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, HEALTH);
    for item in ["open", "rename", "duplicate", "move-to", "delete"] {
        present(cx, &format!("tree-menu-{item}"));
    }
    present(cx, "tree-menu-key-open");
    absent(cx, "tree-menu-new-request");
    absent(cx, "tree-menu-new-folder");
    absent(cx, &format!("doc-tab-{HEALTH}"));

    click(cx, "tree-menu-open");
    present(cx, &format!("doc-tab-{HEALTH}"));
    absent(cx, "tree-menu");
}

#[gpui::test]
fn duplicate_opens_the_copy_into_rename(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, HEALTH);
    click(cx, "tree-menu-duplicate");

    assert!(exists(dir.path(), "collections/api/health-copy.req.toml"));
    present(cx, "tree-rename-input");
    present(cx, "tree-rename-hint");
    assert_eq!(rename_text(cx, &shell).as_deref(), Some("Health copy"));
    assert_eq!(rename_hint(cx, &shell), Some("Enter saves · Esc cancels"));

    // Escape keeps the copy under its default name.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    absent(cx, "tree-rename-hint");
    assert!(exists(dir.path(), "collections/api/health-copy.req.toml"));
}

#[gpui::test]
fn a_new_folder_is_named_inline_and_escape_cancels_it(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, USERS);
    click(cx, "tree-menu-new-folder");
    assert!(exists(
        dir.path(),
        "collections/api/users/new-folder/folder.toml"
    ));
    present(cx, "tree-rename-hint");
    assert_eq!(rename_hint(cx, &shell), Some("Enter creates · Esc cancels"));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!exists(dir.path(), "collections/api/users/new-folder"));
    assert!(!names(cx, &shell).iter().any(|name| name == "New folder"));
    absent(cx, "tree-rename-input");
}

#[gpui::test]
fn escape_on_a_rename_keeps_an_existing_item(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, USERS);
    click(cx, "tree-menu-rename");
    assert_eq!(rename_hint(cx, &shell), Some("Enter saves · Esc cancels"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(exists(dir.path(), "collections/api/users/folder.toml"));
    assert!(names(cx, &shell).iter().any(|name| name == "users"));
}

#[gpui::test]
fn the_delete_dialog_lists_the_requests_and_flags_drafts(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let sidebar = sidebar(cx, &shell);
    let prompted = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let prompted = Rc::clone(&prompted);
        cx.subscribe(&sidebar, move |_, event: &DeletePrompted, _| {
            prompted.borrow_mut().push(event.ids.clone());
        })
    });

    right_click(cx, API);
    click(cx, "tree-menu-delete");
    assert_eq!(
        prompted.borrow().as_slice(),
        &[vec![LIST.to_owned(), HEALTH.to_owned()]],
        "the shell learns which requests the dialog lists"
    );
    sidebar.update(cx, |sidebar, cx| {
        sidebar.set_unsaved(HashSet::from([LIST.to_owned(), TOKEN.to_owned()]), cx);
    });
    cx.run_until_parked();

    present(cx, "tree-delete-prompt");
    present(cx, "tree-delete-backdrop");
    present(cx, "tree-delete-title");
    present(cx, "tree-delete-body");
    present(cx, "tree-delete-note");
    present(cx, &format!("tree-delete-item-{LIST}"));
    present(cx, &format!("tree-delete-item-{HEALTH}"));
    present(cx, &format!("tree-delete-draft-{LIST}"));
    absent(cx, &format!("tree-delete-draft-{HEALTH}"));
    absent(cx, &format!("tree-delete-item-{TOKEN}"));
    // The Esc keycap is the one `bolt`'s Button draws, which has no element id yet.
    click(cx, "tree-delete-cancel");

    // The draft warns but does not block.
    right_click(cx, USERS);
    click(cx, "tree-menu-delete");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.set_unsaved(HashSet::from([LIST.to_owned()]), cx);
    });
    cx.run_until_parked();
    present(cx, &format!("tree-delete-draft-{LIST}"));
    click(cx, "tree-delete-confirm");
    assert!(!exists(dir.path(), "collections/api/users"));
    absent(cx, "tree-delete-prompt");
}

#[gpui::test]
fn the_delete_dialog_closes_on_cancel_and_on_the_backdrop(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, AUTH);
    click(cx, "tree-menu-delete");
    click(cx, "tree-delete-cancel");
    absent(cx, "tree-delete-prompt");

    right_click(cx, AUTH);
    click(cx, "tree-menu-delete");
    let backdrop = present(cx, "tree-delete-backdrop");
    cx.simulate_click(backdrop.origin, Modifiers::none());
    cx.run_until_parked();
    absent(cx, "tree-delete-prompt");
    assert!(exists(dir.path(), "collections/auth/token.req.toml"));
}

#[gpui::test]
fn move_to_lists_the_other_containers_and_moves_there(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, HEALTH);
    click(cx, "tree-menu-move-to");
    present(cx, "tree-menu");
    present(cx, &format!("tree-move-{AUTH}"));
    present(cx, &format!("tree-move-{USERS}"));
    absent(cx, &format!("tree-move-{API}"));
    absent(cx, "tree-menu-rename");

    click(cx, &format!("tree-move-{AUTH}"));
    assert!(exists(dir.path(), "collections/auth/health.req.toml"));
    assert!(!exists(dir.path(), "collections/api/health.req.toml"));
    absent(cx, "tree-menu");
}

#[gpui::test]
fn move_to_never_offers_a_folder_its_own_subtree(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    right_click(cx, USERS);
    click(cx, "tree-menu-move-to");
    present(cx, &format!("tree-move-{AUTH}"));
    absent(cx, &format!("tree-move-{USERS}"));
    absent(cx, &format!("tree-move-{API}"));
}

#[gpui::test]
fn tree_keys_rename_duplicate_and_delete_the_selection(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{USERS}"));
    cx.simulate_keystrokes("f2");
    cx.run_until_parked();
    assert_eq!(rename_text(cx, &shell).as_deref(), Some("users"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    present(cx, "tree-delete-prompt");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    absent(cx, "tree-delete-prompt");

    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("ctrl-d");
    cx.run_until_parked();
    assert!(exists(dir.path(), "collections/api/health-copy.req.toml"));
}

#[test]
fn the_delete_copy_names_what_it_removes() {
    let folder = NodeKind::Folder;
    let request = NodeKind::Request {
        method: "GET".into(),
    };
    assert_eq!(delete_title(&folder, "invoices"), "Delete folder invoices?");
    assert_eq!(delete_title(&request, "Health"), "Delete request Health?");

    let (body, bold) = delete_body(&folder, "invoices", 4);
    assert_eq!(
        body,
        "The folder invoices holds 4 requests. Deleting it removes the folder and its \
         request files from the workspace."
    );
    let spans: Vec<&str> = bold
        .iter()
        .filter_map(|range| body.get(range.clone()))
        .collect();
    assert_eq!(spans, ["invoices", "4 requests"]);
    assert_eq!(
        delete_body(&NodeKind::Collection, "Auth", 0).0,
        "The collection Auth holds no requests. Deleting it removes the collection from the \
         workspace."
    );
    assert_eq!(
        delete_body(&request, "Health", 1).0,
        "Deleting the request Health removes its file from the workspace."
    );

    assert_eq!(
        delete_note(4, 1).as_deref(),
        Some("History keeps the sends of these requests. The unsaved draft is discarded.")
    );
    assert_eq!(
        delete_note(1, 0).as_deref(),
        Some("History keeps the sends of this request.")
    );
    assert_eq!(
        delete_note(3, 2).as_deref(),
        Some("History keeps the sends of these requests. The 2 unsaved drafts are discarded.")
    );
    assert_eq!(delete_note(0, 0), None);

    assert_eq!(delete_button(&folder, 4), "Delete folder and 4 requests");
    assert_eq!(
        delete_button(&NodeKind::Collection, 1),
        "Delete collection and 1 request"
    );
    assert_eq!(delete_button(&folder, 0), "Delete folder");
    assert_eq!(delete_button(&request, 1), "Delete request");
}

#[gpui::test]
fn the_delete_dialog_flags_a_request_edited_in_the_window(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{HEALTH}"));
    edit_url(cx, "https://example.test/edited");
    right_click(cx, API);
    click(cx, "tree-menu-delete");

    present(cx, &format!("tree-delete-draft-{HEALTH}"));
    absent(cx, &format!("tree-delete-draft-{LIST}"));
}
