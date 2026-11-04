//! Column resize of the request grids: dragging a header cell's right edge changes only that
//! column's width, the width never drops under bolt's minimum, and the widths belong to the
//! grid kind, so every request shares them.

use gpui::{Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext, point, px};

use super::draft_tests::{bounds, click, present, render, state_file};
use super::grid_edit_tests::{grid_workspace, open_grid};

/// The width of the element `selector` names.
fn width(cx: &mut VisualTestContext, selector: &str) -> Pixels {
    present(cx, selector).size.width
}

/// Presses the right edge of the header cell `selector`, moves by `dx` and releases.
fn drag_edge(cx: &mut VisualTestContext, selector: &str, dx: Pixels) {
    let cell = present(cx, selector);
    let start = point(cell.right() - px(2.), cell.center().y);
    let end = point(start.x + dx, start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x + px(1.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn dragging_a_header_edge_resizes_only_that_column(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    let key = width(cx, "request-headers-key-1");
    let description = width(cx, "request-headers-description-1");
    let enable = width(cx, "request-headers-header-enable");

    drag_edge(cx, "request-headers-header-key", px(40.));
    assert_eq!(width(cx, "request-headers-key-1"), key + px(40.));
    assert_eq!(
        width(cx, "request-headers-key-2"),
        key + px(40.),
        "every row follows"
    );
    assert_eq!(width(cx, "request-headers-description-1"), description);
    assert_eq!(width(cx, "request-headers-header-enable"), enable);

    drag_edge(cx, "request-headers-header-description", px(-30.));
    assert_eq!(
        width(cx, "request-headers-description-1"),
        description - px(30.)
    );
    assert_eq!(
        width(cx, "request-headers-key-1"),
        key + px(40.),
        "the key stays"
    );
}

#[gpui::test]
fn a_column_never_gets_narrower_than_the_minimum(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);

    drag_edge(cx, "request-headers-header-key", px(-200.));
    assert_eq!(
        width(cx, "request-headers-key-1"),
        px(bolt::components::grid::MIN_COLUMN_WIDTH)
    );
}

#[gpui::test]
fn widths_are_shared_per_grid_kind(cx: &mut TestAppContext) {
    let dir = grid_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_grid(cx);
    let params = {
        click(cx, "request-tab-params");
        width(cx, "request-params-key-1")
    };
    click(cx, "request-tab-headers");

    drag_edge(cx, "request-headers-header-key", px(40.));
    click(cx, "request-tab-params");
    assert_eq!(
        width(cx, "request-params-key-1"),
        params,
        "Params keeps its own width"
    );
    click(cx, "request-tab-headers");
    assert_eq!(
        width(cx, "request-headers-key-1"),
        params + px(40.),
        "Headers keeps the dragged width after a tab switch"
    );
    assert!(bounds(cx, "request-headers-header-key").is_some());
}
