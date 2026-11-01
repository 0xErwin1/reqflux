//! The title bar environment button (S1) and its menu (S2).

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::title_bar::environment_label;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000021";
const REQUEST: &str = "01J00000000000000000000022";

/// `Demo` with `GET Ping` at `{{host}}/ping` and the environments `prod` and `dev`, written in
/// that order; only `dev` defines `host`.
fn environment_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/ping.req.toml",
        &format!(
            "id = \"{REQUEST}\"\nname = \"Ping\"\nmethod = \"GET\"\nurl = \"{{{{host}}}}/ping\"\n"
        ),
    );
    write(
        root,
        "environments/prod.env.toml",
        "id = \"01J00000000000000000000023\"\nname = \"prod\"\n",
    );
    write(
        root,
        "environments/dev.env.toml",
        "id = \"01J00000000000000000000024\"\nname = \"dev\"\n\n\
         [[variables]]\nkey = \"host\"\nvalue = \"https://h\"\n",
    );
    dir
}

fn render(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    render_dir(cx, environment_workspace())
}

fn render_dir(cx: &mut TestAppContext, dir: TempDir) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    drop(dir);
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    (shell, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

fn present(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    match bounds(cx, selector) {
        Some(found) => found,
        None => panic!("{selector} is missing"),
    }
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

fn active(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).active_environment.clone())
}

fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<usize> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(REQUEST)
            .map(|state| state.inspection.unresolved.len())
    })
}

#[test]
fn the_label_names_the_environment_or_none() {
    assert_eq!(environment_label(Some("dev")), "Environment: dev");
    assert_eq!(environment_label(None), "Environment: none");
}

#[gpui::test]
fn the_button_shows_the_placeholder_and_no_menu(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    present(cx, "environment-selector");
    present(cx, "environment-name");
    assert!(bounds(cx, "environment-menu").is_none());
}

#[gpui::test]
fn clicking_the_button_lists_none_then_environments_by_name(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, "environment-selector");
    let button = present(cx, "environment-selector");
    let menu = present(cx, "environment-menu");
    assert!(
        menu.top() >= button.bottom(),
        "the menu opens under the button"
    );
    let tops: Vec<_> = [
        "environment-none",
        "environment-option-dev",
        "environment-option-prod",
    ]
    .iter()
    .map(|id| present(cx, id).top())
    .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}

#[gpui::test]
fn choosing_an_environment_closes_the_menu_and_resolves_requests(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{COLLECTION}"));
    click(cx, &format!("tree-row-{REQUEST}"));
    assert_eq!(unresolved(cx, &shell), Some(1));

    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    assert!(bounds(cx, "environment-menu").is_none());
    assert_eq!(active(cx, &shell).as_deref(), Some("dev"));
    assert_eq!(unresolved(cx, &shell), Some(0));

    click(cx, "environment-selector");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "environment-menu").is_none());
    assert_eq!(active(cx, &shell).as_deref(), Some("dev"));

    click(cx, "environment-selector");
    click(cx, "document-content");
    assert!(bounds(cx, "environment-menu").is_none());
    assert_eq!(active(cx, &shell).as_deref(), Some("dev"));

    click(cx, "environment-selector");
    click(cx, "environment-none");
    assert!(bounds(cx, "environment-menu").is_none());
    assert_eq!(active(cx, &shell), None);
    assert_eq!(unresolved(cx, &shell), Some(1));
}

#[gpui::test]
fn an_environment_named_none_keeps_its_own_row(cx: &mut TestAppContext) {
    let dir = environment_workspace();
    write(
        dir.path(),
        "environments/none.env.toml",
        "id = \"01J00000000000000000000025\"\nname = \"zz\"\n",
    );
    let (shell, cx) = render_dir(cx, dir);
    click(cx, "environment-selector");
    let no_environment = present(cx, "environment-none");
    let named_none = present(cx, "environment-option-none");
    assert!(no_environment.origin.y < named_none.origin.y);

    click(cx, "environment-option-none");
    assert_eq!(active(cx, &shell).as_deref(), Some("none"));
}
