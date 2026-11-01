//! ReqFlux's own tokens, on top of Bolt: method labels, the draft marker, syntax colors, shell
//! sizes and the main window size. The only `reqflux_ui` file besides `brand.rs` that holds raw values.

use bolt_legacy::Palette;
use bolt_legacy::tokens::hex;
use std::time::Duration;

use gpui::{App, Hsla, Pixels, Size, px, size};

/// The label color of the uppercase HTTP `verb` on `palette`: Bolt's semantic hues, as DBFlux
/// colors its object kinds. GET success, POST warning, PUT info, PATCH the null purple, DELETE
/// danger, and HEAD, OPTIONS and custom verbs cyan. Screens draw [`method_hue`]; this pins
/// the hues by palette for the tests.
#[cfg(test)]
pub(crate) fn method_color(verb: &str, palette: Palette) -> Hsla {
    let colors = palette.colors();
    match verb {
        "GET" => colors.success,
        "POST" => colors.warning,
        "PUT" => colors.info,
        "PATCH" => method_patch(palette),
        "DELETE" => colors.danger,
        _ => method_other(palette),
    }
}

/// The label color of `verb` on the palette `bolt` has installed.
pub(crate) fn method_hue(verb: &str, cx: &App) -> Hsla {
    let colors = bolt::colors(cx);
    let palette = bolt_palette(cx);
    match verb {
        "GET" => colors.success,
        "POST" => colors.warning,
        "PUT" => colors.info,
        "PATCH" => method_patch(palette),
        "DELETE" => colors.danger,
        _ => method_other(palette),
    }
}

/// Whether `bolt` draws dark or light.
fn bolt_palette(cx: &App) -> Palette {
    match bolt::palette::resolved_mode(cx) {
        bolt::palette::PaletteMode::Light => Palette::Light,
        bolt::palette::PaletteMode::Dark | bolt::palette::PaletteMode::System => Palette::Dark,
    }
}

/// The PATCH label: the purple Bolt gives `null` in data views, deeper on the light ground.
fn method_patch(palette: Palette) -> Hsla {
    match palette {
        Palette::Dark => hex(0xB79CFF),
        Palette::Light => hex(0x7445D6),
    }
}

/// HEAD, OPTIONS and custom verbs: cyan, deeper on the light ground.
fn method_other(palette: Palette) -> Hsla {
    match palette {
        Palette::Dark => hex(0x6FD3D8),
        Palette::Light => hex(0x0E7C83),
    }
}

/// Body syntax colors: quiet keys, values carry the data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SyntaxColors {
    pub(crate) key: Hsla,
    pub(crate) string: Hsla,
    /// Numbers, booleans and null.
    pub(crate) number: Hsla,
    pub(crate) punct: Hsla,
}

impl SyntaxColors {
    /// The syntax colors on `palette`: strings take its success color, punctuation its
    /// muted text.
    pub(crate) fn for_palette(palette: Palette) -> Self {
        let colors = palette.colors();
        match palette {
            Palette::Dark => Self {
                key: hex(0xA9A4B2),
                string: colors.success,
                number: method_patch(palette),
                punct: colors.muted,
            },
            Palette::Light => Self {
                key: hex(0x5C5766),
                string: colors.success,
                number: method_patch(palette),
                punct: colors.muted,
            },
        }
    }

    /// The syntax colors on the palette `bolt` has installed.
    pub(crate) fn current(cx: &App) -> Self {
        let palette = bolt_palette(cx);
        let colors = bolt::colors(cx);
        Self {
            string: colors.success,
            punct: colors.muted,
            ..Self::for_palette(palette)
        }
    }
}

/// The width of the method column of a tree row: `OPTIONS`, the longest standard verb, in
/// JetBrains Mono at `METHOD_LABEL_SIZE` (about 42 px), plus room before the name.
pub(crate) const TREE_METHOD_COLUMN: Pixels = px(46.0);

/// The size of a method label in a tree row or tab.
pub(crate) const METHOD_LABEL_SIZE: Pixels = px(10.0);

/// The tallest the command palette's list grows before it scrolls.
pub(crate) const PALETTE_LIST_HEIGHT: Pixels = px(432.0);

/// The variables table of an approval: its variable and source columns.
pub(crate) const APPROVAL_VARIABLE_COLUMN: Pixels = px(160.0);
pub(crate) const APPROVAL_SOURCE_COLUMN: Pixels = px(120.0);

/// The width of the title bar environment menu.
pub(crate) const ENVIRONMENT_MENU_WIDTH: Pixels = px(360.0);

/// The width of the title bar workspace menu.
pub(crate) const WORKSPACE_MENU_WIDTH: Pixels = px(420.0);

/// The width of the recent list and actions of the empty state.
pub(crate) const EMPTY_STATE_WIDTH: Pixels = px(420.0);

/// The width of the Auth pane type menu.
pub(crate) const AUTH_MENU_WIDTH: Pixels = px(240.0);

/// The height of the document path bar under the tab row.
pub(crate) const PATH_BAR: Pixels = px(28.0);

/// The side of the chevron between path bar segments.
pub(crate) const PATH_SEPARATOR: Pixels = px(10.0);

/// The height of the request line: method, URL and Send.
pub(crate) const REQUEST_LINE: Pixels = px(48.0);

/// The width of the method chip on the request line.
pub(crate) const METHOD_CHIP_WIDTH: Pixels = px(96.0);

/// The side of the draft marker dot on a document tab.
pub(crate) const DIRTY_DOT: Pixels = px(5.0);

/// The opacity of the wash behind a `{{name}}` reference in the URL.
pub(crate) const VARIABLE_WASH: f32 = 0.12;

/// The wash behind a `{{name}}` reference: the color at `VARIABLE_WASH`.
pub(crate) fn variable_wash(color: Hsla) -> Hsla {
    wash(color, VARIABLE_WASH)
}

/// The grid columns: key and value; the description fills the rest.
pub(crate) const GRID_KEY_COLUMN: Pixels = px(200.0);
pub(crate) const GRID_VALUE_COLUMN: Pixels = px(420.0);

/// The variables grid columns of the workspace settings page (variables/01): row number,
/// enable box, key, initial value, current value and where the value is stored; the
/// description fills the rest.
pub(crate) const VARIABLES_NUMBER_COLUMN: Pixels = px(24.0);
pub(crate) const VARIABLES_ON_COLUMN: Pixels = px(34.0);
pub(crate) const VARIABLES_KEY_COLUMN: Pixels = px(140.0);
pub(crate) const VARIABLES_VALUE_COLUMN: Pixels = px(220.0);
pub(crate) const VARIABLES_CURRENT_COLUMN: Pixels = px(150.0);
pub(crate) const VARIABLES_STORED_COLUMN: Pixels = px(92.0);
/// The width of the `Set by` column of an undeclared row, which the page leaves empty.
pub(crate) const VARIABLES_UNDECLARED_NOTE: Pixels = px(118.0);
/// The padding of a narrow grid cell (the enable box column).
pub(crate) const VARIABLES_NARROW_PADDING: Pixels = px(6.0);
/// The opacity of the key and values of a disabled variable.
pub(crate) const VARIABLES_DISABLED_OPACITY: f32 = 0.55;
/// The horizontal padding of the variables column of the page.
pub(crate) const VARIABLES_PANEL_INSET: Pixels = px(20.0);

/// The workspace settings page (variables/01): the section nav and the environment list.
pub(crate) const SETTINGS_NAV_WIDTH: Pixels = px(230.0);
pub(crate) const SETTINGS_ENVIRONMENT_LIST: Pixels = px(244.0);

/// The opacity of the warning wash on a grid row with an unresolved reference.
const UNRESOLVED_ROW_WASH: f32 = 0.05;

/// The wash of a grid row with an unresolved reference, from the palette's `warning`.
pub(crate) fn unresolved_wash(warning: Hsla) -> Hsla {
    wash(warning, UNRESOLVED_ROW_WASH)
}

/// The height of the indeterminate progress bar under the tab row while sending.
pub(crate) const RESPONSE_PROGRESS: Pixels = px(4.0);

/// The width of the moving progress segment, as a fraction of the track.
pub(crate) const RESPONSE_PROGRESS_SEGMENT: f32 = 0.22;

/// How long the progress segment takes to cross the track.
pub(crate) const RESPONSE_PROGRESS_PERIOD: Duration = Duration::from_millis(1400);

/// The opacity of the previous response while a new send is in flight.
pub(crate) const RESPONSE_DIMMED: f32 = 0.62;

/// The width of the Problems panel (status-bar/02); the rest of it is DBFlux's notifications
/// popover (`bolt::tokens::NotificationMetrics`).
pub(crate) const PROBLEMS_PANEL_WIDTH: Pixels = px(540.0);

/// `color` at opacity `alpha`.
pub(crate) fn wash(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// A previewed image's natural size, one logical pixel per image pixel: a size the
/// response data sets, not a design value.
pub(crate) fn image_size(width: u32, height: u32) -> Size<Pixels> {
    #[expect(clippy::cast_precision_loss, reason = "a pixel size shown on screen")]
    size(px(width as f32), px(height as f32))
}

/// The size the main window opens at.
pub(crate) const MAIN_WINDOW_SIZE: Size<Pixels> = size(px(1200.0), px(800.0));

/// How far the rename hint under a tree row starts past the row's indent
/// (design/tree-editing/01-context-menu-modal.html).
pub(crate) const TREE_HINT_OFFSET: Pixels = px(20.0);
/// The opacity of the tree row being dragged.
pub(crate) const TREE_DRAG_SOURCE_OPACITY: f32 = 0.4;
/// The distance from the window top to the tree's delete dialog; the dialog itself is
/// `bolt`'s modal.
pub(crate) const TREE_DIALOG_TOP: Pixels = px(250.0);

/// The width Send and Cancel share, so the request line keeps its layout while a send is in
/// flight: the wider of the two, Send with its icon and `Ctrl ↵` keycap, with room to spare.
pub(crate) const SEND_BUTTON_WIDTH: Pixels = px(140.0);

/// The request/response split (request-panes/01): the least height of each pane and the
/// request pane's height before the user moves it. The gutter is `bolt`'s splitter.
pub(crate) const SPLIT_PANE_MIN: Pixels = px(120.0);
pub(crate) const SPLIT_DEFAULT_REQUEST: Pixels = px(306.0);

/// The Params and Headers grid columns (request-panes/01) `bolt`'s data grid has no
/// equivalent for: the enable, key, description and actions columns (the value fills the
/// rest). Rows, header, cell padding and text are `bolt::tokens::GridMetrics`.
pub(crate) const GRID_ENABLE_COLUMN: Pixels = px(36.0);
pub(crate) const GRID_CELL_KEY_COLUMN: Pixels = px(210.0);
pub(crate) const GRID_DESCRIPTION_COLUMN: Pixels = px(230.0);
pub(crate) const GRID_ACTIONS_COLUMN: Pixels = px(34.0);

/// The width of the Settings pane's timeout field (request-panes/01 frame C).
pub(crate) const SETTING_NUMBER_FIELD: Pixels = px(160.0);

/// The status column of a History panel row (history/01).
pub(crate) const HISTORY_STATUS_COLUMN: Pixels = px(26.0);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bolt_legacy::Palette;
    use bolt_legacy::tokens::Colors;
    use gpui::Rgba;

    /// `0xRRGGBB` of an opaque `color`.
    pub(crate) fn rgb_of(color: Hsla) -> u32 {
        let rgba = Rgba::from(color);
        assert_eq!(rgba.a, 1.0);
        let channel = |value: f32| (value * 255.0).round() as u32;
        (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b)
    }

    #[test]
    fn methods_use_the_bolt_semantic_hues() {
        let colors = Colors::dark();
        let palette = Palette::Dark;
        let pairs = [
            ("GET", 0x7BE0A0),
            ("POST", 0xFFC23D),
            ("PUT", 0x6EA8FF),
            ("PATCH", 0xB79CFF),
            ("DELETE", 0xFF6B5E),
        ];
        for (verb, rgb) in pairs {
            assert_eq!(rgb_of(method_color(verb, palette)), rgb, "{verb}");
        }
        assert_eq!(method_color("GET", palette), colors.success);
        assert_eq!(method_color("DELETE", palette), colors.danger);
    }

    #[test]
    fn other_verbs_are_cyan() {
        for verb in ["HEAD", "OPTIONS", "PURGE"] {
            assert_eq!(
                rgb_of(method_color(verb, Palette::Dark)),
                0x6FD3D8,
                "{verb}"
            );
        }
    }

    #[test]
    fn syntax_colors_match_the_design_system() {
        let syntax = SyntaxColors::for_palette(Palette::Dark);
        assert_eq!(rgb_of(syntax.key), 0xA9A4B2);
        assert_eq!(rgb_of(syntax.string), 0x7BE0A0);
        assert_eq!(rgb_of(syntax.number), 0xB79CFF);
        assert_eq!(rgb_of(syntax.punct), 0x8E8996);
    }

    #[test]
    fn light_methods_use_the_light_semantics_and_deeper_product_hues() {
        let colors = Colors::light();
        let palette = Palette::Light;
        assert_eq!(method_color("GET", palette), colors.success);
        assert_eq!(method_color("POST", palette), colors.warning);
        assert_eq!(method_color("PUT", palette), colors.info);
        assert_eq!(method_color("DELETE", palette), colors.danger);
        assert_eq!(rgb_of(method_color("PATCH", palette)), 0x7445D6);
        assert_eq!(rgb_of(method_color("OPTIONS", palette)), 0x0E7C83);
        for verb in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] {
            assert_ne!(
                method_color(verb, Palette::Dark),
                method_color(verb, palette),
                "{verb}"
            );
        }
    }

    #[test]
    fn light_syntax_colors_read_on_the_light_panel() {
        let syntax = SyntaxColors::for_palette(Palette::Light);
        assert_eq!(rgb_of(syntax.key), 0x5C5766);
        assert_eq!(rgb_of(syntax.string), 0x1C7A45);
        assert_eq!(rgb_of(syntax.number), 0x7445D6);
        assert_eq!(rgb_of(syntax.punct), 0x6B6673);
    }
}
