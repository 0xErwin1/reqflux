//! The Problems chip and panel (design-catch-up T3, artboard
//! `design/status-bar/02-problems-panel.html`): one chip at the right of the status bar for the
//! config error and every config and workspace load warning, opening one panel above it.

use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext, px};
use tempfile::TempDir;

use super::Shell;
use super::approvals_tests::label;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::problems::{Problem, Problems, count};
use super::sidebar_tests::sidebar_workspace;
use crate::theme::ConfigReport;
use crate::workspace_tests::demo_workspace;

const PARSE_ERROR: &str = "the config file /home/u/.config/reqflux/config.toml does not parse: \
                           line 14, column 9: expected dark, light or system";

fn config_path() -> PathBuf {
    PathBuf::from("/home/u/.config/reqflux/config.toml")
}

/// A shell on `dir` with `report` as the startup config report, watched by UI automation.
fn window<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    report: ConfigReport,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    gpui_mcp::Automation,
) {
    let (shell, cx) = render(cx, dir, state_path);
    shell.update(cx, |shell, _| shell.set_config(Some(config_path()), report));
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    cx.run_until_parked();
    (shell, cx, automation)
}

fn error_report() -> ConfigReport {
    ConfigReport {
        error: Some(PARSE_ERROR.to_owned()),
        warnings: Vec::new(),
    }
}

#[test]
fn counts_name_errors_and_warnings_in_the_singular_and_plural() {
    assert_eq!(count(1, "error"), "1 error");
    assert_eq!(count(0, "error"), "0 errors");
    assert_eq!(count(3, "warning"), "3 warnings");
}

#[test]
fn a_problem_splits_its_location_from_its_message_only_on_its_own_path() {
    let config = config_path();
    let warning = format!(
        "{}: unknown key `host` in [mcp]; it is ignored",
        config.display()
    );
    assert_eq!(
        Problem::at(&warning, &config, None),
        Problem {
            location: Some(config.display().to_string()),
            message: "unknown key `host` in [mcp]; it is ignored".to_owned(),
        }
    );
    let other = "something without a path";
    assert_eq!(
        Problem::at(other, &config, None),
        Problem {
            location: None,
            message: other.to_owned(),
        }
    );
    let root = Path::new("/w");
    let file = Path::new("/w/collections/api/broken.req.toml");
    let text = format!("{}: missing method", file.display());
    assert_eq!(
        Problem::at(&text, file, Some(root)),
        Problem {
            location: Some("collections/api/broken.req.toml".to_owned()),
            message: "missing method".to_owned(),
        }
    );
}

#[test]
fn the_summary_leads_with_the_error_and_drops_a_zero_part() {
    let warning = Problem {
        location: None,
        message: "w".to_owned(),
    };
    let both = Problems {
        error: Some(PARSE_ERROR.to_owned()),
        warnings: vec![warning.clone(), warning.clone(), warning.clone()],
    };
    assert_eq!(both.summary(), "1 error · 3 warnings");
    assert_eq!(
        both.chip_label(),
        "Problems: 1 error, 3 warnings. Show the problems panel"
    );
    let warnings = Problems {
        error: None,
        warnings: vec![warning],
    };
    assert_eq!(warnings.summary(), "1 warning");
    assert_eq!(
        warnings.chip_label(),
        "Problems: 0 errors, 1 warning. Show the problems panel"
    );
    let error = Problems {
        error: Some(PARSE_ERROR.to_owned()),
        warnings: Vec::new(),
    };
    assert_eq!(error.summary(), "1 error");
    assert!(Problems::default().is_empty());
}

#[gpui::test]
fn nothing_wrong_draws_no_problems_chip(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx, automation) = window(cx, &dir, &state_path, ConfigReport::default());
    present(cx, "status-bar");
    assert!(bounds(cx, "status-problems").is_none());
    assert!(bounds(cx, "problems-panel").is_none());
    for gone in [
        "status-warnings",
        "status-config-warning",
        "status-approvals",
    ] {
        assert_eq!(label(&automation, gone), None, "{gone} is gone");
    }
}

#[gpui::test]
fn a_config_that_does_not_load_is_a_danger_problem(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx, automation) = window(cx, &dir, &state_path, error_report());
    assert_eq!(
        label(&automation, "status-problems").as_deref(),
        Some("Problems: 1 error, 0 warnings. Show the problems panel")
    );
    assert_eq!(
        label(&automation, "status-problems-errors").as_deref(),
        Some("1 error")
    );
    assert_eq!(label(&automation, "status-problems-warnings"), None);
    assert!(
        cx.update(|_, cx| shell.read(cx).problems().has_error()),
        "the chip takes the danger wash"
    );

    let bar = present(cx, "status-bar");
    let chip = present(cx, "status-problems");
    assert_eq!(chip.size.height, px(26.0));
    assert_eq!(
        chip.origin.x + chip.size.width,
        bar.origin.x + bar.size.width - px(8.0),
        "the chip sits at the right of the status bar"
    );

    click(cx, "status-problems");
    let panel = present(cx, "problems-panel");
    assert_eq!(panel.size.width, px(540.0));
    assert_eq!(
        panel.origin.x + panel.size.width,
        chip.origin.x + chip.size.width,
        "the panel aligns with the chip's right edge"
    );
    assert_eq!(
        panel.origin.y + panel.size.height,
        chip.origin.y - px(12.0),
        "the panel opens above the chip"
    );
    // The header of DBFlux's notifications popover, `NotificationMetrics::HEADER_HEIGHT`
    // (was 44).
    assert_eq!(present(cx, "problems-header").size.height, px(46.0));
    for (id, text) in [
        ("problems-title", "Problems"),
        ("problems-count", "1 error"),
        ("problems-keycap", "Esc"),
        ("problems-config-label", "CONFIG"),
        (
            "problems-config-error-title",
            "Config not loaded · defaults in use",
        ),
        ("problems-config-error-detail", PARSE_ERROR),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(text), "{id}");
    }
    assert!(bounds(cx, "problems-warnings-label").is_none());

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "problems-panel").is_none(), "Escape closes it");
}

#[gpui::test]
fn config_and_workspace_warnings_share_the_chip_and_the_panel(cx: &mut TestAppContext) {
    let dir = sidebar_workspace();
    let (_state_dir, state_path) = state_file();
    let config = config_path();
    let report = ConfigReport {
        error: None,
        warnings: vec![format!(
            "{}: unknown key `host` in [mcp]; it is ignored",
            config.display()
        )],
    };
    let (shell, cx, automation) = window(cx, &dir, &state_path, report);
    assert_eq!(
        label(&automation, "status-problems").as_deref(),
        Some("Problems: 0 errors, 2 warnings. Show the problems panel")
    );
    assert_eq!(label(&automation, "status-problems-errors"), None);
    assert_eq!(
        label(&automation, "status-problems-warnings").as_deref(),
        Some("2 warnings")
    );
    assert!(!cx.update(|_, cx| shell.read(cx).problems().has_error()));

    click(cx, "status-problems");
    present(cx, "problems-panel");
    assert!(
        bounds(cx, "problems-config-label").is_none(),
        "no error, no Config"
    );
    assert_eq!(
        label(&automation, "problems-warnings-label").as_deref(),
        Some("WARNINGS · 2")
    );
    assert_eq!(
        label(&automation, "problems-warning-0-location").as_deref(),
        Some(config.display().to_string().as_str()),
        "config warnings come first"
    );
    assert_eq!(
        label(&automation, "problems-warning-0-message").as_deref(),
        Some("unknown key `host` in [mcp]; it is ignored")
    );
    let workspace = label(&automation, "problems-warning-1-location").unwrap_or_default();
    assert!(
        !workspace.starts_with('/') && !workspace.is_empty(),
        "a workspace warning names its file inside the workspace: {workspace}"
    );
    assert!(label(&automation, "problems-warning-1-message").is_some());
    assert!(bounds(cx, "problems-warning-2").is_none());

    click(cx, "problems-backdrop");
    assert!(
        bounds(cx, "problems-panel").is_none(),
        "a click outside closes it"
    );
}
