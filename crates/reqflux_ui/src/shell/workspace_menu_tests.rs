//! The workspace menu as workspace-menu/01-refined-dropdown draws it with the bolt-v2/02
//! copy (design-catch-up T4): the `Workspaces` header, the actions in order with their keys,
//! the footer note, and a recent workspace that did not load keeping its error and Retry in
//! its row; `Remove` on hover and keyboard focus and `Import Postman collection…`
//! (app-pending S6).

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use super::super::workspace_new_tests::hover;
use super::super::workspace_picker::TestPicker;
use super::super::workspace_switch_tests::{bounds, click, name, pair, present, render};
use super::{FOOTER_NOTE, HEADER, IMPORT_POSTMAN, OPEN_WORKSPACE};

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

fn automation(cx: &mut VisualTestContext) -> gpui_mcp::Automation {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    automation
}

/// Moves `dir` aside so it no longer loads; returns where it went.
fn hide(dir: &Path) -> PathBuf {
    let aside = dir.with_extension("aside");
    if let Err(error) = fs::rename(dir, &aside) {
        panic!("rename {}: {error}", dir.display());
    }
    aside
}

#[gpui::test]
fn the_menu_has_the_workspaces_header_actions_in_order_and_the_note(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    click(cx, "workspace-selector");

    assert_eq!(HEADER, "Workspaces");
    assert_eq!(OPEN_WORKSPACE, "Open workspace…");
    assert_eq!(
        label(&automation, "workspace-menu-header").as_deref(),
        Some(HEADER)
    );
    assert_eq!(
        label(&automation, "workspace-open").as_deref(),
        Some(OPEN_WORKSPACE)
    );
    assert_eq!(
        label(&automation, "workspace-menu-note").as_deref(),
        Some(FOOTER_NOTE)
    );
    let tops: Vec<_> = [
        "workspace-menu-header",
        "workspace-recent-1",
        "workspace-new",
        "workspace-new-folder",
        "workspace-open",
        "workspace-import",
        "workspace-settings",
        "workspace-close",
        "workspace-menu-note",
    ]
    .iter()
    .map(|id| present(cx, id).top())
    .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
    present(cx, "workspace-legend");
    present(cx, "workspace-new-keycap");
    present(cx, "workspace-open-keycap");
    assert_eq!(IMPORT_POSTMAN, "Import Postman collection…");
    assert_eq!(
        label(&automation, "workspace-import").as_deref(),
        Some(IMPORT_POSTMAN)
    );
    present(cx, "workspace-settings-keycap");
}

#[gpui::test]
fn without_a_workspace_the_menu_offers_no_import(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(cx, None, &pair.state_path, TestPicker::default());
    click(cx, "workspace-selector");
    present(cx, "workspace-open");
    assert!(bounds(cx, "workspace-import").is_none());
}

#[gpui::test]
fn remove_shows_only_while_its_row_is_hovered(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "workspace-selector");
    assert!(bounds(cx, "workspace-recent-remove-1").is_none());
    assert!(bounds(cx, "workspace-recent-remove-2").is_none());

    hover(cx, "workspace-recent-2");
    present(cx, "workspace-recent-remove-2");
    assert!(bounds(cx, "workspace-recent-remove-1").is_none());

    hover(cx, "workspace-menu-note");
    assert!(bounds(cx, "workspace-recent-remove-2").is_none());
}

#[gpui::test]
fn remove_shows_while_its_row_holds_the_keyboard_focus(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "workspace-selector");
    hover(cx, "workspace-menu-note");
    assert!(bounds(cx, "workspace-recent-remove-1").is_none());
    // The app binds no Tab key yet; move along the tab stops as a Tab binding would.
    let mut reached = false;
    for _ in 0..80 {
        cx.update(|window, cx| window.focus_next(cx));
        cx.run_until_parked();
        if bounds(cx, "workspace-recent-remove-1").is_some() {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "focus reaches the first recent row and shows its Remove"
    );
    present(cx, "workspace-menu");
}

#[gpui::test]
fn a_recent_that_did_not_load_keeps_its_error_and_retry_in_its_row(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let aside = hide(pair.second.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));

    let row = present(cx, "workspace-recent-2");
    let error = present(cx, "workspace-error");
    assert!(row.contains(&error.center()), "the error sits in its row");
    present(cx, "workspace-recent-failed-2");
    assert!(bounds(cx, "workspace-recent-failed-1").is_none());
    assert!(bounds(cx, "workspace-recent-retry-1").is_none());
    let retry = present(cx, "workspace-recent-retry-2");
    assert!(retry.left() > row.left());
    let away = present(cx, "workspace-menu-note").center();
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.run_until_parked();
    present(cx, "workspace-recent-remove-2");

    if let Err(error) = fs::rename(&aside, pair.second.path()) {
        panic!("rename back: {error}");
    }
    click(cx, "workspace-recent-retry-2");
    assert_eq!(name(cx, &shell).as_deref(), Some("Second"));
    assert!(bounds(cx, "workspace-menu").is_none());
    assert!(
        cx.update(|_, cx| shell.read(cx).failed_recent.is_none()),
        "a workspace that opens clears the failure"
    );
}

#[gpui::test]
fn the_actions_still_open_new_name_field_and_close(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    present(cx, "workspace-new-name");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "workspace-new-name").is_none());
    present(cx, "workspace-menu");

    click(cx, "workspace-close");
    assert_eq!(name(cx, &shell), None);
    present(cx, "empty-open");
}
