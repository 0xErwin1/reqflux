//! The theme command (light-theme S2-S4): Ctrl+Shift+T and the palette cycle Dark, Light and
//! System, the palette and product colors follow, and the choice persists in the user's config
//! file (user-config-mcp-policy S1, S2). Every test uses a config file in a temp directory.

use std::path::{Path, PathBuf};

use bolt::Colors;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::config::{self, ThemeMode};
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{render, state_file};
use crate::theme::{ConfigReport, PaletteMode};
use crate::tokens::{Palette, method_color};
use crate::workspace_tests::demo_workspace;

fn focus_shell(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|window, cx| {
        let focus = shell.read(cx).initial_focus();
        focus.focus(window, cx);
    });
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

fn mode(cx: &mut VisualTestContext) -> PaletteMode {
    cx.update(|_, cx| crate::theme::mode(cx))
}

/// The color roles `bolt` draws with.
fn colors(cx: &mut VisualTestContext) -> Colors {
    cx.update(|_, cx| bolt::colors(cx))
}

/// The palette `bolt` draws.
fn palette(cx: &mut VisualTestContext) -> Palette {
    cx.update(|_, cx| Palette::current(cx))
}

/// A config file path in a fresh temp directory; the file does not exist yet.
fn config_file() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = dir.path().join("reqflux/config.toml");
    (dir, path)
}

/// The `theme` the config file at `path` holds.
fn saved(path: &Path) -> Option<ThemeMode> {
    match config::load(path) {
        Ok(config) => config.theme,
        Err(error) => panic!("read the config: {error}"),
    }
}

fn open_state(path: &Path) -> AppState {
    match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open the state: {error}"),
    }
}

fn legacy_setting(state: &AppState) -> Option<String> {
    match state.setting(crate::theme::THEME_SETTING) {
        Ok(value) => value,
        Err(error) => panic!("read the setting: {error}"),
    }
}

/// Renders the shell over `config_path`, as the app does after [`crate::theme::startup`].
fn render_with_config<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    config_path: &Path,
    report: ConfigReport,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = render(cx, dir, state_path);
    shell.update(cx, |shell, _| {
        shell.set_config(Some(config_path.to_path_buf()), report);
    });
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn ctrl_shift_t_cycles_dark_light_system_and_the_palette_follows(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let (shell, cx) =
        render_with_config(cx, &dir, &state_path, &config_path, ConfigReport::default());
    focus_shell(cx, &shell);
    assert_eq!(mode(cx), PaletteMode::Dark, "dark is the default");
    assert_eq!(colors(cx), Colors::dark());
    let dark_get = method_color("GET", palette(cx));

    press(cx, "ctrl-shift-t");
    assert_eq!(mode(cx), PaletteMode::Light);
    assert_eq!(colors(cx), Colors::light());
    let light_get = method_color("GET", palette(cx));
    assert_ne!(dark_get, light_get, "a product color follows the palette");
    assert_eq!(saved(&config_path), Some(ThemeMode::Light));

    press(cx, "ctrl-shift-t");
    assert_eq!(mode(cx), PaletteMode::System);
    let appearance = cx.update(|window, _| window.appearance());
    let expected = match bolt::theme::resolve_mode(PaletteMode::System, appearance) {
        PaletteMode::Light => Colors::light(),
        PaletteMode::Dark | PaletteMode::System => Colors::dark(),
    };
    assert_eq!(colors(cx), expected);
    assert_eq!(saved(&config_path), Some(ThemeMode::System));

    press(cx, "ctrl-shift-t");
    assert_eq!(mode(cx), PaletteMode::Dark);
    assert_eq!(colors(cx), Colors::dark());
    assert_eq!(saved(&config_path), Some(ThemeMode::Dark));
    assert_eq!(
        legacy_setting(&open_state(&state_path)),
        None,
        "the app state no longer holds the theme"
    );
}

#[gpui::test]
fn the_palette_lists_toggle_theme_and_runs_it(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    focus_shell(cx, &shell);

    press(cx, "ctrl-k");
    cx.simulate_input("toggle theme");
    cx.run_until_parked();
    let tree = automation.snapshot();
    let first = tree
        .nodes
        .get("palette-row-1")
        .and_then(|row| row.label.clone());
    assert_eq!(first.as_deref(), Some("Toggle theme Ctrl+Shift+T"));

    press(cx, "enter");
    assert_eq!(mode(cx), PaletteMode::Light);
    assert_eq!(palette(cx), Palette::Light);
}

#[gpui::test]
fn the_choice_is_installed_again_from_the_same_config_file(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    {
        let (shell, cx) =
            render_with_config(cx, &dir, &state_path, &config_path, ConfigReport::default());
        focus_shell(cx, &shell);
        press(cx, "ctrl-shift-t");
        assert_eq!(mode(cx), PaletteMode::Light);
    }
    cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));

    let state = open_state(&state_path);
    let startup = crate::theme::startup(Some(&config_path), Some(&state));
    assert_eq!(startup.config, ConfigReport::default());
    cx.update(|cx| crate::theme::install(startup.mode, cx));

    cx.update(|cx| {
        assert_eq!(crate::theme::mode(cx), PaletteMode::Light);
        assert_eq!(bolt::colors(cx), Colors::light());
    });
    let (_shell, cx) = cx.add_window_view(|_, cx| {
        Shell::with_state(
            crate::workspace::OpenedWorkspace::open(Some(dir.path().to_path_buf())),
            Some(state),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(mode(cx), PaletteMode::Light, "a new shell keeps it");
}

#[test]
fn startup_reads_the_theme_from_the_config_file() {
    let (_config_dir, config_path) = config_file();
    if let Err(error) = config::set_theme(&config_path, ThemeMode::System) {
        panic!("write the config: {error}");
    }
    let startup = crate::theme::startup(Some(&config_path), None);
    assert_eq!(startup.mode, PaletteMode::System);
    assert_eq!(startup.config, ConfigReport::default());
}

#[test]
fn an_unknown_mcp_key_is_the_startup_warning_and_the_file_still_applies() {
    let (_config_dir, config_path) = config_file();
    if let Err(error) = config::set_theme(&config_path, ThemeMode::Light) {
        panic!("write the config: {error}");
    }
    let mut text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(error) => panic!("read the config: {error}"),
    };
    text.push_str("[mcp]\nhost = [\"api.example.com\"]\n");
    if let Err(error) = std::fs::write(&config_path, text) {
        panic!("write the config: {error}");
    }
    let startup = crate::theme::startup(Some(&config_path), None);
    assert_eq!(startup.mode, PaletteMode::Light);
    assert_eq!(startup.config.error, None, "the file still applies");
    assert_eq!(
        startup.config.warnings,
        vec![format!(
            "{}: unknown key `host` in [mcp]; it is ignored",
            config_path.display()
        )]
    );
}

#[test]
fn the_old_app_state_setting_moves_to_the_config_file_once() {
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let mut state = open_state(&state_path);
    if let Err(error) = state.set_setting(crate::theme::THEME_SETTING, "light") {
        panic!("seed the setting: {error}");
    }

    let first = crate::theme::startup(Some(&config_path), Some(&state));
    assert_eq!(first.mode, PaletteMode::Light, "the old setting is used");
    assert_eq!(saved(&config_path), Some(ThemeMode::Light), "and written");

    if let Err(error) = state.set_setting(crate::theme::THEME_SETTING, "system") {
        panic!("change the setting: {error}");
    }
    let second = crate::theme::startup(Some(&config_path), Some(&state));
    assert_eq!(
        second.mode,
        PaletteMode::Light,
        "the file wins from then on"
    );
    assert_eq!(saved(&config_path), Some(ThemeMode::Light));
}

#[test]
fn without_a_config_file_the_app_still_defaults_to_dark() {
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    let state = open_state(&state_path);
    let startup = crate::theme::startup(Some(&config_path), Some(&state));
    assert_eq!(startup.mode, PaletteMode::Dark);
    assert_eq!(startup.config, ConfigReport::default());
    assert!(!config_path.exists(), "nothing to migrate writes nothing");
    assert_eq!(crate::theme::startup(None, None).mode, PaletteMode::Dark);
}

#[gpui::test]
fn a_config_file_that_does_not_parse_opens_on_the_defaults_and_says_so(cx: &mut TestAppContext) {
    let dir = demo_workspace();
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_file();
    if let Some(parent) = config_path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        panic!("config dir: {error}");
    }
    let broken = "theme = \"light\"\ntoken = sk-live-SECRET123\n";
    if let Err(error) = std::fs::write(&config_path, broken) {
        panic!("write the config: {error}");
    }
    let mut state = open_state(&state_path);
    if let Err(error) = state.set_setting(crate::theme::THEME_SETTING, "system") {
        panic!("seed the setting: {error}");
    }

    let startup = crate::theme::startup(Some(&config_path), Some(&state));
    assert_eq!(startup.mode, PaletteMode::Dark, "the defaults");
    assert!(startup.config.warnings.is_empty());
    let warning = match startup.config.error.clone() {
        Some(warning) => warning,
        None => panic!("a broken config file must be reported"),
    };
    assert!(warning.contains("line 2"), "{warning}");
    assert!(!warning.contains("SECRET123"), "{warning}");
    assert!(
        std::fs::read_to_string(&config_path).is_ok_and(|text| text == broken),
        "a broken file is never rewritten"
    );

    let (_shell, cx) = render_with_config(cx, &dir, &state_path, &config_path, startup.config);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    cx.run_until_parked();
    let label = |id: &str| {
        automation
            .snapshot()
            .nodes
            .get(id)
            .and_then(|node| node.label.clone())
    };
    assert_eq!(
        label("status-problems").as_deref(),
        Some("Problems: 1 error, 0 warnings. Show the problems panel")
    );
    super::draft_tests::click(cx, "status-problems");
    assert_eq!(
        label("problems-config-error-detail").as_deref(),
        Some(warning.as_str())
    );
}

#[test]
fn theme_modes_map_both_ways() {
    for (palette, theme) in [
        (PaletteMode::Dark, ThemeMode::Dark),
        (PaletteMode::Light, ThemeMode::Light),
        (PaletteMode::System, ThemeMode::System),
    ] {
        assert_eq!(crate::theme::palette_mode(theme), palette);
        assert_eq!(crate::theme::theme_mode(palette), theme);
    }
}
