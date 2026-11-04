//! The response rows in a narrow side-by-side pane (S23, L68, L69): every tab and view chip
//! stays reachable down to the splitter's minimum, and none draws past the pane edge.

use gpui::{
    Bounds, Modifiers, MouseButton, Pixels, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, VisualTestContext, point, px,
};

use super::draft_tests::{bounds, click, present};
use super::response_tests::sent;

/// What a chip may lack of the viewport's width and still be in view: the control's own
/// padding around its segments.
const SLACK: Pixels = px(4.);

/// The chips of the views row, in order.
const VIEWS: [&str; 4] = [
    "response-tab-body",
    "response-tab-headers",
    "response-tab-cookies",
    "response-tab-timeline",
];

/// The chips of the tab row of a response without script results.
const TABS: [&str; 1] = ["response-tab-response"];

/// Moves the splitter between the request and the response by `dx`, right when positive.
pub(super) fn drag_split(cx: &mut VisualTestContext, dx: Pixels) {
    let start = present(cx, "request-response-split").center();
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

/// Side by side at 1280 px.
pub(super) fn side_by_side(cx: &mut VisualTestContext) {
    click(cx, "layout-strip-side-by-side");
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    cx.run_until_parked();
}

/// Side by side at 1280 px with the response pane `width` wide.
fn narrow_pane(cx: &mut VisualTestContext, width: Pixels) {
    side_by_side(cx);
    let pane = present(cx, "response-area").size.width;
    drag_split(cx, pane - width);
}

/// The part of the chip `chip` that shows: its bounds clipped to the scroll viewport
/// `strip`, which must itself be inside the response pane.
fn shown(cx: &mut VisualTestContext, strip: &str, chip: &str) -> Bounds<Pixels> {
    let pane = present(cx, "response-area");
    let viewport = present(cx, strip);
    assert!(
        viewport.left() >= pane.left() && viewport.right() <= pane.right(),
        "{strip} spans {:?} to {:?} in a pane from {:?} to {:?}",
        viewport.left(),
        viewport.right(),
        pane.left(),
        pane.right()
    );
    present(cx, chip).intersect(&viewport)
}

/// `true` when the chip `chip` shows in the viewport `strip` as far as it can: whole, or
/// across the viewport's whole width when it is wider than the viewport.
fn in_view(cx: &mut VisualTestContext, strip: &str, chip: &str) -> bool {
    let viewport = present(cx, strip).size.width;
    let whole = present(cx, chip).size.width;
    shown(cx, strip, chip).size.width >= whole.min(viewport) - SLACK
}

/// Rolls the wheel over the strip `strip` by `dy`, the way a mouse wheel does.
fn wheel(cx: &mut VisualTestContext, strip: &str, dy: Pixels) {
    let position = present(cx, strip).center();
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.), dy)),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
}

/// Rolls the wheel over `strip` until the chip `chip` shows whole, a bounded number of times.
fn scroll_to(cx: &mut VisualTestContext, strip: &str, chip: &str) {
    for _ in 0..100 {
        if in_view(cx, strip, chip) {
            return;
        }
        wheel(cx, strip, px(-10.));
    }
    panic!("{chip} never shows in {strip}");
}

fn timeline_is_selected(cx: &mut VisualTestContext) -> bool {
    bounds(cx, "response-body").is_none() && bounds(cx, "response-timeline-summary").is_some()
}

fn assert_every_chip_reachable(cx: &mut VisualTestContext) {
    for (strip, chips) in [
        ("response-views-scroll", &VIEWS[..]),
        ("response-tabs-scroll", &TABS[..]),
    ] {
        for chip in chips {
            let visible = shown(cx, strip, chip);
            let pane = present(cx, "response-area");
            assert!(
                visible.right() <= pane.right() && visible.left() >= pane.left(),
                "{chip} draws past the pane"
            );
            scroll_to(cx, strip, chip);
            let target = shown(cx, strip, chip).center();
            assert!(
                present(cx, strip).contains(&target),
                "{chip} has a part to click"
            );
        }
    }
    click(cx, "response-tab-timeline");
    assert!(timeline_is_selected(cx), "Timeline selected by a click");
    assert!(
        in_view(cx, "response-views-scroll", "response-tab-timeline"),
        "the selected tab is in view"
    );
}

#[gpui::test]
fn at_345_px_every_tab_and_view_is_inside_the_pane_and_reachable(cx: &mut TestAppContext) {
    let cx = sent(cx);
    narrow_pane(cx, px(345.));
    assert_eq!(present(cx, "response-area").size.width, px(345.));
    assert_every_chip_reachable(cx);
}

#[gpui::test]
fn at_the_120_px_minimum_every_tab_and_view_is_inside_the_pane_and_reachable(
    cx: &mut TestAppContext,
) {
    let cx = sent(cx);
    narrow_pane(cx, px(120.));
    assert_eq!(present(cx, "response-area").size.width, px(120.));
    assert_every_chip_reachable(cx);
}

#[gpui::test]
fn narrowing_the_pane_keeps_the_selected_view_in_sight(cx: &mut TestAppContext) {
    let cx = sent(cx);
    click(cx, "response-tab-timeline");
    narrow_pane(cx, px(120.));
    assert!(timeline_is_selected(cx));
    assert!(
        in_view(cx, "response-views-scroll", "response-tab-timeline"),
        "the selected view scrolled into the narrowed row"
    );
}

#[gpui::test]
fn a_pane_that_fits_the_chips_does_not_scroll_them(cx: &mut TestAppContext) {
    let cx = sent(cx);
    narrow_pane(cx, px(600.));
    assert_eq!(present(cx, "response-area").size.width, px(600.));
    for chip in VIEWS {
        assert!(
            in_view(cx, "response-views-scroll", chip),
            "{chip} shows at the default width"
        );
    }
}

#[gpui::test]
fn the_cookies_table_scrolls_sideways_inside_a_narrow_pane(cx: &mut TestAppContext) {
    let (_dir, _state_dir, cx) = super::response_tests::cookie_sent(cx);
    click(cx, "response-tab-cookies");
    narrow_pane(cx, px(120.));
    let pane = present(cx, "response-area");
    let viewport = present(cx, "response-cookies-scroll");
    assert!(
        viewport.left() >= pane.left() && viewport.right() <= pane.right(),
        "the table's viewport spans {:?} to {:?} in a pane from {:?} to {:?}",
        viewport.left(),
        viewport.right(),
        pane.left(),
        pane.right()
    );
    let flags = present(cx, "response-cookies-header-flags");
    assert!(
        flags.right() > viewport.right(),
        "the last column starts cut off"
    );
    for _ in 0..200 {
        if present(cx, "response-cookies-header-flags").right() <= viewport.right() + px(0.5) {
            break;
        }
        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(-40.), px(0.))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        cx.run_until_parked();
    }
    let flags = present(cx, "response-cookies-header-flags");
    assert!(
        flags.right() <= viewport.right() + px(0.5),
        "the flags column scrolled into view"
    );
    let name = present(cx, "response-cookies-header-name");
    let value = present(cx, "response-cookies-header-value");
    assert!(
        name.right() <= value.left() + px(0.5),
        "the cells do not overlap"
    );
}

#[gpui::test]
fn at_the_120_px_minimum_the_footer_switch_is_inside_the_pane_and_reachable(
    cx: &mut TestAppContext,
) {
    let cx = sent(cx);
    narrow_pane(cx, px(120.));
    let pane = present(cx, "response-area");
    let viewport = present(cx, "response-format-scroll");
    assert!(
        viewport.left() >= pane.left() && viewport.right() <= pane.right(),
        "the footer switch spans {:?} to {:?} in a pane from {:?} to {:?}",
        viewport.left(),
        viewport.right(),
        pane.left(),
        pane.right()
    );
    for chip in ["response-view-pretty", "response-view-raw"] {
        scroll_to(cx, "response-format-scroll", chip);
        let target = shown(cx, "response-format-scroll", chip).center();
        assert!(
            present(cx, "response-format-scroll").contains(&target),
            "{chip} has a part to click"
        );
    }
    click(cx, "response-view-raw");
    assert!(
        in_view(cx, "response-format-scroll", "response-view-raw"),
        "the selected format is in view"
    );
}

/// Asserts the chip `chip` is revealed exactly in the viewport `strip`, to a pixel: whole
/// when it fits, else aligned to the viewport's start.
fn assert_revealed(cx: &mut VisualTestContext, strip: &str, chip: &str) {
    let viewport = present(cx, strip);
    let chip_bounds = present(cx, chip);
    let fits = chip_bounds.size.width <= viewport.size.width;
    let revealed = if fits {
        chip_bounds.left() >= viewport.left() - px(1.)
            && chip_bounds.right() <= viewport.right() + px(1.)
    } else {
        (chip_bounds.left() - viewport.left()).abs() <= px(1.)
    };
    assert!(
        revealed,
        "{chip} spans {:?} to {:?} in {strip} from {:?} to {:?}",
        chip_bounds.left(),
        chip_bounds.right(),
        viewport.left(),
        viewport.right()
    );
}

#[gpui::test]
fn at_120_px_a_middle_view_is_revealed_exactly(cx: &mut TestAppContext) {
    for view in ["response-tab-headers", "response-tab-cookies"] {
        let cx = sent(cx);
        click(cx, view);
        narrow_pane(cx, px(120.));
        assert_revealed(cx, "response-views-scroll", view);
    }
}
