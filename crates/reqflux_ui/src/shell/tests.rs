//! The window shell layout in each workspace state: title bar (S3), activity rail (S4),
//! islands (S5, S6) and status bar (S7).

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, px, size};

use super::Shell;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::demo_workspace;

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    match cx.debug_bounds(selector) {
        Some(bounds) => bounds,
        None => panic!("{selector} is missing from the element tree"),
    }
}

fn render(cx: &mut TestAppContext, workspace: OpenedWorkspace) -> &mut VisualTestContext {
    // The chrome draws with `bolt`, whose components read the installed theme.
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    let (_shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    cx
}

fn assert_layout(cx: &mut VisualTestContext) {
    let window = cx.update(|window, _| window.viewport_size());
    let title_bar = bounds(cx, "title-bar");
    let status_bar = bounds(cx, "status-bar");
    let rail = bounds(cx, "activity-rail");
    let sidebar = bounds(cx, "sidebar");
    let document = bounds(cx, "document");
    for selector in [
        "workspace-name",
        "command-search",
        "command-search-key",
        "approvals",
    ] {
        bounds(cx, selector);
    }

    assert_eq!(title_bar.origin, gpui::point(px(0.0), px(0.0)));
    assert_eq!(title_bar.size, size(window.width, px(44.0)));
    assert_eq!(status_bar.size, size(window.width, px(38.0)));
    assert_eq!(status_bar.origin.y, window.height - px(38.0));

    let context = bounds(cx, "title-context");
    let search = bounds(cx, "command-search");
    let bell = bounds(cx, "approvals");
    assert_eq!(
        context.origin.x,
        px(8.0 + 46.0 + 8.0),
        "over the sidebar column"
    );
    assert_eq!(search.size, size(px(420.0), px(30.0)));
    assert!(context.origin.x + context.size.width <= search.origin.x);
    assert!(search.origin.x + search.size.width <= bell.origin.x);
    let search_center = search.origin.x + search.size.width / 2.0;
    assert!(
        (search_center - window.width / 2.0).abs() <= px(1.0),
        "search centered"
    );

    let middle = window.height - px(44.0) - px(38.0);
    assert_eq!(rail.origin, gpui::point(px(8.0), px(44.0)));
    assert_eq!(rail.size, size(px(46.0), middle));
    let mut tops = Vec::new();
    for selector in super::rail::VIEWS
        .iter()
        .map(|(id, _, _)| *id)
        .chain([super::rail::SETTINGS])
    {
        let button = bounds(cx, selector);
        assert!(rail.contains(&button.origin), "{selector} in the rail");
        tops.push(button.origin.y);
    }
    assert_eq!(tops.len(), 6);
    assert!(tops.windows(2).all(|pair| pair.first() < pair.get(1)));
    let settings = bounds(cx, super::rail::SETTINGS);
    assert!(
        settings.origin.y + settings.size.height > rail.origin.y + middle - px(46.0),
        "Settings at the bottom"
    );

    assert_eq!(sidebar.origin, gpui::point(px(8.0 + 46.0 + 8.0), px(44.0)));
    assert_eq!(sidebar.size.width, px(290.0));
    assert_eq!(sidebar.size.height, middle);

    let document_x = 8.0 + 46.0 + 8.0 + 290.0 + 8.0;
    assert_eq!(document.origin, gpui::point(px(document_x), px(44.0)));
    assert_eq!(document.size.width, window.width - px(document_x + 8.0));
    assert_eq!(document.size.height, sidebar.size.height);

    let tabs = bounds(cx, "document-tabs");
    let path = bounds(cx, "document-path");
    assert_eq!(tabs.origin, document.origin);
    assert_eq!(tabs.size, size(document.size.width, px(46.0)));
    assert_eq!(path.origin.y, tabs.origin.y + px(46.0));
    assert_eq!(path.size.height, px(28.0));
}

#[gpui::test]
fn empty_state_shows_the_open_hint(cx: &mut TestAppContext) {
    let cx = render(cx, OpenedWorkspace::None);
    assert_layout(cx);
    assert!(
        cx.debug_bounds("environment-name").is_none(),
        "no environment without a workspace"
    );
    let message = bounds(cx, "document-message");
    assert!(bounds(cx, "document").contains(&message.origin));
}

#[gpui::test]
fn failed_state_shows_the_error(cx: &mut TestAppContext) {
    let workspace = OpenedWorkspace::Failed {
        path: "/nowhere".into(),
        error: "no reqflux.toml".to_owned(),
    };
    let cx = render(cx, workspace);
    assert_layout(cx);
    bounds(cx, "document-message");
}

#[gpui::test]
fn loaded_state_has_an_empty_document(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    let cx = render(cx, workspace);
    assert_layout(cx);
    bounds(cx, "environment-name");
    assert!(cx.debug_bounds("document-message").is_none());
}

#[gpui::test]
fn the_command_search_shrinks_on_a_narrow_window(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    let cx = render(cx, workspace);

    cx.simulate_resize(size(px(1400.0), px(800.0)));
    cx.run_until_parked();
    assert_eq!(bounds(cx, "command-search").size.width, px(420.0));

    cx.simulate_resize(size(px(640.0), px(800.0)));
    cx.run_until_parked();
    let search = bounds(cx, "command-search");
    let context = bounds(cx, "title-context");
    let bell = bounds(cx, "approvals");
    assert!(search.size.width < px(420.0), "shrinks: {search:?}");
    assert!(
        search.size.width >= px(160.0),
        "keeps its minimum: {search:?}"
    );
    assert!(context.origin.x + context.size.width <= search.origin.x);
    assert!(
        search.origin.x + search.size.width <= bell.origin.x,
        "{context:?} {search:?} {bell:?}"
    );
}

#[test]
fn chrome_labels_follow_the_design() {
    use super::title_bar::*;
    assert_eq!(WORKSPACE_PLACEHOLDER, "No workspace");
    assert_eq!(ENVIRONMENT_PLACEHOLDER, "No environment");
    assert_eq!(COMMAND_SEARCH, "Search or run a command");
    assert_eq!(COMMAND_SEARCH_KEYS, "Ctrl K");
    let labels: Vec<_> = super::rail::VIEWS
        .iter()
        .map(|(_, _, label)| *label)
        .collect();
    assert_eq!(
        labels,
        [
            "Collections",
            "History",
            "Environments",
            "MCP approvals",
            "Audit log"
        ]
    );
}
