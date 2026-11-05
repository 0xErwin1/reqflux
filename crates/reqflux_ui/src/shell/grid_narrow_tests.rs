//! The request grids in a narrow request pane (L69): the cells never overlap, the value keeps
//! a minimum width, and the grid scrolls sideways when the pane is too narrow for the columns.

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, px};

use super::draft_tests::{click, present};
use super::grid_resize_tests::{drag_edge, width};
use super::response_narrow_tests::{drag_split, side_by_side};
use super::response_tests::sent;
use crate::tokens;

/// The strip `bolt`'s sideways viewport reserves under the rows while the columns overflow.
const STRIP: Pixels = bolt::tokens::GridMetrics::SCROLLBAR_WIDTH;

/// The cells of the Headers grid's header row and of its first row, left to right.
const CELLS: [&str; 6] = [
    "request-headers-header-key",
    "request-headers-header-value",
    "request-headers-header-description",
    "request-headers-key-1",
    "request-headers-value-1",
    "request-headers-description-1",
];

/// Side by side at 1280 px with a Headers row, the request pane `width` wide.
fn headers_in_pane(cx: &mut VisualTestContext, width: Pixels) {
    click(cx, "request-tab-headers");
    click(cx, "request-headers-add");
    side_by_side(cx);
    let pane = present(cx, "request-area").size.width;
    drag_split(cx, width - pane);
}

/// `true` when the two bounds share no horizontal span.
fn disjoint(left: Bounds<Pixels>, right: Bounds<Pixels>) -> bool {
    left.right() <= right.left() + px(0.5) || right.right() <= left.left() + px(0.5)
}

fn assert_cells_apart(cx: &mut VisualTestContext) {
    for (header, row) in [(0, 3), (1, 4), (2, 5)] {
        let (header, row) = (present(cx, CELLS[header]), present(cx, CELLS[row]));
        assert_eq!(
            header.left(),
            row.left(),
            "a column lines up with its header"
        );
    }
    for cells in [&CELLS[..3], &CELLS[3..]] {
        for pair in cells.windows(2) {
            let (left, right) = (present(cx, pair[0]), present(cx, pair[1]));
            assert!(
                left.right() <= right.left() + px(0.5),
                "{} {:?}..{:?} overlaps {} {:?}..{:?}",
                pair[0],
                left.left(),
                left.right(),
                pair[1],
                right.left(),
                right.right()
            );
        }
        assert!(disjoint(present(cx, cells[0]), present(cx, cells[2])));
    }
    let value = present(cx, "request-headers-value-1");
    assert!(
        value.size.width >= tokens::GRID_VALUE_MIN_COLUMN,
        "the value column kept {:?}",
        value.size.width
    );
}

#[gpui::test]
fn a_request_pane_narrower_than_the_columns_keeps_the_cells_apart(cx: &mut TestAppContext) {
    let cx = sent(cx);
    headers_in_pane(cx, px(405.));
    assert_eq!(present(cx, "request-area").size.width, px(405.));
    assert_cells_apart(cx);
}

#[gpui::test]
fn the_narrowest_request_pane_keeps_the_cells_apart(cx: &mut TestAppContext) {
    let cx = sent(cx);
    headers_in_pane(cx, px(120.));
    assert_eq!(present(cx, "request-area").size.width, px(120.));
    assert_cells_apart(cx);
}

#[gpui::test]
fn a_wide_request_pane_has_no_sideways_scroll(cx: &mut TestAppContext) {
    let cx = sent(cx);
    headers_in_pane(cx, px(800.));
    let grid = present(cx, "request-headers-scroll");
    let header = present(cx, "request-headers-header");
    assert_eq!(
        header.size.width, grid.size.width,
        "the grid fits its viewport"
    );
    assert_cells_apart(cx);
}

#[gpui::test]
fn a_narrow_grid_scrolls_sideways_to_its_last_column(cx: &mut TestAppContext) {
    let cx = sent(cx);
    headers_in_pane(cx, px(405.));
    let pane = present(cx, "request-area");
    let viewport = present(cx, "request-headers-scroll");
    assert!(
        viewport.left() >= pane.left() && viewport.right() <= pane.right(),
        "the grid's viewport stays inside the pane"
    );
    // The rows keep their height and end above the strip the bar draws in, instead of
    // collapsing under a viewport with no height of its own.
    let row = present(cx, "request-headers-key-1");
    assert!(row.size.height > px(0.), "the row laid out: {row:?}");
    assert!(
        present(cx, "request-headers-add").bottom() <= viewport.bottom() - STRIP + px(0.5),
        "the rows end above the strip: {viewport:?}"
    );
    let description = present(cx, "request-headers-description-1");
    assert!(description.right() > viewport.right(), "it starts cut off");

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-400.), px(0.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let description = present(cx, "request-headers-description-1");
    assert!(
        description.right() <= viewport.right() + px(0.5),
        "the last column scrolled into view: {:?} in {:?}",
        description.right(),
        viewport.right()
    );
}

/// A resize still works while the grid is scrolled sideways: the handle moves with the header
/// and the drag resizes by pointer delta, so the scroll offset never enters the width.
#[gpui::test]
fn a_column_resizes_while_the_grid_is_scrolled_sideways(cx: &mut TestAppContext) {
    let cx = sent(cx);
    headers_in_pane(cx, px(405.));
    let viewport = present(cx, "request-headers-scroll");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-400.), px(0.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let before = width(cx, "request-headers-description-1");
    let scrolled = present(cx, "request-headers-description-1");
    assert!(
        scrolled.right() <= viewport.right() + px(0.5),
        "the column is in view: {scrolled:?} in {viewport:?}"
    );

    drag_edge(cx, "request-headers-header-description", px(30.));
    assert_eq!(
        width(cx, "request-headers-description-1"),
        before + px(30.),
        "the drag changed the column by its own delta"
    );
    assert_eq!(
        present(cx, "request-headers-header-description").left(),
        present(cx, "request-headers-description-1").left(),
        "the column stays under its header"
    );
}
