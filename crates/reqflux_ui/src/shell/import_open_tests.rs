//! Opening a request the window's Postman import wrote, in a workspace created from the
//! workspace menu and in one opened normally: a click, Enter or the row menu's Open opens
//! its tab.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::super::sidebar::OpenRequest;
use super::super::toasts::tests as toasts;
use super::super::tree_model::Node;
use super::super::workspace_picker::TestPicker;
use super::super::workspace_switch_tests::{
    bounds, canonical, click, pair, present, recents, render, temp_dir,
};
use super::super::{CANNOT_OPEN_REQUEST, Shell};
use super::TestImportPicker;

/// A collection with nested folders, folder auth, a request with a test script and
/// collection variables, as Postman exports them.
const PIX: &str = r#"{
  "info": { "_postman_id": "pix-1", "name": "Pix Adapter",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  "item": [
    { "name": "0. Auth",
      "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}", "type": "string" }] },
      "item": [
        { "name": "Get token",
          "event": [{ "listen": "test", "script": { "type": "text/javascript",
                      "exec": ["pm.collectionVariables.set('token', pm.response.json().token);"] } }],
          "request": { "method": "POST",
                       "header": [{ "key": "Content-Type", "value": "application/json" }],
                       "body": { "mode": "raw", "raw": "{\"client\":\"{{clientId}}\"}",
                                 "options": { "raw": { "language": "json" } } },
                       "url": { "raw": "{{baseUrl}}/auth/token", "host": ["{{baseUrl}}"],
                                "path": ["auth", "token"] } },
          "response": [] }
      ] },
    { "name": "1. Resolve (MODO)",
      "item": [
        { "name": "Resolve",
          "request": { "method": "GET", "auth": { "type": "inherit" },
                       "url": "{{baseUrl}}/resolve?key={{key}}" } }
      ] }
  ],
  "auth": { "type": "apikey", "apikey": [{ "key": "key", "value": "x-api" }, { "key": "value", "value": "{{apiKey}}" }] },
  "variable": [
    { "key": "baseUrl", "value": "https://pix.example.com" },
    { "key": "clientId", "value": "client" },
    { "key": "apiKey", "value": "", "type": "secret" }
  ]
}"#;

/// The collection file in a fresh directory, kept alive by the returned guard.
fn collection_file() -> (TempDir, PathBuf) {
    let dir = temp_dir();
    let path = dir.path().join("pix.postman_collection.json");
    if let Err(error) = std::fs::write(&path, PIX) {
        panic!("write {}: {error}", path.display());
    }
    (dir, path)
}

/// Imports `file` through the workspace menu's dialog.
fn import(cx: &mut VisualTestContext, shell: &Entity<Shell>, file: PathBuf) {
    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker {
            file: Some(file),
            ..TestImportPicker::default()
        });
    });
    click(cx, "workspace-selector");
    click(cx, "workspace-import");
    click(cx, "import-confirm");
    assert!(bounds(cx, "import-backdrop").is_none(), "the import ran");
}

/// The id of the sidebar row named `name`.
fn row_id(cx: &mut VisualTestContext, shell: &Entity<Shell>, name: &str) -> String {
    fn find(nodes: &[Node], name: &str) -> Option<String> {
        nodes.iter().find_map(|node| {
            (node.name == name)
                .then(|| node.id.clone())
                .or_else(|| find(&node.children, name))
        })
    }
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());
    cx.update(|_, cx| find(sidebar.read(cx).roots(), name))
        .unwrap_or_else(|| panic!("no row named {name}"))
}

fn tab_open(cx: &mut VisualTestContext, id: &str) -> bool {
    bounds(cx, &format!("doc-tab-{id}")).is_some()
}

/// Creates the workspace `Houlak` from the workspace menu and imports the collection.
fn new_workspace_with_import(
    cx: &mut TestAppContext,
) -> (
    super::super::workspace_switch_tests::Pair,
    TempDir,
    TempDir,
    Entity<Shell>,
    &mut VisualTestContext,
) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (file_dir, file) = collection_file();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let root = managed.path().to_path_buf();
    shell.update(cx, |shell, _| shell.managed_dir = Some(root));
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    cx.simulate_input("Houlak");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    import(cx, &shell, file);
    (pair, managed, file_dir, shell, cx)
}

/// Expands the collection and the folder that hold `Get token`, returning its id.
fn reveal_token(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    let collection = row_id(cx, shell, "Pix Adapter");
    let folder = row_id(cx, shell, "0. Auth");
    let token = row_id(cx, shell, "Get token");
    for (parent, child) in [(&collection, &folder), (&folder, &token)] {
        if bounds(cx, &format!("tree-row-{child}")).is_none() {
            // A click selects a container; Enter expands it.
            click(cx, &format!("tree-row-{parent}"));
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
        }
    }
    present(cx, &format!("tree-row-{token}"));
    token
}

#[gpui::test]
fn a_click_opens_a_request_imported_into_a_new_workspace(cx: &mut TestAppContext) {
    let (_pair, _managed, _file, shell, cx) = new_workspace_with_import(cx);
    let token = reveal_token(cx, &shell);
    click(cx, &format!("tree-row-{token}"));
    assert!(tab_open(cx, &token), "the click opens the request's tab");
}

#[gpui::test]
fn enter_and_the_row_menu_open_a_request_imported_into_a_new_workspace(cx: &mut TestAppContext) {
    let (_pair, _managed, _file, shell, cx) = new_workspace_with_import(cx);
    let token = reveal_token(cx, &shell);
    let row = present(cx, &format!("tree-row-{token}"));
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    click(cx, "tree-menu-open");
    assert!(tab_open(cx, &token), "Open in the row menu opens the tab");

    click(cx, &format!("doc-tab-close-{token}"));
    assert!(!tab_open(cx, &token));
    let row = present(cx, &format!("tree-row-{token}"));
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        tab_open(cx, &token),
        "Enter on the selected row opens the tab"
    );
}

#[gpui::test]
fn the_new_workspace_is_the_newest_recent_after_the_import(cx: &mut TestAppContext) {
    let (pair, managed, _file, _shell, _cx) = new_workspace_with_import(cx);
    let dirs: Vec<PathBuf> = match std::fs::read_dir(managed.path()) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect(),
        Err(error) => panic!("read_dir: {error}"),
    };
    let [dir] = dirs.as_slice() else {
        panic!("one managed workspace expected: {dirs:?}");
    };
    assert_eq!(
        recents(&pair.state_path).first(),
        Some(&(canonical(dir), "Houlak".to_owned()))
    );
}

#[gpui::test]
fn a_click_opens_a_request_imported_into_the_open_workspace(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = collection_file();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    import(cx, &shell, file);
    let token = reveal_token(cx, &shell);
    click(cx, &format!("tree-row-{token}"));
    assert!(tab_open(cx, &token));
}

#[gpui::test]
fn a_click_opens_a_request_of_a_workspace_opened_normally(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let request = cx.update(|_, cx| {
        fn first(nodes: &[Node]) -> Option<String> {
            nodes.iter().find_map(|node| match node.kind {
                super::super::tree_model::NodeKind::Request { .. } => Some(node.id.clone()),
                _ => first(&node.children),
            })
        }
        first(&shell.read(cx).roots)
    });
    let request = request.unwrap_or_else(|| panic!("the workspace has a request"));
    if bounds(cx, &format!("tree-row-{request}")).is_none() {
        let collection =
            cx.update(|_, cx| shell.read(cx).roots.first().map(|root| root.id.clone()));
        click(cx, &format!("tree-row-{}", collection.unwrap_or_default()));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
    }
    click(cx, &format!("tree-row-{request}"));
    assert!(tab_open(cx, &request));
}

#[gpui::test]
fn a_request_that_cannot_open_says_so_in_a_toast(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let missing = "01J00000000000000000000Z99";
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(OpenRequest(missing.to_owned())));
    cx.run_until_parked();

    assert!(!tab_open(cx, missing));
    let open = cx.update(|_, cx| shell.read(cx).documents.open_ids().to_vec());
    assert!(open.is_empty(), "nothing opens: {open:?}");
    assert_eq!(
        toasts::pushed(cx),
        vec![toasts::error(
            CANNOT_OPEN_REQUEST,
            &format!("The request {missing} is not in the workspace tree.")
        )]
    );
}
