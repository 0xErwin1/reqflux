//! Toolbars and header rows at a narrow window (S23, L71): nothing a row draws goes past
//! its container's edge, so every control stays reachable.

use gpui::{Pixels, VisualTestContext, px};

use gpui::TestAppContext;

use super::draft_tests::{click, present, render, state_file};

/// The window the checks use, 800 by 450 logical pixels.
pub(super) fn narrow_window(cx: &mut VisualTestContext) {
    cx.simulate_resize(gpui::size(px(800.), px(450.)));
    cx.run_until_parked();
}

/// Asserts every element in `ids` is inside `container`, horizontally.
pub(super) fn assert_inside(cx: &mut VisualTestContext, container: &str, ids: &[&str]) {
    let outer = present(cx, container);
    for id in ids {
        let bounds = present(cx, id);
        assert!(
            bounds.left() >= outer.left() - Pixels::from(0.5)
                && bounds.right() <= outer.right() + Pixels::from(0.5),
            "{id} spans {:?} to {:?} in {container} from {:?} to {:?}",
            bounds.left(),
            bounds.right(),
            outer.left(),
            outer.right()
        );
    }
}

/// A shell on the demo workspace at the narrow window, with the rail item `rail` open.
fn open(cx: &mut TestAppContext, rail: &str, test: impl FnOnce(&mut VisualTestContext)) {
    let dir = crate::workspace_tests::demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    narrow_window(cx);
    click(cx, rail);
    test(cx);
}

#[gpui::test]
fn the_network_settings_fit_a_narrow_window(cx: &mut TestAppContext) {
    open(cx, "rail-settings", |cx| {
        click(cx, "nav-network");
        assert_inside(
            cx,
            "settings-page",
            &[
                "settings-heading",
                "settings-description",
                "network-open-config",
                "network-reload",
                "network-proxy-mode-system",
                "network-proxy-mode-none",
                "network-proxy-mode-manual",
            ],
        );
    });
}

#[gpui::test]
fn the_cookie_jar_header_fits_a_narrow_window(cx: &mut TestAppContext) {
    open(cx, "rail-settings", |cx| {
        click(cx, "nav-cookies");
        assert_inside(
            cx,
            "settings-page",
            &[
                "settings-heading",
                "settings-description",
                "cookies-add",
                "cookies-clear",
            ],
        );
    });
}

#[gpui::test]
fn the_approvals_empty_state_fits_a_narrow_window(cx: &mut TestAppContext) {
    open(cx, "rail-approvals", |cx| {
        assert_inside(cx, "approvals-view", &["approvals-empty"]);
    });
}
