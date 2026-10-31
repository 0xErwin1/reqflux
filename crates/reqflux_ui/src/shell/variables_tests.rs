//! Environments on the workspace settings page (variables-editor S6, design-catch-up T7):
//! the rail's Environments button lists Globals, the collections and the environments, and
//! creates, renames and deletes environments.

use std::fs;
use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::{Server, closed_port, serve, settle};
use crate::workspace_tests::write;

pub(crate) const COLLECTION: &str = "01J000000000000000000000A1";
pub(crate) const USE: &str = "01J000000000000000000000A2";
pub(crate) const DEV: &str = "01J000000000000000000000A3";
pub(crate) const STAGING: &str = "01J000000000000000000000A4";

/// A workspace whose globals declare `greeting = "hello"`, whose collection `API` declares
/// `base` at `port` and holds `GET Use` at `{{base}}/use/{{greeting}}`, with the
/// environments `dev` (declaring `token`) and `staging`.
pub(crate) fn variables_workspace(port: u16) -> TempDir {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        "id = \"01J00000000000000000000900\"\nname = \"Demo\"\nformat = 1\n\n\
         [[globals]]\nkey = \"greeting\"\nvalue = \"hello\"\n",
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://127.0.0.1:{port}\"\n"
        ),
    );
    write(
        root,
        "collections/api/use.req.toml",
        &format!(
            "id = \"{USE}\"\nname = \"Use\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/use/{{{{greeting}}}}\"\n"
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{DEV}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"t\"\n"
        ),
    );
    write(
        root,
        "environments/staging.env.toml",
        &format!("id = \"{STAGING}\"\nname = \"staging\"\n"),
    );
    dir
}

pub(crate) fn read(root: &Path, relative: &str) -> String {
    match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("read {relative}: {error}"),
    }
}

/// Types `text` into the focused field, replacing what it holds.
pub(crate) fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(text);
    cx.run_until_parked();
}

pub(crate) fn enter(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

pub(crate) fn active(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).active_environment.clone())
}

pub(crate) fn select_environment(cx: &mut VisualTestContext, shell: &Entity<Shell>, slug: &str) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_environment(Some(slug.to_owned()), cx);
        });
    });
    cx.run_until_parked();
}

/// Sends `GET Use` from the tree and returns the URL it sent.
pub(crate) fn send_use(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    click(cx, "rail-collections");
    click(cx, &format!("tree-row-{USE}"));
    click(cx, "request-send");
    assert_eq!(settle(cx, shell, USE).as_deref(), Some("200 OK"));
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let (state, workspace) = (shell.state.as_ref()?, shell.workspace_id.as_deref()?);
        match state.history(workspace, 1) {
            Ok(entries) => entries.first().map(|entry| entry.record.sent.url.clone()),
            Err(error) => panic!("history: {error}"),
        }
    })
}

/// Opens the `…` menu of the environment `id`, selecting its row first.
fn open_menu(cx: &mut VisualTestContext, id: &str) {
    click(cx, &format!("variables-scope-environment-{id}"));
    click(cx, &format!("variables-scope-menu-{id}"));
}

#[gpui::test]
fn the_page_lists_globals_collections_and_environments(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    assert!(bounds(cx, "settings-page").is_none());

    click(cx, "rail-environments");
    present(cx, "settings-page");
    assert!(
        bounds(cx, "collection-tree").is_none(),
        "the page replaces the tree"
    );
    let globals = present(cx, "variables-scope-globals");
    let collection = present(cx, &format!("variables-scope-collection-{COLLECTION}"));
    let dev = present(cx, &format!("variables-scope-environment-{DEV}"));
    let staging = present(cx, &format!("variables-scope-environment-{STAGING}"));
    assert!(globals.origin.y < collection.origin.y);
    assert!(dev.origin.y < staging.origin.y, "environments by name");

    click(cx, "variables-scope-globals");
    assert!(bounds(cx, &format!("variables-scope-environment-{DEV}")).is_none());
    present(cx, "variables-document");
    present(cx, "variables-row-1");
    present(cx, "variables-key-1");
}

#[gpui::test]
fn new_environment_creates_a_file_and_opens_rename(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-new-environment");
    assert!(
        dir.path()
            .join("environments/new-environment.env.toml")
            .exists()
    );
    present(cx, "variables-rename-input");

    type_text(cx, "qa");
    enter(cx);
    assert!(bounds(cx, "variables-rename-input").is_none());
    assert!(dir.path().join("environments/qa.env.toml").exists());
    assert!(
        !dir.path()
            .join("environments/new-environment.env.toml")
            .exists()
    );
}

#[gpui::test]
fn renaming_the_active_environment_renames_its_file_and_keeps_it_active(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    select_environment(cx, &shell, "dev");
    click(cx, "rail-environments");
    open_menu(cx, DEV);
    click(cx, "variables-menu-rename");
    type_text(cx, "develop");
    enter(cx);

    assert!(dir.path().join("environments/develop.env.toml").exists());
    assert!(!dir.path().join("environments/dev.env.toml").exists());
    assert!(read(dir.path(), "environments/develop.env.toml").contains(DEV));
    assert_eq!(active(cx, &shell).as_deref(), Some("develop"));
}

#[gpui::test]
fn deleting_the_active_environment_asks_then_resets_the_selector(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    select_environment(cx, &shell, "dev");
    click(cx, "rail-environments");

    open_menu(cx, DEV);
    click(cx, "variables-menu-delete");
    present(cx, "variables-delete-prompt");
    click(cx, "variables-delete-cancel");
    assert!(bounds(cx, "variables-delete-prompt").is_none());
    assert!(dir.path().join("environments/dev.env.toml").exists());

    open_menu(cx, DEV);
    click(cx, "variables-menu-delete");
    click(cx, "variables-delete-confirm");
    assert!(!dir.path().join("environments/dev.env.toml").exists());
    assert!(bounds(cx, &format!("variables-scope-environment-{DEV}")).is_none());
    assert_eq!(active(cx, &shell), None);
}

#[gpui::test]
fn the_tree_history_and_selector_still_work_after_visiting_variables(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = variables_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");

    // The tree comes back and opens and sends a request as before.
    assert_eq!(
        send_use(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/hello"))
    );
    assert!(bounds(cx, "variables-document").is_none());
    present(cx, "request-url");

    click(cx, "rail-history");
    present(cx, "history-row-1");
    assert!(bounds(cx, "settings-page").is_none());

    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    assert_eq!(active(cx, &shell).as_deref(), Some("dev"));
}
