//! Copying variables into an environment from the settings page (feedback-nov3 S13):
//! selection boxes on the Globals and collection grids, the `Copy to environment…` dialog,
//! the policy for keys the environment already has, and creating an environment when the
//! workspace has none.

use std::fs;

use gpui::{TestAppContext, VisualTestContext};

use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::variables_tests::{
    COLLECTION, DEV, read, select_environment, variables_workspace,
};
use crate::workspace_tests::write;

const DEV_FILE: &str = "environments/dev.env.toml";
const STAGING_FILE: &str = "environments/staging.env.toml";

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

fn open_globals(cx: &mut VisualTestContext) {
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
}

fn open_collection(cx: &mut VisualTestContext) {
    click(cx, "rail-environments");
    click(cx, &format!("variables-scope-collection-{COLLECTION}"));
}

/// Gives `dev` a `greeting` of its own, so copying the global one meets a key that exists.
fn dev_has_greeting(dir: &tempfile::TempDir) {
    write(
        dir.path(),
        DEV_FILE,
        &format!(
            "id = \"{DEV}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"t\"\n\n\
             [[variables]]\nkey = \"greeting\"\nvalue = \"mine\"\n"
        ),
    );
}

#[gpui::test]
fn selected_globals_copy_into_the_first_environment(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    open_globals(cx);

    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    present(cx, "variables-copy-dialog");
    click(cx, "variables-copy-confirm");

    assert!(
        bounds(cx, "variables-copy-dialog").is_none(),
        "the dialog closes"
    );
    let dev = read(dir.path(), DEV_FILE);
    assert!(
        dev.contains("key = \"greeting\"\nvalue = \"hello\""),
        "{dev}"
    );
    assert_eq!(
        label(&automation, "variables-warning").as_deref(),
        Some("Copied 1 variable to dev.")
    );
    assert!(
        !read(dir.path(), STAGING_FILE).contains("greeting"),
        "only the chosen environment changes"
    );
}

#[gpui::test]
fn the_dialog_defaults_to_the_active_environment(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    select_environment(cx, &shell, "staging");
    open_collection(cx);

    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    click(cx, "variables-copy-confirm");

    assert!(read(dir.path(), STAGING_FILE).contains("key = \"base\""));
    assert!(!read(dir.path(), DEV_FILE).contains("base"));
}

#[gpui::test]
fn a_key_the_environment_has_is_skipped_unless_overwrite_is_ticked(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    dev_has_greeting(&dir);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    let automation = automation(cx);
    open_globals(cx);

    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    click(cx, "variables-copy-confirm");
    assert!(read(dir.path(), DEV_FILE).contains("value = \"mine\""));
    assert_eq!(
        label(&automation, "variables-warning").as_deref(),
        Some("Copied 0 variables to dev; skipped 1 already there: greeting.")
    );

    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    click(cx, "variables-copy-overwrite");
    click(cx, "variables-copy-confirm");
    let dev = read(dir.path(), DEV_FILE);
    assert!(dev.contains("value = \"hello\""), "{dev}");
    assert!(!dev.contains("mine"), "{dev}");
}

#[gpui::test]
fn the_header_box_selects_every_row_and_cancel_changes_nothing(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    let before = read(dir.path(), DEV_FILE);
    open_globals(cx);
    click(cx, "variables-add");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    click(cx, "variables-select-all");
    click(cx, "variables-copy");
    present(cx, "variables-copy-dialog");
    click(cx, "variables-copy-cancel");
    assert!(bounds(cx, "variables-copy-dialog").is_none());
    assert_eq!(read(dir.path(), DEV_FILE), before);

    click(cx, "variables-copy");
    click(cx, "variables-copy-confirm");
    let dev = read(dir.path(), DEV_FILE);
    assert!(dev.contains("key = \"greeting\""), "{dev}");
    assert!(dev.contains("key = \"new_variable\""), "{dev}");
}

#[gpui::test]
fn an_environment_can_be_created_from_the_dialog_when_there_is_none(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    for file in [DEV_FILE, STAGING_FILE] {
        if let Err(error) = fs::remove_file(dir.path().join(file)) {
            panic!("remove {file}: {error}");
        }
    }
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);

    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    present(cx, "variables-copy-dialog");
    present(cx, "variables-copy-new-env");
    click(cx, "variables-copy-confirm");
    present(cx, "variables-copy-dialog");
    assert!(
        !dir.path()
            .join("environments/new-environment.env.toml")
            .exists(),
        "no environment to copy into"
    );

    click(cx, "variables-copy-new-env");
    click(cx, "variables-copy-confirm");
    let created = read(dir.path(), "environments/new-environment.env.toml");
    assert!(
        created.contains("key = \"greeting\"\nvalue = \"hello\""),
        "{created}"
    );
}

#[gpui::test]
fn nothing_opens_without_a_selection(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    click(cx, "variables-copy");
    assert!(bounds(cx, "variables-copy-dialog").is_none());
}

#[gpui::test]
fn the_environment_menu_is_as_wide_as_its_trigger(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    click(cx, "variables-select-1");
    click(cx, "variables-copy");
    click(cx, "variables-copy-env");

    let trigger = present(cx, "variables-copy-env");
    let menu = present(cx, "variables-copy-env-menu");
    assert!(
        menu.size.width >= trigger.size.width,
        "menu {:?} is narrower than its trigger {:?}",
        menu.size.width,
        trigger.size.width
    );
}
