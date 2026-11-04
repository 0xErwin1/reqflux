//! Column resize in the tables other than the request grids: dragging a header cell's right
//! edge changes only that column's width, every row follows, and the width belongs to the
//! table. The request grids have their own tests in `grid_resize_tests`.

use gpui::{Entity, TestAppContext, VisualTestContext, px};

use super::Shell;
use super::draft_tests::{click, present, render, state_file};
use super::grid_resize_tests::{drag_edge, width};
use super::history_tests::{PING, history_workspace, send};
use super::runner::tests::{COLLECTION, open_run, settle_run, workspace};
use super::send_tests::{Server, closed_port, serve};
use super::variables_tests::variables_workspace;
use crate::workspace::OpenedWorkspace;

/// The x of the left edge of the element `selector` names.
fn left(cx: &mut VisualTestContext, selector: &str) -> gpui::Pixels {
    present(cx, selector).origin.x
}

#[gpui::test]
fn dragging_a_variables_header_edge_resizes_only_that_column(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    let key = width(cx, "variables-key-1");
    let value = width(cx, "variables-value-1");
    let current = width(cx, "variables-current-1");
    let stored = width(cx, "variables-stored-1");

    drag_edge(cx, "variables-header-key", px(40.));
    assert_eq!(
        width(cx, "variables-key-1"),
        key + px(40.),
        "the row follows"
    );
    assert_eq!(width(cx, "variables-header-key"), key + px(40.));
    assert_eq!(width(cx, "variables-value-1"), value);
    assert_eq!(width(cx, "variables-current-1"), current);
    assert_eq!(width(cx, "variables-stored-1"), stored);

    drag_edge(cx, "variables-header-current", px(-30.));
    assert_eq!(width(cx, "variables-current-1"), current - px(30.));
    assert_eq!(width(cx, "variables-key-1"), key + px(40.), "the key stays");

    drag_edge(cx, "variables-header-stored", px(25.));
    assert_eq!(width(cx, "variables-stored-1"), stored + px(25.));
    assert_eq!(width(cx, "variables-value-1"), value);
}

#[gpui::test]
fn a_variables_column_never_gets_narrower_than_the_minimum(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");

    drag_edge(cx, "variables-header-value", px(-400.));
    assert_eq!(
        width(cx, "variables-value-1"),
        px(bolt::components::grid::MIN_COLUMN_WIDTH)
    );
}

#[gpui::test]
fn the_variables_widths_survive_a_scope_switch(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
    let key = width(cx, "variables-key-1");
    drag_edge(cx, "variables-header-key", px(40.));

    click(
        cx,
        &format!(
            "variables-scope-collection-{}",
            super::variables_tests::COLLECTION
        ),
    );
    assert_eq!(
        width(cx, "variables-key-1"),
        key + px(40.),
        "the table keeps its widths across scopes"
    );
}

fn add_cookie(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|_window, cx| {
        shell.update(cx, |shell, cx| {
            let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
                panic!("no workspace");
            };
            if let Err(error) = engine.add_cookie(
                "https://api.example.com/",
                "session=abc123; Path=/; Secure; HttpOnly",
            ) {
                panic!("add cookie: {error}");
            }
            cx.notify();
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn dragging_a_cookie_jar_header_edge_resizes_only_that_column(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_cookie_jar(window, cx)));
    cx.run_until_parked();
    add_cookie(cx, &shell);
    let domain = width(cx, "cookies-header-domain");
    let path = width(cx, "cookies-header-path");
    let path_left = left(cx, "cookie-1-path");
    let name_left = left(cx, "cookie-1-name");
    let domain_left = left(cx, "cookie-1-domain");

    drag_edge(cx, "cookies-header-domain", px(40.));
    assert_eq!(width(cx, "cookies-header-domain"), domain + px(40.));
    assert_eq!(width(cx, "cookies-header-path"), path);
    assert_eq!(
        left(cx, "cookie-1-path"),
        path_left + px(40.),
        "rows follow"
    );
    assert_eq!(left(cx, "cookie-1-name"), name_left + px(40.));
    assert_eq!(left(cx, "cookie-1-domain"), domain_left);
}

#[gpui::test]
fn dragging_the_history_key_edge_resizes_the_recorded_grid(cx: &mut TestAppContext) {
    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "rail-history");
    click(cx, "history-row-1");
    click(cx, "history-tab-headers");
    let key = width(cx, "history-grid-key-1");
    let value_right = present(cx, "history-grid-value-1").right();

    drag_edge(cx, "history-grid-header-key", px(40.));
    assert_eq!(width(cx, "history-grid-key-1"), key + px(40.));
    assert_eq!(width(cx, "history-grid-header-key"), key + px(40.));
    assert_eq!(
        present(cx, "history-grid-value-1").right(),
        value_right,
        "the value column keeps filling the rest"
    );

    click(cx, "history-tab-body");
    click(cx, "history-tab-headers");
    assert_eq!(
        width(cx, "history-grid-header-key"),
        key + px(40.),
        "the width outlives the view"
    );
}

#[gpui::test]
fn dragging_a_runner_header_edge_resizes_only_that_column(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, "");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_run(cx, &shell);
    let cell = |column: usize| format!("runner-grid-header-cell-{column}");
    let request = width(cx, &cell(1));
    let folder = width(cx, &cell(2));
    let method = width(cx, &cell(0));
    let folder_left = left(cx, &cell(2));

    drag_edge(cx, &cell(1), px(40.));
    assert_eq!(width(cx, &cell(1)), request + px(40.));
    assert_eq!(width(cx, &cell(0)), method);
    assert_eq!(width(cx, &cell(2)), folder);
    assert_eq!(left(cx, &cell(2)), folder_left + px(40.), "the rest shifts");
}
