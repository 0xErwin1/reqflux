//! The request/response split (request-pane-feedback S6, request-panes/01): the gutter
//! between the panes, dragging it, Up and Down on it, collapsing the response with Enter or a
//! double click, the minimum height of each pane and the position kept in the app state.
//! Every test uses a temporary state file.

use std::path::Path;

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
    VisualTestContext, point, px,
};
use reqflux_engine::state::AppState;

use super::Shell;
use super::draft_tests::{bounds, present, render, state_file};
use super::sidebar_tests::{HEALTH, sidebar_workspace};
use super::split::{COLLAPSED_SETTING, HEIGHT_SETTING};

const SPLIT: &str = "request-response-split";

fn click(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

fn open_health(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{HEALTH}"));
}

fn setting(path: &Path, key: &str) -> Option<String> {
    let state = match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    match state.setting(key) {
        Ok(value) => value,
        Err(error) => panic!("read {key}: {error}"),
    }
}

fn store(path: &Path, key: &str, value: &str) {
    let mut state = match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    if let Err(error) = state.set_setting(key, value) {
        panic!("write {key}: {error}");
    }
}

/// Drags the gutter's center by `dy` and releases it.
fn drag_split(cx: &mut VisualTestContext, dy: gpui::Pixels) {
    let start = present(cx, SPLIT).center();
    let end = point(start.x, start.y + dy);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x, start.y + px(4.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn double_click(cx: &mut VisualTestContext, selector: &str) {
    let position = present(cx, selector).center();
    for click_count in 1..=2 {
        cx.simulate_event(MouseDownEvent {
            position,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
        });
    }
    cx.run_until_parked();
}

fn request_height(cx: &mut VisualTestContext) -> gpui::Pixels {
    present(cx, "request-area").size.height
}

fn collapsed(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).split.collapsed())
}

#[gpui::test]
fn the_gutter_sits_between_the_request_and_the_response(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);

    let request = present(cx, "request-area");
    let gutter = present(cx, SPLIT);
    let response = present(cx, "response");
    assert_eq!(gutter.size.height, px(9.));
    assert_eq!(request.size.height, px(306.));
    assert_eq!(gutter.top(), request.bottom());
    assert_eq!(response.top(), gutter.bottom());
}

#[gpui::test]
fn a_tall_tab_scrolls_inside_the_request_pane_instead_of_running_under_the_response(
    cx: &mut TestAppContext,
) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);
    click(cx, "request-tab-scripts");

    let area = present(cx, "request-area");
    let pane = present(cx, "request-pane");
    assert!(
        pane.bottom() <= area.bottom(),
        "the Scripts pane ends at {:?}, below the request area's {:?}",
        pane.bottom(),
        area.bottom()
    );
}

#[gpui::test]
fn dragging_the_gutter_resizes_the_panes_and_is_kept(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);

    let before = request_height(cx);
    drag_split(cx, px(-100.));
    assert_eq!(request_height(cx), before - px(100.));
    assert_eq!(
        setting(&state_path, HEIGHT_SETTING).as_deref(),
        Some(format!("{}", f32::from(before - px(100.))).as_str())
    );
}

#[gpui::test]
fn each_pane_keeps_its_minimum_height(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);

    drag_split(cx, px(-2000.));
    assert_eq!(request_height(cx), px(120.));

    drag_split(cx, px(4000.));
    let response = present(cx, "response-area");
    assert_eq!(response.size.height, px(120.));
}

#[gpui::test]
fn up_and_down_on_the_focused_gutter_move_it(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);
    click(cx, SPLIT);

    let before = request_height(cx);
    cx.simulate_keystrokes("up up");
    cx.run_until_parked();
    assert_eq!(request_height(cx), before - px(32.));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(request_height(cx), before - px(16.));
    assert_eq!(
        setting(&state_path, HEIGHT_SETTING).as_deref(),
        Some(format!("{}", f32::from(before - px(16.))).as_str())
    );
}

#[gpui::test]
fn enter_and_a_double_click_collapse_and_restore_the_response(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);

    double_click(cx, SPLIT);
    assert!(collapsed(cx, &shell));
    assert!(bounds(cx, "response").is_none());
    let bar = present(cx, "response-bar");
    assert_eq!(bar.size.height, px(40.));
    assert_eq!(present(cx, SPLIT).bottom(), bar.top());
    assert_eq!(
        setting(&state_path, COLLAPSED_SETTING).as_deref(),
        Some("true")
    );

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!collapsed(cx, &shell));
    assert!(bounds(cx, "response").is_some());
    assert!(bounds(cx, "response-bar").is_none());

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(collapsed(cx, &shell));
    click(cx, "response-expand");
    assert!(!collapsed(cx, &shell));
    assert_eq!(
        setting(&state_path, COLLAPSED_SETTING).as_deref(),
        Some("false")
    );
}

#[gpui::test]
fn the_stored_position_is_restored_on_start(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    store(&state_path, HEIGHT_SETTING, "200");
    store(&state_path, COLLAPSED_SETTING, "true");
    let (shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);

    assert!(collapsed(cx, &shell));
    assert!(bounds(cx, "response-bar").is_some());
    click(cx, "response-expand");
    assert_eq!(request_height(cx), px(200.));
}

#[gpui::test]
fn an_unreadable_stored_height_keeps_the_default(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    store(&state_path, HEIGHT_SETTING, "tall");
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);
    assert_eq!(request_height(cx), px(306.));
}

#[gpui::test]
fn the_response_still_shows_under_the_request(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_health(cx);
    let line = present(cx, "request-line");
    let response = present(cx, "response");
    assert!(response.top() > line.bottom());
    assert!(bounds(cx, "response-tab-response").is_some());
}
