//! The quick add of the variable popover (design/variable-popover/01, state d): the "Add to"
//! dropdown over every scope the collection allows, adding to one of them, and the two-level
//! Escape over its list.

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::variable_popover::TARGET_ID;
use super::variable_popover_tests::{
    label_of, open, popover_open, popover_workspace, type_text, unresolved,
};

#[gpui::test]
fn quick_add_creates_the_variable_in_the_chosen_scope(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    assert!(unresolved(cx, &shell).contains(&"tenant".to_owned()));

    click(cx, "request-url-var-3");
    assert!(bounds(cx, "variable-popover-not-defined").is_some());
    // Targets: Staging, Production, API, Globals.
    click(cx, "variable-popover-target");
    pick_target(cx, 4);
    click(cx, "variable-popover-input");
    type_text(cx, "tnt_7f2a");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    let manifest = std::fs::read_to_string(dir.path().join("reqflux.toml")).unwrap_or_default();
    assert!(manifest.contains("key = \"tenant\""), "{manifest}");
    assert!(manifest.contains("value = \"tnt_7f2a\""), "{manifest}");
    assert!(!unresolved(cx, &shell).contains(&"tenant".to_owned()));
}

fn target_index(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<usize> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .variable_popover
            .as_ref()
            .and_then(|popover| popover.target_index(cx))
    })
}

/// Picks row `index` (1-based, as the "Add to" list draws it) of the open list: `bolt`'s
/// dropdown draws row `n` from 0 as `<dropdown id>-item-<n>`, so the row is picked by id and
/// never by where it landed.
fn pick_target(cx: &mut VisualTestContext, index: usize) {
    click(cx, &format!("{TARGET_ID}-item-{}", index - 1));
}

#[gpui::test]
fn the_add_to_dropdown_lists_every_environment_and_preselects_the_active_one(
    cx: &mut TestAppContext,
) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-3");
    assert!(bounds(cx, "variable-popover-target").is_some());
    assert!(
        bounds(cx, "variable-popover-target-menu").is_none(),
        "closed"
    );
    assert_eq!(target_index(cx, &shell), Some(0), "Staging is active");

    click(cx, "variable-popover-target");
    let menu = present(cx, "variable-popover-target-menu");
    assert!(popover_open(cx), "the list opens inside the popover");
    // Four scopes: Staging, Production, the collection and the globals.
    let rem_size = cx.update(|window, _| window.rem_size());
    let rows = 4.0 * bolt::tokens::Fields::MENU_ROW_HEIGHT.to_pixels(rem_size);
    assert!(
        menu.size.height >= rows,
        "every scope has a row: {menu:?} for {rows:?}"
    );

    pick_target(cx, 2);
    assert!(
        bounds(cx, "variable-popover-target-menu").is_none(),
        "picking closes the list"
    );
    assert!(popover_open(cx));
    assert_eq!(target_index(cx, &shell), Some(1));
}

#[gpui::test]
fn quick_add_creates_the_variable_in_an_environment_that_is_not_active(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-3");
    click(cx, "variable-popover-target");
    pick_target(cx, 2);
    click(cx, "variable-popover-input");
    type_text(cx, "tnt_p");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let production = std::fs::read_to_string(dir.path().join("environments/production.env.toml"))
        .unwrap_or_default();
    assert!(production.contains("key = \"tenant\""), "{production}");
    assert!(production.contains("value = \"tnt_p\""), "{production}");
    let staging = std::fs::read_to_string(dir.path().join("environments/staging.env.toml"))
        .unwrap_or_default();
    assert!(!staging.contains("tenant"), "{staging}");
}

#[gpui::test]
fn with_no_active_environment_the_quick_add_defaults_to_the_collection(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    click(cx, "request-url-var-3");

    // Targets: Production, Staging, API, Globals.
    assert_eq!(target_index(cx, &shell), Some(2));
}

#[gpui::test]
fn escape_closes_the_add_to_list_first_and_the_popover_second(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    click(cx, "request-url-var-3");
    click(cx, "variable-popover-target");
    assert!(
        bounds(cx, "variable-popover-target-menu").is_some(),
        "list open"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        bounds(cx, "variable-popover-target-menu").is_none(),
        "list closed"
    );
    assert!(popover_open(cx), "the first Escape keeps the popover");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!popover_open(cx), "the second Escape closes it");
}

#[gpui::test]
fn adding_to_an_inactive_environment_leaves_the_unresolved_count_as_it_was(
    cx: &mut TestAppContext,
) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let before = unresolved(cx, &shell);
    assert_eq!(before, ["token", "tenant"]);

    click(cx, "request-url-var-3");
    // Targets: Production, Staging, API, Globals.
    click(cx, "variable-popover-target");
    pick_target(cx, 1);
    click(cx, "variable-popover-input");
    type_text(cx, "tnt_p");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let production = std::fs::read_to_string(dir.path().join("environments/production.env.toml"))
        .unwrap_or_default();
    assert!(production.contains("key = \"tenant\""), "{production}");
    assert_eq!(unresolved(cx, &shell), before, "no environment is active");
    assert_eq!(
        label_of(cx, "request-unresolved").as_deref(),
        Some("2 unresolved variables")
    );
}
