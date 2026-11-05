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

/// A window narrow enough for none of the tables to fit its columns beside the sidebar.
fn narrow_window(cx: &mut VisualTestContext) {
    cx.simulate_resize(gpui::size(px(700.), px(700.)));
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
