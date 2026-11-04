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

/// Asserts no text in `ids` is hard-cut at the narrow window: compared with a wide window, each
/// one is either as wide as it was or wrapped onto more lines (taller). A text that is
/// narrower and no taller is clipped, which is neither wrapping nor an ellipsis. Ids the page
/// does not draw are skipped.
pub(super) fn assert_not_cut(cx: &mut VisualTestContext, ids: &[&str]) {
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    cx.run_until_parked();
    let wide: Vec<_> = ids
        .iter()
        .map(|id| (id, super::draft_tests::bounds(cx, id)))
        .collect();
    narrow_window(cx);
    for (id, wide) in wide {
        let (Some(wide), Some(narrow)) = (wide, super::draft_tests::bounds(cx, id)) else {
            continue;
        };
        let half = Pixels::from(0.5);
        assert!(
            narrow.size.width >= wide.size.width - half
                || narrow.size.height > wide.size.height + half,
            "{id} is hard-cut: {:?} by {:?} narrow against {:?} by {:?} wide",
            narrow.size.width,
            narrow.size.height,
            wide.size.width,
            wide.size.height
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

#[gpui::test]
fn the_cookie_count_wraps_instead_of_being_cut(cx: &mut TestAppContext) {
    let dir = crate::workspace_tests::demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    cx.update(|_window, cx| {
        shell.update(cx, |shell, cx| {
            let crate::workspace::OpenedWorkspace::Loaded(engine) = &shell.workspace else {
                panic!("no workspace");
            };
            if let Err(error) = engine.add_cookie("https://api.example.com/", "session=abc; Path=/")
            {
                panic!("add cookie: {error}");
            }
            cx.notify();
        });
    });
    narrow_window(cx);
    click(cx, "rail-settings");
    click(cx, "nav-cookies");
    assert_not_cut(cx, &["cookies-count"]);
    assert_inside(cx, "settings-page", &["cookies-count"]);
}

#[gpui::test]
fn the_audit_header_text_wraps_instead_of_being_cut(cx: &mut TestAppContext) {
    open(cx, "rail-audit", |cx| {
        assert_not_cut(
            cx,
            &["audit-title", "audit-subtitle", "audit-count", "audit-path"],
        );
        assert_inside(cx, "audit-view", &["audit-subtitle", "audit-refresh"]);
    });
}

#[gpui::test]
fn the_approvals_header_text_wraps_instead_of_being_cut(cx: &mut TestAppContext) {
    open(cx, "rail-approvals", |cx| {
        assert_not_cut(
            cx,
            &[
                "approvals-title",
                "approvals-subtitle",
                "approvals-pending-label",
            ],
        );
        assert_inside(
            cx,
            "approvals-view",
            &["approvals-subtitle", "approvals-refresh"],
        );
    });
}

#[gpui::test]
fn every_page_header_and_count_wraps_instead_of_being_cut(cx: &mut TestAppContext) {
    for nav in [
        "nav-general",
        "nav-mcp-policy",
        "nav-network",
        "nav-keymap",
        "nav-cookies",
    ] {
        open(cx, "rail-settings", |cx| {
            click(cx, nav);
            assert_not_cut(cx, &["settings-title"]);
        });
    }
    for rail in ["rail-collections", "rail-history", "rail-environments"] {
        open(cx, rail, |cx| {
            assert_not_cut(
                cx,
                &[
                    "collections-label",
                    "history-label",
                    "history-footer",
                    "sidebar-footer",
                    "history-banner-title",
                    "history-banner-meta",
                ],
            );
        });
    }
}

#[gpui::test]
fn the_history_banner_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
    use super::history_tests::{PING, history_workspace, open_top_row, send};
    use super::send_tests::{Server, serve};

    let dir = history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(send(cx, &shell, PING).as_deref(), Some("200 OK"));
    narrow_window(cx);
    open_top_row(cx);
    assert_not_cut(cx, &["history-banner-title", "history-banner-meta"]);
    assert_inside(
        cx,
        "history-banner",
        &[
            "history-banner-title",
            "history-banner-meta",
            "history-open-source",
            "history-resend",
        ],
    );
}
