//! The layout modes (v1-features S7, design/layouts/01): `layout` in the user's config
//! file, the tab-row and action-strip switchers, the vertical gutter between the request
//! and response columns, the minimum width of each column and the collapsed response
//! across the island. Stacked behavior itself stays `split_tests`'s. Every test uses a
//! temporary state and config file and sends to a port nothing listens on.

use std::path::{Path, PathBuf};

use gpui::{Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext, point, px};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, present, render, state_file};
use super::split::{COLLAPSED_SETTING, HEIGHT_SETTING, WIDTH_SETTING};
use super::split_tests::{
    click, double_click, open_sent, request_height, setting, split_workspace, store,
};
use crate::theme::ConfigReport;

const SPLIT: &str = "request-response-split";

/// A config file with `content`, held alive for the test.
fn config_file(content: &str) -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = dir.path().join("config.toml");
    if let Err(error) = std::fs::write(&path, content) {
        panic!("write config: {error}");
    }
    (dir, path)
}

/// Points the shell's config file at `path`, which reads its `layout` key.
fn use_config(cx: &mut VisualTestContext, shell: &Entity<Shell>, path: &Path) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, _cx| {
            shell.set_config(Some(path.to_path_buf()), ConfigReport::default());
        });
    });
    cx.run_until_parked();
}

/// The text of the config file at `path`.
fn config_text(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read config: {error}"),
    }
}

/// Drags the gutter's center by `dx` and releases it.
fn drag_wide(cx: &mut VisualTestContext, dx: Pixels) {
    let start = present(cx, SPLIT).center();
    let end = point(start.x + dx, start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x + px(4.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// `true` while the gutter stands vertically between the columns.
fn in_columns(cx: &mut VisualTestContext) -> bool {
    let gutter = present(cx, SPLIT);
    gutter.size.width == px(6.) && gutter.size.height > gutter.size.width
}

#[gpui::test]
fn a_config_without_a_layout_key_starts_stacked(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file("theme = \"dark\"\n");
    let (_shell, cx) = render(cx, &dir, &state_path);
    use_config(cx, &_shell, &config_path);
    open_sent(cx);

    assert!(!in_columns(cx), "the default layout is stacked");
    let request = present(cx, "request-area");
    let gutter = present(cx, SPLIT);
    let response = present(cx, "response-area");
    assert_eq!(gutter.size.height, px(6.));
    assert_eq!(gutter.top(), request.bottom());
    assert_eq!(response.top(), gutter.bottom());
    assert_eq!(setting(&state_path, WIDTH_SETTING), None, "nothing stored");
    assert!(
        setting(&state_path, HEIGHT_SETTING).is_none()
            || setting(&state_path, COLLAPSED_SETTING).is_none(),
        "the config read stored nothing by itself"
    );
}

#[gpui::test]
fn the_config_layout_key_opens_side_by_side(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file("layout = \"side-by-side\"\n");
    let (_shell, cx) = render(cx, &dir, &state_path);
    use_config(cx, &_shell, &config_path);
    open_sent(cx);

    assert!(in_columns(cx), "the config file put the window in columns");
    let area = present(cx, "request-response-area");
    let request = present(cx, "request-area");
    let gutter = present(cx, SPLIT);
    let response = present(cx, "response-area");
    assert_eq!(gutter.left(), request.right());
    assert_eq!(response.left(), gutter.right());
    assert_eq!(request.top(), area.top());
    assert_eq!(request.bottom(), area.bottom());
    assert_eq!(response.bottom(), area.bottom());
    let share = f64::from(request.size.width / area.size.width);
    assert!(
        (share - 0.46).abs() < 0.02,
        "the request column takes the artboard's default share, got {share}"
    );
}

#[gpui::test]
fn an_unknown_layout_name_starts_stacked(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file("layout = \"future-mode\"\n");
    let (_shell, cx) = render(cx, &dir, &state_path);
    use_config(cx, &_shell, &config_path);
    open_sent(cx);

    assert!(
        !in_columns(cx),
        "a name this build does not know is stacked"
    );
    assert!(
        config_text(&config_path).contains("layout = \"future-mode\""),
        "the file is read, never rewritten"
    );
}

#[gpui::test]
fn only_the_action_strip_switches_layouts(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);

    assert!(
        bounds(cx, "segmented-layout-stacked").is_none(),
        "the tab row has no layout switcher"
    );
    assert!(bounds(cx, "layout-strip-stacked").is_some());
}

#[gpui::test]
fn the_action_strip_switcher_switches_modes(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);

    assert!(bounds(cx, "layout-strip-stacked").is_some());
    assert!(bounds(cx, "layout-strip-side-by-side").is_some());
    click(cx, "layout-strip-side-by-side");
    assert!(
        in_columns(cx),
        "the action-strip switcher opened the columns"
    );
    click(cx, "layout-strip-stacked");
    assert!(!in_columns(cx));
}

#[gpui::test]
fn the_layout_commands_switch_modes(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);

    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();
    assert!(in_columns(cx), "Ctrl+Alt+2 opens the columns");

    cx.simulate_keystrokes("ctrl-alt-1");
    cx.run_until_parked();
    assert!(!in_columns(cx), "Ctrl+Alt+1 goes back to stacked");
}

#[gpui::test]
fn dragging_the_vertical_gutter_resizes_the_columns_and_is_kept(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);
    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();

    let before = present(cx, "request-area").size.width;
    drag_wide(cx, px(-100.));
    let after = present(cx, "request-area").size.width;
    assert_eq!(after, before - px(100.));
    let response = present(cx, "response-area").size.width;
    assert!(response > px(0.), "the response column took the room");
    assert_eq!(
        setting(&state_path, WIDTH_SETTING).as_deref(),
        Some(format!("{}", f32::from(after)).as_str()),
        "the column position is kept in the app state"
    );
}

#[gpui::test]
fn each_column_keeps_its_minimum_width(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);
    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();

    drag_wide(cx, px(-2000.));
    assert_eq!(present(cx, "request-area").size.width, px(120.));

    drag_wide(cx, px(4000.));
    assert_eq!(present(cx, "response-area").size.width, px(120.));
}

#[gpui::test]
fn left_and_right_on_the_focused_gutter_move_the_columns(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);
    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();
    click(cx, SPLIT);

    let before = present(cx, "request-area").size.width;
    cx.simulate_keystrokes("left left");
    cx.run_until_parked();
    assert_eq!(present(cx, "request-area").size.width, before - px(32.));
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert_eq!(present(cx, "request-area").size.width, before - px(16.));
    assert_eq!(
        setting(&state_path, WIDTH_SETTING).as_deref(),
        Some(format!("{}", f32::from(before - px(16.))).as_str())
    );
}

#[gpui::test]
fn a_double_click_collapse_spans_the_response_across_the_island(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);
    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();

    double_click(cx, SPLIT);
    assert!(bounds(cx, "response").is_none(), "the response is its bar");
    let area = present(cx, "request-response-area");
    let request = present(cx, "request-area");
    let gutter = present(cx, SPLIT);
    let bar = present(cx, "response-bar");
    assert_eq!(request.size.width, area.size.width, "the request fills it");
    assert_eq!(gutter.top(), request.bottom());
    assert_eq!(bar.top(), gutter.bottom());
    assert_eq!(bar.bottom(), area.bottom());
    assert_eq!(
        setting(&state_path, COLLAPSED_SETTING).as_deref(),
        Some("true")
    );

    click(cx, "response-expand");
    assert!(in_columns(cx), "expanding brings the columns back");
    assert_eq!(
        setting(&state_path, COLLAPSED_SETTING).as_deref(),
        Some("false")
    );
}

#[gpui::test]
fn switching_back_to_stacked_restores_the_stored_height(cx: &mut TestAppContext) {
    let dir = split_workspace();
    let (_state_dir, state_path) = state_file();
    store(&state_path, HEIGHT_SETTING, "200");
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_sent(cx);
    cx.simulate_keystrokes("ctrl-alt-2");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-alt-1");
    cx.run_until_parked();

    assert!(!in_columns(cx));
    assert_eq!(
        request_height(cx),
        px(200.),
        "the stacked position survived"
    );
}
