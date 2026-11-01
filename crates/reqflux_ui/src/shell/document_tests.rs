//! Opening requests in document tabs (S1), the path bar (S2), the action strip (S3) and their
//! automation ids (S7).

use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, px};

use super::Shell;
use super::sidebar_tests::{API, HEALTH, LIST, TOKEN, USERS, sidebar_workspace};
use crate::workspace::OpenedWorkspace;

fn render(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.update(|cx| crate::theme::install(bolt_legacy::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let dir = sidebar_workspace();
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    // The workspace is loaded into memory; the directory is not read again.
    drop(dir);
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    (shell, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let Some(target) = bounds(cx, selector) else {
        panic!("{selector} is missing");
    };
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

/// The open request ids, in tab order, and the active one.
fn tabs(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> (Vec<String>, Option<String>) {
    cx.update(|_, cx| {
        let documents = &shell.read(cx).documents;
        (
            documents.open_ids().to_vec(),
            documents.active().map(str::to_owned),
        )
    })
}

fn tab(id: &str) -> String {
    format!("doc-tab-{id}")
}

#[gpui::test]
fn clicking_a_request_row_opens_an_active_tab(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    assert!(bounds(cx, &tab(HEALTH)).is_none());

    click(cx, &format!("tree-row-{HEALTH}"));
    assert_eq!(
        tabs(cx, &shell),
        (vec![HEALTH.to_owned()], Some(HEALTH.to_owned()))
    );
    let row = bounds(cx, "document-tabs");
    let Some(health) = bounds(cx, &tab(HEALTH)) else {
        panic!("the Health tab is missing");
    };
    assert!(row.is_some_and(|row| row.contains(&health.origin)));
    assert_eq!(health.size.height, px(30.0));
    assert!(bounds(cx, &format!("doc-tab-close-{HEALTH}")).is_some());
}

#[gpui::test]
fn clicking_a_collection_or_folder_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{API}"));
    click(cx, &format!("tree-row-{USERS}"));
    assert_eq!(tabs(cx, &shell), (Vec::new(), None));
}

#[gpui::test]
fn enter_opens_the_selected_request_and_toggles_folders(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());
    cx.update(|window, cx| sidebar.read(cx).focus_handle(cx).focus(window, cx));

    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    assert!(
        bounds(cx, &format!("tree-row-{LIST}")).is_some(),
        "Enter expands the folder"
    );
    assert_eq!(tabs(cx, &shell).0, Vec::<String>::new());

    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(
        tabs(cx, &shell),
        (vec![LIST.to_owned()], Some(LIST.to_owned()))
    );
    assert!(bounds(cx, &tab(LIST)).is_some());
}

#[gpui::test]
fn opening_an_open_request_focuses_its_tab(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{HEALTH}"));
    click(cx, &format!("tree-row-{TOKEN}"));
    click(cx, &format!("tree-row-{HEALTH}"));
    assert_eq!(
        tabs(cx, &shell),
        (
            vec![HEALTH.to_owned(), TOKEN.to_owned()],
            Some(HEALTH.to_owned())
        )
    );
}

#[gpui::test]
fn clicking_tabs_activates_and_closing_moves_to_the_neighbor(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{HEALTH}"));
    click(cx, &format!("tree-row-{TOKEN}"));

    click(cx, &tab(HEALTH));
    assert_eq!(tabs(cx, &shell).1.as_deref(), Some(HEALTH));

    click(cx, &format!("doc-tab-close-{HEALTH}"));
    assert_eq!(
        tabs(cx, &shell),
        (vec![TOKEN.to_owned()], Some(TOKEN.to_owned()))
    );
    assert!(bounds(cx, &tab(HEALTH)).is_none());

    click(cx, &format!("doc-tab-close-{TOKEN}"));
    assert_eq!(tabs(cx, &shell), (Vec::new(), None));
}

#[gpui::test]
fn the_path_bar_shows_the_active_request(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    assert!(bounds(cx, "document-path").is_some());
    assert!(
        bounds(cx, "path-collection").is_none(),
        "empty without a tab"
    );

    click(cx, &format!("tree-chevron-{USERS}"));
    click(cx, &format!("tree-row-{LIST}"));
    let path = bounds(cx, "document-path");
    let segments: Vec<_> = ["path-collection", "path-folder-0", "path-request"]
        .into_iter()
        .map(|selector| match bounds(cx, selector) {
            Some(segment) => segment,
            None => panic!("{selector} is missing"),
        })
        .collect();
    for segment in &segments {
        assert!(path.is_some_and(|path| path.contains(&segment.origin)));
    }
    assert!(segments.windows(2).all(|pair| match pair {
        [left, right] => left.origin.x < right.origin.x,
        _ => false,
    }));
    assert!(bounds(cx, "path-folder-1").is_none());

    click(cx, &format!("tree-row-{TOKEN}"));
    assert!(
        bounds(cx, "path-folder-0").is_none(),
        "Token is at the top of Auth"
    );
    assert!(bounds(cx, "path-request").is_some());
}

#[gpui::test]
fn the_action_strip_holds_five_actions(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{HEALTH}"));
    let Some(strip) = bounds(cx, "request-actions") else {
        panic!("the action strip is missing");
    };
    assert_eq!(strip.size.width, px(36.0));
    let Some(body) = bounds(cx, "document-body") else {
        panic!("the document body is missing");
    };
    assert_eq!(strip.origin, body.origin + gpui::point(px(0.0), px(1.0)));
    let mut tops = Vec::new();
    for action in ["save", "revert", "undo", "redo", "split"] {
        let Some(button) = bounds(cx, &format!("request-action-{action}")) else {
            panic!("{action} is missing");
        };
        assert!(strip.contains(&button.origin), "{action} in the strip");
        tops.push(button.origin.y);
    }
    assert!(tops.windows(2).all(|pair| pair.first() < pair.get(1)));
}
