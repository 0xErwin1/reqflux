//! Toasts (app-pending S9, design/toasts/01): the shell mounts `bolt`'s stack with ReqFlux's
//! icons, Copy body and a save that cannot write push theirs, an error keeps its details
//! behind the toggle, and only an advanced clock closes a success toast, so a test that never
//! advances it sees every toast it pushed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use bolt::icon::IconSource;
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_kit_assets::IconName;

use super::super::Shell;
use super::super::draft_tests::{self, PLAIN, edit_url, state_file};
use super::super::send_tests::{PING, Server, click, closed_port, render, serve, settle};
use super::{Kind, Pushed, PushedToast, counted};
use crate::theme::ConfigReport;

/// The automation tree of the window, read after a fresh frame.
fn tree(cx: &mut VisualTestContext) -> gpui_mcp::UiTree {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation.snapshot()
}

/// The ids of the toast cards on screen, `toast-<n>`, oldest first.
pub(crate) fn shown_toasts(cx: &mut VisualTestContext) -> Vec<String> {
    tree(cx)
        .nodes
        .into_keys()
        .filter(|id| {
            id.strip_prefix("toast-")
                .is_some_and(|n| n.parse::<u64>().is_ok())
        })
        .collect()
}

/// The accessible label of `id` in the automation tree.
pub(crate) fn tree_label(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    tree(cx).nodes.get(id).and_then(|node| node.label.clone())
}

/// Clicks the center of `id`, found in the automation tree: `bolt`'s toast controls carry no
/// debug selector.
fn click_node(cx: &mut VisualTestContext, id: &str) {
    let Some(rect) = tree(cx).nodes.get(id).and_then(|node| node.bounds) else {
        panic!("{id} is missing");
    };
    let center = gpui::point(
        gpui::px(rect.x + rect.width / 2.0),
        gpui::px(rect.y + rect.height / 2.0),
    );
    cx.simulate_click(center, gpui::Modifiers::none());
    cx.run_until_parked();
}

/// The toasts pushed in this window so far, oldest first.
pub(crate) fn pushed(cx: &mut VisualTestContext) -> Vec<PushedToast> {
    cx.update(|_, cx| {
        cx.try_global::<Pushed>()
            .map(|pushed| pushed.0.clone())
            .unwrap_or_default()
    })
}

pub(crate) fn success(title: &str, text: &str) -> PushedToast {
    PushedToast {
        kind: Kind::Success,
        title: title.to_owned(),
        text: text.to_owned(),
    }
}

pub(crate) fn error(title: &str, text: &str) -> PushedToast {
    PushedToast {
        kind: Kind::Error,
        title: title.to_owned(),
        text: text.to_owned(),
    }
}

fn sent(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    (shell, cx)
}

#[gpui::test]
fn the_toast_icons_are_reqflux_lucide_icons(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::theme::install(crate::theme::PaletteMode::Dark, cx);
        let icons = bolt::toast::ToastIcons::current(cx);
        assert_eq!(icons.success, IconSource::Svg(IconName::CircleCheck.path()));
        assert_eq!(icons.error, IconSource::Svg(IconName::CircleAlert.path()));
        assert_eq!(icons.close, IconSource::Svg(IconName::CircleX.path()));
    });
}

#[gpui::test]
fn copy_body_shows_copied(cx: &mut TestAppContext) {
    let (_shell, cx) = sent(cx);
    assert!(shown_toasts(cx).is_empty(), "no toast before Copy");

    click(cx, "response-copy");
    assert_eq!(pushed(cx), vec![success("Copied", "Response body")]);
    assert_eq!(shown_toasts(cx), vec!["toast-1".to_owned()]);
    assert!(
        tree_label(cx, "toast-toggle-1").is_none(),
        "a success toast has no details"
    );
}

#[gpui::test]
fn copy_without_a_text_body_says_so(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, closed_port());
    shell.update(cx, |shell, cx| shell.copy_response_body(PING, cx));
    cx.run_until_parked();
    assert_eq!(
        pushed(cx),
        vec![error(
            "Cannot copy the response body",
            "The request has no text response to copy."
        )]
    );
    assert_eq!(shown_toasts(cx), vec!["toast-1".to_owned()]);
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard()), None);
}

#[gpui::test]
fn an_error_keeps_its_details_behind_the_toggle(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx, closed_port());
    cx.update(|_, cx| super::failure("Cannot open config.toml", "no handler", cx));
    cx.run_until_parked();
    assert_eq!(
        tree_label(cx, "toast-toggle-1").as_deref(),
        Some("Show details")
    );

    click_node(cx, "toast-toggle-1");
    assert_eq!(
        tree_label(cx, "toast-toggle-1").as_deref(),
        Some("Hide details")
    );
}

#[gpui::test]
fn a_success_closes_only_when_the_clock_moves_and_an_error_stays(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx, closed_port());
    cx.update(|_, cx| super::success("Copied", "Response body", cx));
    cx.update(|_, cx| super::failure("Cannot save Ping", "denied", cx));
    cx.run_until_parked();
    assert_eq!(shown_toasts(cx).len(), 2, "parking never closes a toast");

    cx.executor().advance_clock(Duration::from_secs(9));
    cx.run_until_parked();
    assert_eq!(shown_toasts(cx), vec!["toast-2".to_owned()]);
}

fn set_mode(path: &Path, mode: u32) {
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
        panic!("chmod {}: {error}", path.display());
    }
}

#[gpui::test]
fn a_save_that_cannot_write_shows_an_error(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_tests::draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = draft_tests::render(cx, &dir, &state_path);
    draft_tests::click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, &format!("http://127.0.0.1:{port}/edited"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let collection = dir.path().join("collections/api");
    set_mode(&collection, 0o555);
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    set_mode(&collection, 0o755);

    let toasts = pushed(cx);
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    let Some(toast) = toasts.first() else {
        panic!("no toast");
    };
    assert_eq!(toast.kind, Kind::Error);
    assert_eq!(toast.title, "Cannot save Plain");
    assert!(toast.text.contains("collections/api"), "{}", toast.text);
    assert_eq!(shown_toasts(cx), vec!["toast-1".to_owned()]);
}

#[gpui::test]
fn toggle_theme_that_cannot_save_shows_an_error(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, closed_port());
    // A directory where the config file should be: the write fails.
    let config = tempfile::TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let path = config.path().to_owned();
    shell.update(cx, |shell, _| {
        shell.set_config(Some(path), ConfigReport::default());
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.toggle_theme(window, cx)));
    cx.run_until_parked();

    let toasts = pushed(cx);
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(
        toasts
            .first()
            .map(|toast| (toast.kind, toast.title.as_str())),
        Some((Kind::Error, "Cannot save the theme"))
    );
    assert_eq!(shown_toasts(cx), vec!["toast-1".to_owned()]);
}

#[test]
fn counts_are_singular_for_one() {
    assert_eq!(counted(1, "request"), "1 request");
    assert_eq!(counted(12, "request"), "12 requests");
    assert_eq!(counted(0, "folder"), "0 folders");
}
