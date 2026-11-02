//! The collection's environment allowlist (v1-features S7, design/layouts/01 D7): the
//! Environments block of the collection's settings section, a multi-select over the
//! workspace's environments, empty meaning any, written through the engine.

use gpui::{TestAppContext, VisualTestContext};

use crate::shell::draft_tests::{click, present, render, state_file};
use crate::shell::send_tests::{closed_port, settle};
use crate::shell::variables_tests::{COLLECTION, DEV, STAGING, USE, read, variables_workspace};
use crate::workspace_tests::write;

/// Opens the settings page at the collection's section.
fn open_collection(cx: &mut VisualTestContext) {
    click(cx, "rail-environments");
    click(cx, &format!("variables-scope-collection-{COLLECTION}"));
}

/// The workspace with the collection's environment allowlist naming only `staging`.
fn allowlisted_workspace(port: u16) -> tempfile::TempDir {
    let dir = variables_workspace(port);
    write(
        dir.path(),
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\nenvironments = [\"{STAGING}\"]\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://127.0.0.1:{port}\"\n"
        ),
    );
    dir
}

#[gpui::test]
fn an_empty_allowlist_means_any_and_ticking_environments_writes_it(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_collection(cx);

    present(cx, "collection-allowlist-all");
    present(cx, "collection-allowlist-selected");
    present(cx, &format!("collection-allowlist-environment-{DEV}"));
    present(cx, &format!("collection-allowlist-environment-{STAGING}"));

    click(cx, &format!("collection-allowlist-environment-{DEV}"));
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(
        file.contains(&format!("environments = [\"{DEV}\"]")),
        "the tick wrote the allowlist: {file}"
    );

    click(cx, &format!("collection-allowlist-environment-{STAGING}"));
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(
        file.contains(&format!("environments = [\"{DEV}\", \"{STAGING}\"]")),
        "both environments are listed: {file}"
    );

    click(cx, &format!("collection-allowlist-environment-{DEV}"));
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(
        file.contains(&format!("environments = [\"{STAGING}\"]"))
            && !file.contains(&format!("\"{DEV}\"")),
        "unticking dev leaves staging: {file}"
    );

    click(cx, "collection-allowlist-all");
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(
        !file.contains("environments"),
        "all environments clears the list: {file}"
    );
}

#[gpui::test]
fn the_list_shows_again_after_the_write_and_names_the_ticked_ones(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_collection(cx);

    click(cx, &format!("collection-allowlist-environment-{STAGING}"));
    present(cx, &format!("collection-allowlist-environment-{STAGING}"));
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(
        file.contains(&format!("environments = [\"{STAGING}\"]")),
        "{file}"
    );

    click(cx, "collection-allowlist-all");
    present(cx, &format!("collection-allowlist-environment-{STAGING}"));
    let file = read(dir.path(), "collections/api/collection.toml");
    assert!(!file.contains("environments"), "{file}");
}

#[gpui::test]
fn a_send_with_an_excluded_environment_fails_with_the_engine_refusal(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = allowlisted_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    click(cx, &format!("tree-row-{USE}"));
    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(
        settle(cx, &_shell, USE).as_deref(),
        Some("Failed: environment dev is not allowed for collection API"),
        "the engine refuses before anything goes out"
    );
}

#[gpui::test]
fn adding_the_active_environment_to_the_allowlist_lets_the_send_go_out(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = allowlisted_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_collection(cx);

    click(cx, &format!("collection-allowlist-environment-{DEV}"));
    click(cx, "settings-close");
    click(cx, &format!("tree-row-{USE}"));
    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| shell.read(cx).sends.contains_key(USE)),
        "the send went out once dev is allowed"
    );
}
