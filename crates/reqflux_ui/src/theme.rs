//! The palette mode the user chose (light-theme S2, S4; user-config-mcp-policy S2): kept as
//! `theme` in the user's config file, installed before the first window opens, and followed
//! to the OS appearance in [`PaletteMode::System`].
//!
//! Earlier builds kept it in the app state under [`THEME_SETTING`]; [`startup`] moves that
//! value into the config file the first time the file has no `theme`.

use std::path::Path;

use bolt::modals::{
    MODAL_KEY_CONTEXT, ModalKey, ModalKeyHandler, ModalSeams, ScrollKey, on_modal_key,
};
pub(crate) use bolt::palette::PaletteMode;
use gpui::{App, Div, Global, KeyBinding, Stateful, Window, WindowAppearance, actions};
use gpui_kit_assets::IconName;
use reqflux_engine::config::{self, ThemeMode};
use reqflux_engine::state::AppState;

/// The app state setting earlier builds kept the palette mode in.
pub(crate) const THEME_SETTING: &str = "theme";

/// The label of the key that cancels a modal, on its Cancel keycap.
pub(crate) const MODAL_ESCAPE_LABEL: &str = "Esc";

actions!(
    modal,
    [
        /// Closes the open modal.
        CancelModal,
        /// Confirms the open modal.
        ConfirmModal,
        /// Scrolls the open modal's body a line up.
        ScrollModalUp,
        /// Scrolls the open modal's body a line down.
        ScrollModalDown,
        /// Scrolls the open modal's body a page up.
        ScrollModalPageUp,
        /// Scrolls the open modal's body a page down.
        ScrollModalPageDown,
        /// Scrolls the open modal's body to its top.
        ScrollModalToTop,
        /// Scrolls the open modal's body to its bottom.
        ScrollModalToBottom,
    ]
);

/// What the window starts with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Startup {
    /// The palette mode to install.
    pub(crate) mode: PaletteMode,
    /// The interface font size to install, `font_size` of the config file.
    pub(crate) font_size: f32,
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
        font_size: config::DEFAULT_FONT_SIZE,
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
        font_size: loaded.interface_font_size(),
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

/// The mode the user selected, which `bolt` keeps only resolved: [`PaletteMode::System`]
/// stays here so the window can follow the OS and `Toggle theme` can cycle from it.
struct SelectedMode(PaletteMode);

impl Global for SelectedMode {}

/// The mode the user selected, Dark before [`install`].
pub(crate) fn mode(cx: &App) -> PaletteMode {
    cx.try_global::<SelectedMode>()
        .map(|selected| selected.0)
        .unwrap_or_default()
}

/// The mode after `mode` in the cycle Dark, Light, System.
pub(crate) fn next(mode: PaletteMode) -> PaletteMode {
    match mode {
        PaletteMode::Dark => PaletteMode::Light,
        PaletteMode::Light => PaletteMode::System,
        PaletteMode::System => PaletteMode::Dark,
    }
}

/// Installs `bolt`, DBFlux's design system, with ReqFlux's brand in `mode`: the bundled
/// fonts at the default interface size, where a rem is 16 px as in DBFlux, the default
/// density, and DBFlux's palette drawn with ReqFlux's accent on gpui-component's `Theme`.
/// [`set_font_size`] then applies the config file's interface size.
pub(crate) fn install(mode: PaletteMode, cx: &mut App) {
    if !cx.has_global::<gpui_component::theme::Theme>() {
        gpui_component::init(cx);
    }
    bolt::typography::load_bundled_fonts(cx);
    bolt::fonts::init(cx, bolt::fonts::FontSettings::default());
    bolt::density::init(cx, bolt::density::Density::Default);
    cx.set_global(SelectedMode(mode));
    bolt::theme::install(
        crate::brand::reqflux(),
        mode,
        bolt::density::Density::Default,
        cx,
    );
    install_modal_seams(cx);
    cx.set_global(bolt::composites::ShellIcons {
        search: bolt::icon::IconSource::Svg(IconName::Search.path()),
        bell: bolt::icon::IconSource::Svg(IconName::Bell.path()),
    });
}

/// Installs what `bolt`'s modals read from the product, as DBFlux's theme does: ReqFlux's
/// modal actions, bound in the `Modal` key context (Escape cancels, Enter confirms, the
/// arrows, Page Up, Page Down, Home and End scroll), the Escape keycap, and the close
/// button's icon and name. A modal rendered without them answers no keys.
fn install_modal_seams(cx: &mut App) {
    cx.set_global(ModalSeams {
        attach_keys: attach_modal_keys,
        // No ReqFlux modal leaves its keys to its owner.
        defer_to_owner: |_, _, _| {},
        escape_label: |_| Some(MODAL_ESCAPE_LABEL.into()),
        close_icon: bolt::icon::IconSource::Svg(IconName::CircleX.path()),
        close_label: || "Close".into(),
    });
    let context = Some(MODAL_KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("escape", CancelModal, context),
        KeyBinding::new("enter", ConfirmModal, context),
        KeyBinding::new("up", ScrollModalUp, context),
        KeyBinding::new("down", ScrollModalDown, context),
        KeyBinding::new("pageup", ScrollModalPageUp, context),
        KeyBinding::new("pagedown", ScrollModalPageDown, context),
        KeyBinding::new("home", ScrollModalToTop, context),
        KeyBinding::new("end", ScrollModalToBottom, context),
    ]);
}

/// Registers the backdrop handlers of ReqFlux's modal actions.
fn attach_modal_keys(backdrop: Stateful<Div>, handler: ModalKeyHandler) -> Stateful<Div> {
    let scroll = |key| ModalKey::Scroll(key);
    let backdrop = on_modal_key::<CancelModal>(backdrop, ModalKey::Cancel, &handler);
    let backdrop = on_modal_key::<ConfirmModal>(backdrop, ModalKey::Execute, &handler);
    let backdrop = on_modal_key::<ScrollModalUp>(backdrop, scroll(ScrollKey::LineUp), &handler);
    let backdrop = on_modal_key::<ScrollModalDown>(backdrop, scroll(ScrollKey::LineDown), &handler);
    let backdrop = on_modal_key::<ScrollModalPageUp>(backdrop, scroll(ScrollKey::PageUp), &handler);
    let backdrop =
        on_modal_key::<ScrollModalPageDown>(backdrop, scroll(ScrollKey::PageDown), &handler);
    let backdrop = on_modal_key::<ScrollModalToTop>(backdrop, scroll(ScrollKey::Top), &handler);
    on_modal_key::<ScrollModalToBottom>(backdrop, scroll(ScrollKey::Bottom), &handler)
}

/// Resolves the installed mode against `window`'s appearance now and whenever the OS changes
/// it, so [`PaletteMode::System`] follows the OS.
pub(crate) fn follow_appearance(window: &mut Window, cx: &mut App) {
    set_mode(mode(cx), window.appearance(), cx);
    window
        .observe_window_appearance(|window, cx| {
            if mode(cx) == PaletteMode::System {
                set_mode(PaletteMode::System, window.appearance(), cx);
            }
        })
        .detach();
}

/// The font settings at the interface size `size` (clamped as the config file reads it):
/// the interface and editor at `size`, the data grid half a pixel smaller, as DBFlux's
/// defaults are 13, 13 and 12.5.
pub(crate) fn font_settings(size: f32) -> bolt::fonts::FontSettings {
    let size = config::clamp_font_size(size);
    bolt::fonts::FontSettings {
        ui_size: size,
        editor_size: size,
        grid_size: bolt::fonts::clamp_size(
            size - (bolt::fonts::DEFAULT_UI_FONT_SIZE - bolt::fonts::DEFAULT_GRID_FONT_SIZE),
            bolt::fonts::DEFAULT_GRID_FONT_SIZE,
        ),
        ..bolt::fonts::FontSettings::default()
    }
}

/// Sets the interface font size at runtime through `bolt`'s theme, so every open window
/// redraws at it.
pub(crate) fn set_font_size(size: f32, cx: &mut App) {
    if bolt::fonts::set(cx, font_settings(size)) {
        bolt::theme::apply_fonts(cx);
        cx.refresh_windows();
    }
}

/// Switches to `mode` at runtime, resolved against `appearance` (a window's appearance).
pub(crate) fn set_mode(mode: PaletteMode, appearance: WindowAppearance, cx: &mut App) {
    cx.set_global(SelectedMode(mode));
    let resolved = bolt::theme::resolve_mode(mode, appearance);
    bolt::theme::apply_theme(resolved, bolt::density::active(cx), None, cx);
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
