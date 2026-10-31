//! The workspace settings page (design-catch-up T7, variables/01-settings-page): the rail's
//! Environments entry, the workspace menu's `Workspace settings…` and the palette open it in
//! place of the sidebar and the document; it lists Globals, the environments and the
//! collections, and declares, discards and resets current values.

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::SecretScope;
use reqflux_engine::state::AppState;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::closed_port;
use super::variables_tests::{COLLECTION, DEV, STAGING, read, variables_workspace};

const WORKSPACE_ID: &str = "01J00000000000000000000900";

fn automation(cx: &mut VisualTestContext) -> gpui_mcp::Automation {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    automation
}

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

/// Stores `value` as the current value of `key` in `scope` before the shell opens.
fn seed_current(state_path: &std::path::Path, scope: SecretScope, key: &str, value: &str) {
    let mut state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    if let Err(error) = state.set_current_value(WORKSPACE_ID, scope, key, value) {
        panic!("set current value: {error}");
    }
}

fn stored_current(state_path: &std::path::Path, scope: SecretScope, key: &str) -> Option<String> {
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    match state.current_values(WORKSPACE_ID) {
        Ok(values) => values.get(scope, key).map(str::to_owned),
        Err(error) => panic!("current values: {error}"),
    }
}

fn panel_is_page(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| {
        shell.read(cx).sidebar.read(cx).panel == super::sidebar_history::Panel::Environments
    })
}

#[gpui::test]
fn the_rail_opens_the_page_with_its_sections_and_the_first_environment(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    assert!(bounds(cx, "settings-page").is_none());

    click(cx, "rail-environments");
    present(cx, "settings-page");
    assert!(
        bounds(cx, "sidebar").is_none(),
        "the page replaces the sidebar"
    );
    assert!(bounds(cx, "document").is_none(), "and the document");
    present(cx, "activity-rail");
    assert_eq!(
        label(&automation, "settings-title").as_deref(),
        Some("Workspace settings")
    );

    let globals = present(cx, "variables-scope-globals");
    let environments = present(cx, "settings-nav-environments");
    let collection = present(cx, &format!("variables-scope-collection-{COLLECTION}"));
    assert!(globals.origin.y < environments.origin.y);
    assert!(environments.origin.y < collection.origin.y);
    assert_eq!(
        label(&automation, "settings-nav-environments").as_deref(),
        Some("Environments 2")
    );
    assert_eq!(
        label(&automation, "settings-heading").as_deref(),
        Some("Environments")
    );

    let dev = present(cx, &format!("variables-scope-environment-{DEV}"));
    let staging = present(cx, &format!("variables-scope-environment-{STAGING}"));
    assert!(dev.origin.y < staging.origin.y, "environments by name");
    assert_eq!(
        label(&automation, &format!("variables-environment-meta-{DEV}")).as_deref(),
        Some("1 variable · dev.env.toml")
    );
    assert_eq!(
        label(&automation, "variables-title").as_deref(),
        Some("VARIABLES IN DEV"),
        "the first environment is selected"
    );
    present(cx, "variables-row-1");

    click(cx, "settings-close");
    assert!(bounds(cx, "settings-page").is_none());
    present(cx, "collection-tree");
    assert!(!panel_is_page(cx, &shell));
}

#[gpui::test]
fn the_workspace_menu_and_the_palette_open_the_page(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    click(cx, "workspace-selector");
    present(cx, "workspace-settings");
    click(cx, "workspace-settings");
    assert!(bounds(cx, "workspace-menu").is_none());
    present(cx, "settings-page");
    assert_eq!(
        label(&automation, "settings-heading").as_deref(),
        Some("Globals")
    );

    click(cx, "rail-collections");
    assert!(bounds(cx, "settings-page").is_none());
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.run_until_parked();
    cx.simulate_input(">workspace settings");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    present(cx, "settings-page");
    assert!(panel_is_page(cx, &shell));
}

#[gpui::test]
fn declare_keeps_the_key_in_the_file_and_the_current_value_local(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let staging = match STAGING.parse() {
        Ok(id) => SecretScope::Environment(id),
        Err(error) => panic!("id: {error}"),
    };
    seed_current(&state_path, staging, "customer_id", "cus_9s6");
    let (_shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    click(cx, "rail-environments");
    click(cx, &format!("variables-scope-environment-{STAGING}"));
    present(cx, "variables-undeclared-1");
    assert_eq!(
        label(&automation, "variables-declare-note").as_deref(),
        Some(
            "Declare keeps the key in staging.env.toml with an empty initial value; the \
             current value stays local."
        )
    );

    click(cx, "variables-declare-1");
    let file = read(dir.path(), "environments/staging.env.toml");
    assert!(file.contains("key = \"customer_id\""), "{file}");
    assert!(
        !file.contains("cus_9s6"),
        "the current value never reaches the file"
    );
    assert!(
        !file.contains("value"),
        "declared with no initial value: {file}"
    );
    assert_eq!(
        stored_current(&state_path, staging, "customer_id").as_deref(),
        Some("cus_9s6")
    );
    assert!(bounds(cx, "variables-undeclared-1").is_none());
    present(cx, "variables-row-1");
    assert_eq!(
        label(&automation, "variables-current-1").as_deref(),
        Some("cus_9s6")
    );
}

#[gpui::test]
fn discard_and_reset_clear_current_values_and_leave_the_file(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    seed_current(
        &state_path,
        SecretScope::Global,
        "script_token",
        "from-script",
    );
    seed_current(&state_path, SecretScope::Global, "greeting", "mine");
    let before = read(dir.path(), "reqflux.toml");
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");

    click(cx, "variables-discard-1");
    assert!(bounds(cx, "variables-undeclared-1").is_none());
    assert_eq!(
        stored_current(&state_path, SecretScope::Global, "script_token"),
        None
    );

    click(cx, "variables-reset-current");
    assert_eq!(
        stored_current(&state_path, SecretScope::Global, "greeting"),
        None
    );
    assert!(bounds(cx, "variables-reset-current").is_none());
    assert_eq!(read(dir.path(), "reqflux.toml"), before);
}

#[gpui::test]
fn a_secret_never_shows_and_the_keyring_note_names_its_entry(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    assert_eq!(
        label(&automation, "variables-stored-1").as_deref(),
        Some("file")
    );

    click(cx, "variables-secret-1");
    assert_eq!(
        label(&automation, "variables-stored-1").as_deref(),
        Some("KEYRING")
    );
    assert_eq!(
        label(&automation, "variables-value-1").as_deref(),
        Some("not in the file")
    );
    present(cx, "variables-masked-1");
    assert_eq!(
        label(&automation, "variables-keyring-note").as_deref(),
        Some(&*format!(
            "Secrets never reach the file. Keyring entry for greeting: reqflux · \
             {WORKSPACE_ID}/global/greeting"
        ))
    );
    let tree = automation.snapshot();
    for (id, node) in &tree.nodes {
        assert!(
            !node.label.as_deref().unwrap_or_default().contains("hello"),
            "{id} shows the secret"
        );
    }
}

#[gpui::test]
fn the_empty_count_names_enabled_variables_with_no_value(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    assert!(bounds(cx, "variables-empty").is_none());
    click(cx, "variables-add");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        label(&automation, "variables-empty").as_deref(),
        Some("1 empty variable")
    );
}

#[gpui::test]
fn escape_closes_the_menu_then_the_page(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, &format!("variables-scope-menu-{DEV}"));
    present(cx, "variables-menu");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "variables-menu").is_none());
    present(cx, "settings-page");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "settings-page").is_none());
    assert!(!panel_is_page(cx, &shell));
}

#[gpui::test]
fn the_new_environment_key_opens_the_page_into_rename(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    cx.simulate_keystrokes("ctrl-shift-e");
    cx.run_until_parked();
    present(cx, "settings-page");
    present(cx, "variables-rename-input");
    assert_eq!(
        label(&automation, "variables-rename-hint").as_deref(),
        Some("Enter renames New environment and its file · Esc cancels")
    );
    assert!(
        dir.path()
            .join("environments/new-environment.env.toml")
            .exists()
    );
}

#[gpui::test]
fn a_click_outside_commits_a_new_environment_name(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-new-environment");
    present(cx, "variables-rename-input");

    super::variables_tests::type_text(cx, "qa");
    click(cx, "settings-heading");

    assert!(bounds(cx, "variables-rename-input").is_none());
    assert!(dir.path().join("environments/qa.env.toml").exists());
    assert!(
        !dir.path()
            .join("environments/new-environment.env.toml")
            .exists()
    );
}

#[gpui::test]
fn the_environment_button_opens_its_menu_at_the_button(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    let button = present(cx, &format!("variables-scope-menu-{DEV}"));
    click(cx, &format!("variables-scope-menu-{DEV}"));
    assert_eq!(present(cx, "variables-menu").origin, button.center());
}
