//! `New workspace…` by name in its popover (standalone-workspaces S3, app-pending S6) and
//! managed recent rows (S4).

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::Shell;
use super::workspace_new::{IN_REQFLUX, POPOVER_HINT, POPOVER_TITLE};
use super::workspace_picker::TestPicker;
use super::workspace_switch_tests::{
    bounds, canonical, click, name, pair, present, recents, render, temp_dir,
};

/// The shell on `First` with the managed workspaces under `managed`.
fn with_managed(cx: &mut VisualTestContext, shell: &Entity<Shell>, managed: &Path) {
    let managed = managed.to_path_buf();
    shell.update(cx, |shell, _| shell.managed_dir = Some(managed));
}

/// The directories under `managed`.
fn managed_dirs(managed: &Path) -> Vec<PathBuf> {
    match fs::read_dir(managed) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect(),
        Err(error) => panic!("read_dir {}: {error}", managed.display()),
    }
}

fn type_name(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn field_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).new_name_text(cx))
}

/// Moves the mouse over `selector`.
pub(super) fn hover(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector).center();
    cx.simulate_mouse_move(target, None, Modifiers::none());
    cx.run_until_parked();
}

/// Clicks near the bottom of the window, outside the menu and the popover.
fn click_outside(cx: &mut VisualTestContext) {
    let viewport = cx.update(|window, _| window.viewport_size());
    let outside = gpui::point(viewport.width / 2., viewport.height - gpui::px(4.));
    cx.simulate_click(outside, Modifiers::none());
    cx.run_until_parked();
}

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

#[gpui::test]
fn new_workspace_by_name_creates_a_managed_one_and_remove_keeps_it(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    present(cx, "workspace-new-name");
    assert_eq!(field_text(cx, &shell).as_deref(), Some(""));
    type_name(cx, "Billing");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let dirs = managed_dirs(managed.path());
    let [dir] = dirs.as_slice() else {
        panic!("one managed workspace expected: {dirs:?}");
    };
    assert!(dir.join("reqflux.toml").is_file());
    let id = cx.update(|_, cx| shell.read(cx).workspace_id.clone());
    assert_eq!(dir.file_name().and_then(|n| n.to_str()), id.as_deref());
    assert_eq!(name(cx, &shell).as_deref(), Some("Billing"));
    assert!(bounds(cx, "workspace-menu").is_none());
    assert_eq!(
        recents(&pair.state_path).first(),
        Some(&(canonical(dir), "Billing".to_owned()))
    );

    click(cx, "workspace-selector");
    hover(cx, "workspace-recent-1");
    let details: Vec<String> = cx.update(|_, cx| {
        let shell = shell.read(cx);
        shell
            .recents
            .iter()
            .map(|recent| shell.recent_detail(recent))
            .collect()
    });
    assert_eq!(details.first().map(String::as_str), Some(IN_REQFLUX));
    let first_path = canonical(pair.first.path()).display().to_string();
    assert_eq!(details.get(1), Some(&first_path));

    click(cx, "workspace-recent-remove-1");
    assert!(dir.join("reqflux.toml").is_file(), "Remove deletes nothing");
    assert_eq!(recents(&pair.state_path).len(), 2);
}

#[gpui::test]
fn escape_closes_the_name_field_and_creates_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    type_name(cx, "Draft");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "workspace-new-name").is_none());
    present(cx, "workspace-menu");
    assert!(managed_dirs(managed.path()).is_empty());
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "workspace-menu").is_none());
}

#[gpui::test]
fn an_empty_name_is_refused_with_the_menu_error(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    type_name(cx, "   ");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    present(cx, "workspace-error");
    present(cx, "workspace-new-name");
    assert!(managed_dirs(managed.path()).is_empty());
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
}

#[gpui::test]
fn the_empty_state_offers_new_by_name(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(cx, None, &pair.state_path, TestPicker::default());
    with_managed(cx, &shell, managed.path());
    for id in ["empty-new", "empty-new-folder", "empty-open"] {
        present(cx, id);
    }
    // The empty state has room for Remove, so it shows without a hover.
    present(cx, "empty-recent-remove-1");
    click(cx, "empty-new");
    present(cx, "workspace-new-name");
    type_name(cx, "Personal");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(name(cx, &shell).as_deref(), Some("Personal"));
    assert_eq!(managed_dirs(managed.path()).len(), 1);
}

#[gpui::test]
fn the_popover_sits_beside_the_menu_with_its_title_hint_and_buttons(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    click(cx, "workspace-selector");
    click(cx, "workspace-new");

    let menu = present(cx, "workspace-menu");
    let popover = present(cx, "workspace-new-popover");
    assert!(
        popover.left() >= menu.right(),
        "{popover:?} beside {menu:?}"
    );
    for id in [
        "workspace-new-name",
        "workspace-new-cancel",
        "workspace-new-create",
    ] {
        assert!(
            popover.contains(&present(cx, id).center()),
            "{id} in the popover"
        );
    }
    assert_eq!(
        label(&automation, "workspace-new-title").as_deref(),
        Some(POPOVER_TITLE)
    );
    assert_eq!(
        label(&automation, "workspace-new-hint").as_deref(),
        Some(POPOVER_HINT)
    );
    let cancel = present(cx, "workspace-new-cancel");
    let create = present(cx, "workspace-new-create");
    assert!(cancel.right() <= create.left(), "Create is last");
}

#[gpui::test]
fn create_makes_the_workspace_as_enter_does(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    type_name(cx, "Billing");
    click(cx, "workspace-new-create");

    assert_eq!(managed_dirs(managed.path()).len(), 1);
    assert_eq!(name(cx, &shell).as_deref(), Some("Billing"));
    assert!(bounds(cx, "workspace-new-popover").is_none());
}

#[gpui::test]
fn cancel_closes_the_popover_and_creates_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    type_name(cx, "Billing");
    click(cx, "workspace-new-cancel");

    assert!(managed_dirs(managed.path()).is_empty());
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    assert!(bounds(cx, "workspace-new-popover").is_none());
    assert_eq!(field_text(cx, &shell), None);
    present(cx, "workspace-menu");
}

#[gpui::test]
fn a_click_outside_the_popover_cancels_it(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    type_name(cx, "Billing");
    click_outside(cx);

    assert!(managed_dirs(managed.path()).is_empty());
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    assert!(bounds(cx, "workspace-new-popover").is_none());
    assert_eq!(field_text(cx, &shell), None);
}

#[gpui::test]
fn a_click_outside_an_empty_name_field_creates_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let managed = temp_dir();
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    with_managed(cx, &shell, managed.path());
    click(cx, "workspace-selector");
    click(cx, "workspace-new");
    click_outside(cx);

    assert!(managed_dirs(managed.path()).is_empty());
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    assert!(bounds(cx, "workspace-new-name").is_none());
    assert_eq!(field_text(cx, &shell), None);
}
