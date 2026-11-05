//! Keyboard zoom (bolt-gaps S2): Ctrl+= and Ctrl+- step the interface size by one pixel
//! within the config file's bounds, Ctrl+0 resets it, and every change saves `font_size`
//! in the user's config file, as DBFlux's interface size commands do.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::config;
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{render, state_file};
use super::toasts::tests as toasts;
use crate::theme::ConfigReport;
use crate::workspace_tests::demo_workspace;

fn config_file() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = dir.path().join("config.toml");
    (dir, path)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn render_with_config<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    config_path: &Path,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = render(cx, dir, state_path);
    shell.update(cx, |shell, _| {
        shell.set_config(Some(config_path.to_path_buf()), ConfigReport::default());
    });
    cx.update(|window, cx| {
        let focus = shell.read(cx).initial_focus();
        focus.focus(window, cx);
    });
    cx.run_until_parked();
    (shell, cx)
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

fn size(cx: &mut VisualTestContext) -> f32 {
    cx.update(|_, cx| bolt::fonts::current(cx).ui_size)
}

#[gpui::test]
fn ctrl_plus_minus_and_zero_step_and_reset_the_interface_size(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let (_shell, cx) = render_with_config(cx, &dir, &state_path, &config_path);
    assert_eq!(size(cx), config::DEFAULT_FONT_SIZE);

    press(cx, "ctrl-=");
    assert_eq!(size(cx), 14.0);
    assert_eq!(read(&config_path), "font_size = 14\n");
    // `+` shares the key with `=`, and some platforms report the chord with Shift.
    press(cx, "ctrl-+");
    press(cx, "ctrl-shift-=");
    assert_eq!(size(cx), 16.0);
    assert_eq!(read(&config_path), "font_size = 16\n");

    press(cx, "ctrl--");
    assert_eq!(size(cx), 15.0);
    assert_eq!(read(&config_path), "font_size = 15\n");

    press(cx, "ctrl-0");
    assert_eq!(size(cx), config::DEFAULT_FONT_SIZE);
    assert_eq!(read(&config_path), "font_size = 13\n");
}

#[gpui::test]
fn a_step_past_the_limits_changes_and_saves_nothing(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let (_shell, cx) = render_with_config(cx, &dir, &state_path, &config_path);

    cx.update(|_, cx| crate::theme::set_font_size(config::MAX_FONT_SIZE, cx));
    press(cx, "ctrl-=");
    assert_eq!(size(cx), config::MAX_FONT_SIZE);
    assert_eq!(read(&config_path), "", "a step at the limit writes nothing");

    cx.update(|_, cx| crate::theme::set_font_size(config::MIN_FONT_SIZE, cx));
    press(cx, "ctrl--");
    assert_eq!(size(cx), config::MIN_FONT_SIZE);
    assert_eq!(read(&config_path), "");

    cx.update(|_, cx| crate::theme::set_font_size(config::DEFAULT_FONT_SIZE, cx));
    press(cx, "ctrl-0");
    assert_eq!(
        read(&config_path),
        "",
        "a reset at the default writes nothing"
    );
}

#[gpui::test]
fn a_zoom_that_cannot_save_still_applies_and_shows_an_error(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    // A directory where the config file should be: the write fails.
    let config = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let (_shell, cx) = render_with_config(cx, &dir, &state_path, config.path());

    press(cx, "ctrl-=");
    assert_eq!(size(cx), 14.0);
    let pushed = toasts::pushed(cx);
    assert_eq!(
        pushed
            .iter()
            .map(|toast| toast.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Cannot save the interface size"]
    );
}

#[gpui::test]
fn the_open_app_settings_show_the_zoomed_size(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let (shell, cx) = render_with_config(cx, &dir, &state_path, &config_path);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_app_settings(window, cx)));
    cx.run_until_parked();
    let shown = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            shell
                .read(cx)
                .settings
                .app
                .font_size_control()
                .and_then(|dropdown| dropdown.read(cx).selected_label())
                .map(|label| label.to_string())
        })
    };
    assert_eq!(shown(cx).as_deref(), Some("13 px"));

    press(cx, "ctrl-=");
    assert_eq!(shown(cx).as_deref(), Some("14 px"));
}

#[gpui::test]
fn a_zoom_that_cannot_save_shows_the_applied_size_in_the_open_settings(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    // A directory where the config file should be: reading and writing it fail.
    let config = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let (shell, cx) = render_with_config(cx, &dir, &state_path, config.path());
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_app_settings(window, cx)));
    cx.run_until_parked();

    press(cx, "ctrl-=");
    assert_eq!(size(cx), 14.0);
    let shown = cx.update(|_, cx| {
        shell
            .read(cx)
            .settings
            .app
            .font_size_control()
            .and_then(|dropdown| dropdown.read(cx).selected_label())
            .map(|label| label.to_string())
    });
    assert_eq!(
        shown.as_deref(),
        Some("14 px"),
        "the select shows the size in use"
    );
}
