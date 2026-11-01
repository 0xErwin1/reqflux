//! The sidebar header, filter, tree and footer (S5), the warnings chip (S7) and keyboard
//! navigation (S8).

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, px};
use tempfile::TempDir;

use super::Shell;
use super::sidebar::Sidebar;
use super::sidebar::footer_text;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

pub(super) const API: &str = "01J00000000000000000000001";
pub(super) const USERS: &str = "01J00000000000000000000002";
pub(super) const LIST: &str = "01J00000000000000000000003";
pub(super) const HEALTH: &str = "01J00000000000000000000004";
pub(super) const AUTH: &str = "01J00000000000000000000005";
pub(super) const TOKEN: &str = "01J00000000000000000000006";
const DEV: &str = "01J00000000000000000000007";
const PROD: &str = "01J00000000000000000000008";

/// `Demo` with collection `API` (folder `users` holding `GET List users`, then
/// `GET Health`), collection `Auth` (`POST Token`), environments `dev` and `prod`, and a
/// directory without `folder.toml`, which loads with one warning.
pub(super) fn sidebar_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    let request = |id: &str, name: &str, method: &str| {
        format!(
            "id = \"{id}\"\nname = \"{name}\"\nmethod = \"{method}\"\nurl = \"https://example.test\"\n"
        )
    };
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{API}\"\nname = \"API\"\norder = [\"users\", \"health\"]\n"),
    );
    write(
        root,
        "collections/api/users/folder.toml",
        &format!("id = \"{USERS}\"\nname = \"users\"\n"),
    );
    write(
        root,
        "collections/api/users/list.req.toml",
        &request(LIST, "List users", "GET"),
    );
    write(
        root,
        "collections/api/health.req.toml",
        &request(HEALTH, "Health", "GET"),
    );
    write(
        root,
        "collections/api/broken/stray.req.toml",
        &request("01J00000000000000000000009", "Stray", "GET"),
    );
    write(
        root,
        "collections/auth/collection.toml",
        &format!("id = \"{AUTH}\"\nname = \"Auth\"\n"),
    );
    write(
        root,
        "collections/auth/token.req.toml",
        &request(TOKEN, "Token", "POST"),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!("id = \"{DEV}\"\nname = \"dev\"\n"),
    );
    write(
        root,
        "environments/prod.env.toml",
        &format!("id = \"{PROD}\"\nname = \"prod\"\n"),
    );
    dir
}

fn render<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
) -> (Entity<Sidebar>, &'a mut VisualTestContext) {
    cx.update(|cx| crate::theme::install(bolt_legacy::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());
    (sidebar, cx)
}

fn row(id: &str) -> String {
    format!("tree-row-{id}")
}

fn present(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        .is_some()
}

fn selected(cx: &mut VisualTestContext, sidebar: &Entity<Sidebar>) -> Option<String> {
    cx.update(|_, cx| sidebar.read(cx).selected().map(str::to_owned))
}

fn bounds_of(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    match cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str())) {
        Some(bounds) => bounds,
        None => panic!("{selector} is missing"),
    }
}

#[gpui::test]
fn sidebar_shows_header_filter_rows_and_footer(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_sidebar, cx) = render(cx, &dir);

    for selector in [
        "collections-label",
        "sidebar-new-collection",
        "tree-filter",
        "tree-filter-keycap",
        "sidebar-footer",
        "status-problems",
    ] {
        assert!(present(cx, selector), "{selector} is missing");
    }
    for selector in [
        "environments-label",
        &format!("env-row-{DEV}"),
        &format!("env-row-{PROD}"),
    ] {
        assert!(!present(cx, selector), "{selector} should be gone");
    }
    for id in [API, USERS, HEALTH, AUTH, TOKEN] {
        assert!(present(cx, &row(id)), "row {id} is missing");
    }
    assert!(!present(cx, &row(LIST)), "folders start collapsed");

    let sidebar = bounds_of(cx, "sidebar");
    let header = bounds_of(cx, "collections-header");
    assert_eq!(header.size.height, px(46.0));
    let filter = bounds_of(cx, "tree-filter");
    assert_eq!(filter.size.height, px(30.0));
    assert!(filter.origin.y >= header.origin.y + header.size.height);
    let footer = bounds_of(cx, "sidebar-footer");
    assert_eq!(footer.size.height, px(40.0));
    assert!(sidebar.contains(&footer.origin));

    for (id, icon) in [(API, "box"), (USERS, "folder"), (AUTH, "box")] {
        assert!(
            present(cx, &format!("tree-icon-{id}")),
            "{icon} icon of {id}"
        );
        assert!(present(cx, &format!("tree-count-{id}")), "count of {id}");
    }
    for id in [HEALTH, TOKEN] {
        assert!(present(cx, &format!("tree-method-{id}")), "method of {id}");
        assert!(
            !present(cx, &format!("tree-count-{id}")),
            "no count on {id}"
        );
    }
    assert!(
        present(cx, &format!("tree-guide-{USERS}-0")),
        "one guide at depth 1"
    );
    assert!(!present(cx, &format!("tree-guide-{USERS}-1")));
    assert!(
        !present(cx, &format!("tree-guide-{API}-0")),
        "no guide at depth 0"
    );

    let order: Vec<_> = [API, USERS, HEALTH, AUTH, TOKEN]
        .iter()
        .map(|id| bounds_of(cx, &row(id)))
        .collect();
    for pair in order.windows(2) {
        if let [above, below] = pair {
            assert_eq!(below.origin.y, above.origin.y + px(26.0), "store order");
            assert_eq!(above.size.height, px(26.0));
        }
    }
}

#[test]
fn footer_counts_requests_and_collections() {
    assert_eq!(footer_text(3, 2), "3 requests · 2 collections");
    assert_eq!(footer_text(1, 1), "1 request · 1 collection");
    assert_eq!(footer_text(0, 0), "0 requests · 0 collections");
}

#[test]
fn the_sidebar_workspace_loads_with_one_warning() {
    let dir = sidebar_workspace();
    let opened = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    assert_eq!(opened.warning_count(), 1);
}

#[gpui::test]
fn status_bar_has_no_problems_without_load_warnings(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_sidebar, cx) = render(cx, &dir);
    assert!(!present(cx, "status-problems"));
    assert!(present(cx, "collections-label"));
    assert!(present(cx, "status-bar"));
}

#[gpui::test]
fn keys_move_the_selection_when_the_tree_has_focus(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (sidebar, cx) = render(cx, &dir);
    cx.update(|window, cx| sidebar.read(cx).focus_handle(cx).focus(window, cx));

    cx.simulate_keystrokes("down");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(API));
    cx.simulate_keystrokes("down right");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(USERS));
    cx.run_until_parked();
    assert!(present(cx, &row(LIST)), "Right expands the folder");
    cx.simulate_keystrokes("right");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(LIST));
    cx.simulate_keystrokes("left");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(USERS));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!present(cx, &row(LIST)), "Enter collapses the folder");
    cx.simulate_keystrokes("up up");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(API));
}

#[gpui::test]
fn clicking_a_row_selects_it_and_focuses_the_tree(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (sidebar, cx) = render(cx, &dir);
    let Some(token) = cx.debug_bounds(Box::leak(row(TOKEN).into_boxed_str())) else {
        panic!("token row is missing");
    };
    cx.simulate_click(token.center(), gpui::Modifiers::none());
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(TOKEN));
    cx.simulate_keystrokes("up");
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(AUTH));

    let Some(api) = cx.debug_bounds(Box::leak(row(API).into_boxed_str())) else {
        panic!("API row is missing");
    };
    cx.simulate_click(api.center(), gpui::Modifiers::none());
    assert_eq!(selected(cx, &sidebar).as_deref(), Some(API));
    assert!(present(cx, &row(USERS)), "a row click does not collapse");
}
