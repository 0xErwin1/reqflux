//! Installing Bolt onto gpui-component and reading its roles.
//!
//! [`install`] maps the tokens of the chosen palette and a [`Brand`] onto gpui-component's
//! `Theme`, so its widgets draw in Bolt, and keeps the roles in a global that screens read
//! through [`colors`], [`brand`] and [`accent_washes`]. [`set_mode`] switches the palette at
//! runtime. Screens never read `Theme` fields.

use std::rc::Rc;
use std::sync::LazyLock;

use gpui::{App, Global, Hsla, WindowAppearance};
use gpui_component::theme::{Theme, ThemeConfig, ThemeMode};

use crate::brand::{AccentWashes, Brand};
use crate::fonts;
use crate::palette::{Palette, PaletteMode};
use crate::tokens::{Colors, cut, family, text};

/// The installed Bolt roles.
#[derive(Debug, Clone)]
struct Installed {
    colors: Colors,
    /// The brand as the product passed it.
    base_brand: Brand,
    /// The brand as it draws on the active palette.
    brand: Brand,
    washes: AccentWashes,
    mode: PaletteMode,
    palette: Palette,
}

impl Global for Installed {}

/// What the accessors return before [`install`] runs: the dark palette with a neutral
/// accent, so nothing has to fail.
static FALLBACK: LazyLock<Installed> = LazyLock::new(|| {
    let colors = Colors::dark();
    let brand = Brand {
        name: "".into(),
        accent: colors.raised,
        accent_hover: colors.line,
        accent_pressed: colors.line_2,
        accent_tint: colors.strong,
        on_accent: colors.strong,
    };
    let washes = brand.washes();
    Installed {
        colors,
        base_brand: brand.clone(),
        brand,
        washes,
        mode: PaletteMode::Dark,
        palette: Palette::Dark,
    }
});

/// Installs Bolt for `brand` in `mode`: initializes gpui-component when it is not yet,
/// registers the bundled fonts, and applies the palette `mode` resolves to and the brand to
/// gpui-component's theme. [`PaletteMode::System`] resolves through the app's window
/// appearance; call [`set_mode`] with a window's own appearance once one is open.
///
/// Call it once before opening the first window. Calling it again replaces the brand.
/// Calling `gpui_component::theme::Theme::change` afterwards reloads gpui-component's
/// own theme over Bolt, so do not; switch palettes with [`set_mode`].
///
/// The theme keeps gpui-component's 16 px rem size, which `Root` applies to each window;
/// Bolt sizes are absolute pixels and do not depend on it.
pub fn install(brand: Brand, mode: PaletteMode, cx: &mut App) {
    if !cx.has_global::<Theme>() {
        gpui_component::init(cx);
    }
    fonts::load(cx);
    let appearance = cx.window_appearance();
    publish(brand, mode, appearance, cx);
}

/// Switches to `mode` at runtime: resolves it against `appearance` (a window's
/// `Window::appearance`), publishes that palette and the installed brand drawn on it, maps
/// them onto gpui-component's light or dark theme and refreshes every window.
///
/// With [`PaletteMode::System`], call it again from the window's appearance observer so the
/// palette follows the OS.
pub fn set_mode(mode: PaletteMode, appearance: WindowAppearance, cx: &mut App) {
    let brand = installed(cx).base_brand.clone();
    publish(brand, mode, appearance, cx);
}

fn publish(brand: Brand, mode: PaletteMode, appearance: WindowAppearance, cx: &mut App) {
    let palette = mode.resolve(appearance);
    let colors = palette.colors();
    let active = brand.for_palette(palette);
    let washes = active.washes();

    let theme_mode = if palette.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(theme_mode, None, cx);
    Theme::update(cx, |theme| {
        apply_metrics(theme);
        apply_colors(theme, &colors, &active, &washes);
    });

    cx.set_global(Installed {
        colors,
        base_brand: brand,
        brand: active,
        washes,
        mode,
        palette,
    });
    cx.refresh_windows();
}

/// The color roles of the installed palette.
pub fn colors(cx: &App) -> &Colors {
    &installed(cx).colors
}

/// The installed brand, as it draws on the active palette.
pub fn brand(cx: &App) -> &Brand {
    &installed(cx).brand
}

/// The washes derived from the installed brand's accent tint on the active palette.
pub fn accent_washes(cx: &App) -> &AccentWashes {
    &installed(cx).washes
}

/// The mode the user chose.
pub fn mode(cx: &App) -> PaletteMode {
    installed(cx).mode
}

/// The palette drawing now: the mode resolved against the window appearance.
pub fn palette(cx: &App) -> Palette {
    installed(cx).palette
}

fn installed(cx: &App) -> &Installed {
    cx.try_global::<Installed>().unwrap_or(&FALLBACK)
}

/// Fonts and shape. Written into the light and dark theme configs too, so a reload of
/// either keeps them.
fn apply_metrics(theme: &mut Theme) {
    let with_metrics = |config: &Rc<ThemeConfig>| {
        let mut config = (**config).clone();
        config.font_family = Some(family::INTERFACE.into());
        config.mono_font_family = Some(family::MONO.into());
        config.mono_font_size = Some(text::CODE.size.into());
        config.radius = Some(0);
        config.radius_lg = Some(0);
        Rc::new(config)
    };
    theme.dark_theme = with_metrics(&theme.dark_theme);
    theme.light_theme = with_metrics(&theme.light_theme);

    theme.font_family = family::INTERFACE.into();
    theme.mono_font_family = family::MONO.into();
    theme.mono_font_size = text::CODE.size;
    theme.radius = cut::RADIUS;
    theme.radius_lg = cut::RADIUS;
    theme.shadow = false;
}

/// Lightness offset between a semantic fill and its hover state.
const HOVER_LIGHTNESS_STEP: f32 = 0.05;
/// Lightness offset between a semantic fill and its pressed state.
const ACTIVE_LIGHTNESS_STEP: f32 = -0.06;
/// Share of the distance to white used for the `*_light` base colors.
const LIGHT_VARIANT_MIX: f32 = 0.35;
/// Scrollbar thumb opacity at rest and under the pointer.
const SCROLLBAR_THUMB_ALPHA: (f32, f32) = (0.30, 0.50);

fn shift_lightness(color: Hsla, amount: f32) -> Hsla {
    Hsla {
        l: (color.l + amount).clamp(0.0, 1.0),
        ..color
    }
}

fn lighter(color: Hsla) -> Hsla {
    Hsla {
        l: color.l + (1.0 - color.l) * LIGHT_VARIANT_MIX,
        ..color
    }
}

fn with_alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a, ..color }
}

/// Maps the Bolt roles onto every gpui-component color. `Theme::update` then brings
/// `Theme::tokens` in line with these colors.
fn apply_colors(theme: &mut Theme, colors: &Colors, brand: &Brand, washes: &AccentWashes) {
    let c = colors;
    let t = &mut theme.colors;

    t.background = c.background;
    t.foreground = c.foreground;
    t.border = c.line;
    t.window_border = c.line;
    t.caret = brand.accent_tint;
    t.ring = brand.accent_tint;
    t.selection = washes.text_selection;
    t.input = c.line_2;
    t.muted = c.line_2;
    t.muted_foreground = c.muted;
    t.accent = c.hover;
    t.accent_foreground = c.strong;
    t.overlay = c.scrim;
    t.popover = c.panel;
    t.popover_foreground = c.foreground;

    t.primary = brand.accent;
    t.primary_hover = brand.accent_hover;
    t.primary_active = brand.accent_pressed;
    t.primary_foreground = brand.on_accent;
    t.secondary = c.raised;
    t.secondary_hover = c.line;
    t.secondary_active = c.line_2;
    t.secondary_foreground = c.foreground;

    for (fill, hover, active, foreground, color) in [
        (
            &mut t.danger,
            &mut t.danger_hover,
            &mut t.danger_active,
            &mut t.danger_foreground,
            c.danger,
        ),
        (
            &mut t.success,
            &mut t.success_hover,
            &mut t.success_active,
            &mut t.success_foreground,
            c.success,
        ),
        (
            &mut t.warning,
            &mut t.warning_hover,
            &mut t.warning_active,
            &mut t.warning_foreground,
            c.warning,
        ),
        (
            &mut t.info,
            &mut t.info_hover,
            &mut t.info_active,
            &mut t.info_foreground,
            c.info,
        ),
    ] {
        *fill = color;
        *hover = shift_lightness(color, HOVER_LIGHTNESS_STEP);
        *active = shift_lightness(color, ACTIVE_LIGHTNESS_STEP);
        *foreground = c.on_semantic;
    }

    // Default buttons are secondary; every family follows its semantic color.
    t.button = t.secondary;
    t.button_hover = t.secondary_hover;
    t.button_active = t.secondary_active;
    t.button_foreground = t.foreground;
    t.button_primary = t.primary;
    t.button_primary_hover = t.primary_hover;
    t.button_primary_active = t.primary_active;
    t.button_primary_foreground = t.primary_foreground;
    t.button_secondary = t.secondary;
    t.button_secondary_hover = t.secondary_hover;
    t.button_secondary_active = t.secondary_active;
    t.button_secondary_foreground = t.secondary_foreground;
    t.button_danger = t.danger;
    t.button_danger_hover = t.danger_hover;
    t.button_danger_active = t.danger_active;
    t.button_danger_foreground = t.danger_foreground;
    t.button_success = t.success;
    t.button_success_hover = t.success_hover;
    t.button_success_active = t.success_active;
    t.button_success_foreground = t.success_foreground;
    t.button_warning = t.warning;
    t.button_warning_hover = t.warning_hover;
    t.button_warning_active = t.warning_active;
    t.button_warning_foreground = t.warning_foreground;
    t.button_info = t.info;
    t.button_info_hover = t.info_hover;
    t.button_info_active = t.info_active;
    t.button_info_foreground = t.info_foreground;

    t.scrollbar = gpui::transparent_black();
    t.scrollbar_thumb = with_alpha(c.muted, SCROLLBAR_THUMB_ALPHA.0);
    t.scrollbar_thumb_hover = with_alpha(c.muted, SCROLLBAR_THUMB_ALPHA.1);

    t.title_bar = c.desk;
    t.title_bar_border = gpui::transparent_black();
    t.status_bar = c.desk;
    t.status_bar_border = gpui::transparent_black();

    t.sidebar = c.panel;
    t.sidebar_foreground = c.foreground;
    t.sidebar_border = c.line;
    t.sidebar_accent = washes.selected_item;
    t.sidebar_accent_foreground = c.strong;
    t.sidebar_primary = brand.accent_tint;
    t.sidebar_primary_foreground = brand.on_accent;

    t.tab = c.panel;
    t.tab_bar = c.panel;
    t.tab_foreground = c.muted;
    t.tab_active = c.panel;
    t.tab_active_foreground = c.strong;
    t.tab_bar_segmented = c.raised;

    t.table = c.panel;
    t.table_head = c.panel;
    t.table_head_foreground = c.muted;
    t.table_foot = c.panel;
    t.table_foot_foreground = c.muted;
    t.table_even = c.alternating_row;
    t.table_hover = c.hover;
    t.table_active = washes.selected_row;
    t.table_active_border = brand.accent_tint;
    t.table_row_border = c.row_divider;

    t.list = c.panel;
    t.list_head = c.panel;
    t.list_even = c.alternating_row;
    t.list_hover = c.hover;
    t.list_active = washes.selected_item;
    // The selected row carries a tint bar, not an outline.
    t.list_active_border = gpui::transparent_black();

    t.accordion = c.panel;
    t.group_box = c.panel;
    t.group_box_foreground = c.foreground;
    t.description_list_label = c.panel;
    t.description_list_label_foreground = c.muted;
    t.skeleton = c.raised;

    t.link = c.info;
    t.link_hover = shift_lightness(c.info, HOVER_LIGHTNESS_STEP);
    t.link_active = shift_lightness(c.info, ACTIVE_LIGHTNESS_STEP);

    t.switch = c.line_2;
    t.switch_thumb = c.strong;
    t.slider_bar = c.line_2;
    t.slider_thumb = brand.accent_tint;
    t.progress_bar = brand.accent_tint;
    t.drag_border = brand.accent_tint;
    t.drop_target = washes.drop_target;

    t.chart_1 = brand.accent_tint;
    t.chart_2 = c.info;
    t.chart_3 = c.success;
    t.chart_4 = c.warning;
    t.chart_5 = c.danger;
    t.chart_bullish = c.success;
    t.chart_bearish = c.danger;
    t.chart_grid = c.line;

    t.red = c.danger;
    t.red_light = lighter(c.danger);
    t.green = c.success;
    t.green_light = lighter(c.success);
    t.blue = c.info;
    t.blue_light = lighter(c.info);
    t.yellow = c.warning;
    t.yellow_light = lighter(c.warning);
    t.magenta = brand.accent_tint;
    t.magenta_light = lighter(brand.accent_tint);
    t.cyan = c.info;
    t.cyan_light = lighter(c.info);
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
