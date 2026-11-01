//! The palette mode the user chose (light-theme S2, S4; user-config-mcp-policy S2): kept as
//! `theme` in the user's config file, installed before the first window opens, and followed
//! to the OS appearance in [`PaletteMode::System`].
//!
//! Earlier builds kept it in the app state under [`THEME_SETTING`]; [`startup`] moves that
//! value into the config file the first time the file has no `theme`.

use std::path::Path;

use bolt_legacy::PaletteMode;
use gpui::{App, Window, WindowAppearance};
use reqflux_engine::config::{self, ThemeMode};
use reqflux_engine::state::AppState;

/// The app state setting earlier builds kept the palette mode in.
pub(crate) const THEME_SETTING: &str = "theme";

/// What the window starts with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Startup {
    /// The palette mode to install.
    pub(crate) mode: PaletteMode,
    /// What went wrong reading the config file; the Problems panel shows it.
    pub(crate) config: ConfigReport,
}

/// The problems of the config file at startup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfigReport {
    /// Why the file was not used, when it does not parse or cannot be read: the defaults
    /// apply. Never quotes the file.
    pub(crate) error: Option<String>,
    /// What the file sets that applies to nothing (unknown `[mcp]` keys, invalid
    /// `[mcp.workspaces]` keys); the rest of the file still applies.
    pub(crate) warnings: Vec<String>,
}

/// The palette mode of a config file `theme`.
pub(crate) fn palette_mode(mode: ThemeMode) -> PaletteMode {
    match mode {
        ThemeMode::Dark => PaletteMode::Dark,
        ThemeMode::Light => PaletteMode::Light,
        ThemeMode::System => PaletteMode::System,
    }
}

/// The config file `theme` of a palette mode.
pub(crate) fn theme_mode(mode: PaletteMode) -> ThemeMode {
    match mode {
        PaletteMode::Dark => ThemeMode::Dark,
        PaletteMode::Light => ThemeMode::Light,
        PaletteMode::System => ThemeMode::System,
    }
}

/// The mode to start in, from the config file at `config` (`None` when there is no config
/// directory). A file without `theme` takes the app state's old setting and writes it to the
/// file once; a file that does not parse gives the defaults and an error, and is left alone.
/// Unknown `[mcp]` keys and invalid `[mcp.workspaces]` keys are warnings, beside the file's
/// settings.
pub(crate) fn startup(config: Option<&Path>, state: Option<&AppState>) -> Startup {
    let defaults = Startup {
        mode: PaletteMode::default(),
        config: ConfigReport::default(),
    };
    let Some(path) = config else {
        return defaults;
    };
    let (loaded, warnings) = match config::load_with_warnings(path) {
        Ok(loaded) => loaded,
        Err(error) => {
            log::warn!("{error}; using the defaults");
            return Startup {
                config: ConfigReport {
                    error: Some(error.to_string()),
                    warnings: Vec::new(),
                },
                ..defaults
            };
        }
    };
    for warning in &warnings {
        log::warn!("{warning}");
    }
    let defaults = Startup {
        config: ConfigReport {
            error: None,
            warnings,
        },
        ..defaults
    };
    if let Some(mode) = loaded.theme {
        return Startup {
            mode: palette_mode(mode),
            ..defaults
        };
    }
    let Some(mode) = legacy_mode(state) else {
        return defaults;
    };
    if let Err(error) = config::set_theme(path, mode) {
        log::warn!("cannot move the theme into the config file: {error}");
    }
    Startup {
        mode: palette_mode(mode),
        ..defaults
    }
}

/// The mode earlier builds saved in `state`; `None` when there is none, it is unknown or it
/// cannot be read (which is logged).
fn legacy_mode(state: Option<&AppState>) -> Option<ThemeMode> {
    match state?.setting(THEME_SETTING) {
        Ok(Some(name)) => {
            let mode = ThemeMode::from_name(&name);
            if mode.is_none() {
                log::warn!("unknown theme {name:?} in the app state; ignoring it");
            }
            mode
        }
        Ok(None) => None,
        Err(error) => {
            log::warn!("cannot read the old theme from the app state: {error}");
            None
        }
    }
}

/// Saves `mode` in the config file at `config`, logging a failure: the switch itself already
/// happened. Without a config path nothing is saved.
pub(crate) fn save_mode(config: Option<&Path>, mode: PaletteMode) {
    let Some(path) = config else {
        return;
    };
    if let Err(error) = config::set_theme(path, theme_mode(mode)) {
        log::warn!("cannot save the theme: {error}");
    }
}

/// Installs both design systems with ReqFlux's brand in `mode`: the shared `bolt` first, then
/// `bolt_legacy` over it.
///
/// Both map their palette onto gpui-component's one `Theme`. Screens still draw with
/// `bolt_legacy`, so it goes last and its colors, metrics and editor colors are what
/// gpui-component draws. The shared install still sets what only it owns: its brand, palette
/// mode, density and fonts, and the rem size kept in the stored light and dark configs,
/// which `bolt_legacy` never writes, so it survives every later switch. The reverse order
/// would repaint today's screens in DBFlux's palette.
pub(crate) fn install(mode: PaletteMode, cx: &mut App) {
    if !cx.has_global::<gpui_component::theme::Theme>() {
        gpui_component::init(cx);
    }
    bolt::fonts::init(cx, shared_fonts());
    bolt::density::init(cx, bolt::density::Density::Default);
    bolt::theme::install(
        crate::brand::shared(),
        shared_mode(mode),
        bolt::density::Density::Default,
        cx,
    );
    bolt_legacy::install(crate::brand::reqflux(), mode, cx);
}

/// ReqFlux's fonts for the shared `bolt`, at the default interface size (13), where a rem is
/// 16 px as in DBFlux. ReqFlux has no interface size setting yet.
fn shared_fonts() -> bolt::fonts::FontSettings {
    use bolt_legacy::tokens::family;
    bolt::fonts::FontSettings {
        ui_family: family::INTERFACE.into(),
        ui_size: bolt::fonts::DEFAULT_UI_FONT_SIZE,
        editor_family: family::MONO.into(),
        editor_size: bolt::fonts::DEFAULT_EDITOR_FONT_SIZE,
        grid_family: family::MONO.into(),
        grid_size: bolt::fonts::DEFAULT_GRID_FONT_SIZE,
    }
}

/// The shared `bolt` mode of a `bolt_legacy` mode.
fn shared_mode(mode: PaletteMode) -> bolt::palette::PaletteMode {
    match mode {
        PaletteMode::Dark => bolt::palette::PaletteMode::Dark,
        PaletteMode::Light => bolt::palette::PaletteMode::Light,
        PaletteMode::System => bolt::palette::PaletteMode::System,
    }
}

/// Resolves the installed mode against `window`'s appearance now and whenever the OS changes
/// it, so [`PaletteMode::System`] follows the OS.
pub(crate) fn follow_appearance(window: &mut Window, cx: &mut App) {
    set_mode(bolt_legacy::mode(cx), window.appearance(), cx);
    window
        .observe_window_appearance(|window, cx| {
            if bolt_legacy::mode(cx) == PaletteMode::System {
                set_mode(PaletteMode::System, window.appearance(), cx);
            }
        })
        .detach();
}

/// Switches both design systems to `mode` at runtime, resolved against `appearance` (a
/// window's appearance), in the order [`install`] uses.
pub(crate) fn set_mode(mode: PaletteMode, appearance: WindowAppearance, cx: &mut App) {
    let resolved = bolt::theme::resolve_mode(shared_mode(mode), appearance);
    bolt::theme::apply_theme(resolved, bolt::density::active(cx), None, cx);
    bolt_legacy::set_mode(mode, appearance, cx);
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
