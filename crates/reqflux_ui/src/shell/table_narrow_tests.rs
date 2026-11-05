//! The resizable tables in a narrow page (L69): the column that fills the rest keeps its
//! minimum width and the table scrolls sideways, instead of the column collapsing under the
//! cells beside it.

use gpui::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px};

use super::draft_tests::{click, present, render, state_file};
use super::history_tests::{PING, history_workspace, send};
use super::send_tests::{Server, closed_port, serve};
use super::table_resize_tests::add_cookie;
use super::variables_tests::variables_workspace;
use crate::shell::Shell;
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The strip `bolt`'s sideways viewport reserves under the rows while the columns overflow,
/// where its bar draws.
const STRIP: Pixels = bolt::tokens::GridMetrics::SCROLLBAR_WIDTH;

/// A window narrow enough for none of the tables to fit its columns beside the sidebar.
fn narrow_window(cx: &mut VisualTestContext) {
    cx.simulate_resize(gpui::size(px(700.), px(700.)));
    settle(cx);
}

/// Draws a frame, the way the app's own window does: a content-height grid reads the width it
/// was given from the frame just drawn, so its strip settles one frame after the page's width
/// first reaches it.
pub(super) fn settle(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// Asserts the cell `fill` kept `minimum` and the viewport `scroll` lies inside its parent.
pub(super) fn assert_kept(cx: &mut VisualTestContext, scroll: &str, fill: &str, minimum: Pixels) {
    let viewport: Bounds<Pixels> = present(cx, scroll);
    let window = cx.update(|window, _| window.viewport_size());
    assert!(
        viewport.right() <= window.width,
        "{scroll} spans to {:?} in a {:?} window",
        viewport.right(),
        window.width
    );
    let width = present(cx, fill).size.width;
    assert!(width >= minimum, "{fill} kept {width:?} of {minimum:?}");
}

#[gpui::test]
fn the_variables_description_keeps_its_minimum_in_a_narrow_page(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    narrow_window(cx);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    assert_kept(
        cx,
        "variables-scroll",
        "variables-description-1",
        tokens::VARIABLES_DESCRIPTION_MIN_COLUMN,
    );
    // A table that takes its own height keeps it in the sideways viewport.
    let viewport = present(cx, "variables-scroll");
    assert!(
        viewport.size.height > present(cx, "variables-description-1").size.height,
        "the viewport holds its rows: {:?}",
        viewport.size.height
    );
}

/// A narrow page still lays out the rows of the grids that take their content's height, ends
/// them above the strip the bar draws in, and scrolls them sideways to the last column.
#[gpui::test]
fn a_narrow_variables_page_draws_its_rows_above_the_strip_and_scrolls_sideways(
    cx: &mut TestAppContext,
) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    narrow_window(cx);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    settle(cx);

    let viewport = present(cx, "variables-scroll");
    let row = present(cx, "variables-description-1");
    assert!(row.size.height > px(0.), "the row laid out: {row:?}");
    let last = present(cx, "variables-add");
    assert!(
        last.top() >= viewport.top() && last.bottom() <= viewport.bottom() - STRIP + px(0.5),
        "the rows end above the strip: {last:?} in {viewport:?}"
    );
    assert!(
        row.right() > viewport.right(),
        "the last column starts cut off"
    );

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-600.), px(0.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    settle(cx);
    let row = present(cx, "variables-description-1");
    assert!(
        row.right() <= viewport.right() + px(0.5),
        "the last column scrolled into view: {:?} in {:?}",
        row.right(),
        viewport.right()
    );
}

/// A page wide enough for the variables columns reserves no strip and keeps the grid's full
/// height, as a grid that fits draws without the sideways viewport at all.
#[gpui::test]
fn a_wide_variables_page_keeps_the_full_height_of_its_rows(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    settle(cx);

    let viewport = present(cx, "variables-scroll");
    let add = present(cx, "variables-add");
    assert!(
        (viewport.bottom() - add.bottom()).abs() <= px(0.5),
        "the grid keeps its full height: {add:?} in {viewport:?}"
    );
}

#[gpui::test]
fn the_history_value_keeps_its_minimum_in_a_narrow_page(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    narrow_window(cx);
    click(cx, "rail-history");
    click(cx, "history-row-1");
    click(cx, "history-tab-headers");
    assert_kept(
        cx,
        "history-grid-scroll",
        "history-grid-value-1",
        tokens::GRID_VALUE_MIN_COLUMN,
    );
    settle(cx);
    let viewport = present(cx, "history-grid-scroll");
    let row = present(cx, "history-grid-key-1");
    assert!(row.size.height > px(0.), "the row laid out: {row:?}");
    assert!(
        row.bottom() <= viewport.bottom() - STRIP + px(0.5),
        "the rows end above the strip: {row:?} in {viewport:?}"
    );
}

#[gpui::test]
fn the_cookie_jar_flags_keep_their_minimum_in_a_narrow_page(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    narrow_window(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_cookie_jar(window, cx)));
    cx.run_until_parked();
    add_cookie(cx, &shell);
    assert_kept(
        cx,
        "cookies-scroll",
        "cookies-header-flags",
        tokens::COOKIE_FLAGS_MIN_COLUMN,
    );
}

/// Adds `count` cookies to the jar, named `cookie01` to `cookieNN`, in that order.
fn add_cookies(cx: &mut VisualTestContext, shell: &Entity<Shell>, count: usize) {
    cx.update(|_window, cx| {
        shell.update(cx, |shell, cx| {
            let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
                panic!("no workspace");
            };
            for n in 1..=count {
                let header = format!("cookie{n:02}=value{n:02}; Path=/; Secure; HttpOnly");
                if let Err(error) = engine.add_cookie("https://api.example.com/", &header) {
                    panic!("add cookie: {error}");
                }
            }
            cx.notify();
        });
    });
    cx.run_until_parked();
}

/// The jar's rows are a scroller inside the table, not a table taller than the pane: the table
/// fills the height it is given, so a long list stays inside it while its rows scroll.
#[gpui::test]
fn a_long_cookie_jar_keeps_its_rows_inside_the_table(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    narrow_window(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_cookie_jar(window, cx)));
    cx.run_until_parked();
    add_cookies(cx, &shell, 40);
    let viewport = present(cx, "cookies-scroll");
    let rows = present(cx, "cookies-rows");
    let window_height = cx.update(|window, _| window.viewport_size().height);
    assert!(
        rows.bottom() <= viewport.bottom() + px(0.5),
        "the rows end inside the viewport: {rows:?} in {viewport:?}"
    );
    assert!(
        viewport.bottom() <= window_height,
        "the table stays inside the window: {viewport:?} in {window_height:?}"
    );
    assert!(
        present(cx, "cookie-40-domain").top() >= rows.bottom(),
        "the list is longer than the rows it shows, so they scroll"
    );
}
