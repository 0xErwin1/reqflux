//! ReqFlux's own tokens, on top of Bolt: method labels, the draft marker, syntax colors, shell
//! sizes and the main window size. The only `reqflux_ui` file besides `brand.rs` that holds raw values.

use bolt::Palette;
use bolt::tokens::{Colors, hex, hex_alpha};
use std::time::Duration;

use gpui::{Hsla, Pixels, Size, px, size};

/// The label color of the uppercase HTTP `verb` on `palette`: Bolt's semantic hues, as DBFlux
/// colors its object kinds. GET success, POST warning, PUT info, PATCH the null purple, DELETE
/// danger, and HEAD, OPTIONS and custom verbs cyan.
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
}

/// The width of the method column of a tree row: `OPTIONS`, the longest standard verb, in
/// JetBrains Mono at `METHOD_LABEL_SIZE` (about 42 px), plus room before the name.
pub(crate) const TREE_METHOD_COLUMN: Pixels = px(46.0);

/// The size of a method label in a tree row or tab.
pub(crate) const METHOD_LABEL_SIZE: Pixels = px(10.0);

/// The size of the child count on the right of a collection or folder row.
pub(crate) const TREE_COUNT_SIZE: Pixels = px(11.0);

/// The gap between the parts of a tree row.
pub(crate) const TREE_ROW_GAP: Pixels = px(7.0);

/// Where the guide line of the first indent level sits from the left edge of a tree row; each
/// deeper level adds `chrome::TREE_INDENT`.
pub(crate) const TREE_GUIDE_OFFSET: Pixels = px(19.0);

/// The height of the sidebar header that holds `COLLECTIONS` and the new-request button.
pub(crate) const SIDEBAR_HEADER: Pixels = px(46.0);

/// The bottom padding under the sidebar filter field.
pub(crate) const SIDEBAR_FILTER_BOTTOM: Pixels = px(10.0);

/// The height of the sidebar footer with the request and collection counts.
pub(crate) const SIDEBAR_FOOTER: Pixels = px(40.0);

/// The width of the command palette.
pub(crate) const PALETTE_WIDTH: Pixels = px(560.0);

/// The tallest the command palette's list grows before it scrolls.
pub(crate) const PALETTE_LIST_HEIGHT: Pixels = px(432.0);

/// The width of the label column of the MCP approvals detail.
pub(crate) const APPROVAL_LABEL: Pixels = px(96.0);

/// The height of a button of the MCP approvals detail.
pub(crate) const APPROVAL_BUTTON: Pixels = px(32.0);

/// The height of the reason field of the MCP approvals detail.
pub(crate) const APPROVAL_REASON: Pixels = px(36.0);

/// The width of the command search field at the center of the title bar.
pub(crate) const COMMAND_SEARCH_WIDTH: Pixels = px(420.0);

/// The side of the environment diamond in the title bar.
pub(crate) const ENVIRONMENT_DIAMOND: Pixels = px(7.0);

/// The width of the title bar environment menu.
pub(crate) const ENVIRONMENT_MENU_WIDTH: Pixels = px(360.0);

/// The height of a row of the environment menu.
pub(crate) const ENVIRONMENT_OPTION: Pixels = px(36.0);

/// The width of the title bar workspace menu.
pub(crate) const WORKSPACE_MENU_WIDTH: Pixels = px(420.0);

/// The height of a recent workspace row, name over path, in the workspace menu and the
/// empty state.
pub(crate) const WORKSPACE_RECENT_ROW: Pixels = px(48.0);

/// The width of the recent list and actions of the empty state.
pub(crate) const EMPTY_STATE_WIDTH: Pixels = px(420.0);

/// The width of the Auth pane type menu.
pub(crate) const AUTH_MENU_WIDTH: Pixels = px(240.0);

/// The height of a row of the Auth pane type menu.
pub(crate) const AUTH_OPTION: Pixels = px(32.0);

/// The height of the document path bar under the tab row.
pub(crate) const PATH_BAR: Pixels = px(28.0);

/// The horizontal padding of the document path bar.
pub(crate) const PATH_BAR_PADDING_X: Pixels = px(14.0);

/// The side of the chevron between path bar segments.
pub(crate) const PATH_SEPARATOR: Pixels = px(10.0);

/// The height of the request line: method, URL and Send.
pub(crate) const REQUEST_LINE: Pixels = px(48.0);

/// The width of the method chip on the request line.
pub(crate) const METHOD_CHIP_WIDTH: Pixels = px(96.0);

/// The height of the URL field and the Send button.
pub(crate) const URL_FIELD: Pixels = px(32.0);

/// The side of the draft marker dot on a document tab.
pub(crate) const DIRTY_DOT: Pixels = px(5.0);

/// The width of the options part of the Send split button.
pub(crate) const SEND_OPTIONS_WIDTH: Pixels = px(28.0);

/// The text size of the method chip and the Send button.
pub(crate) const REQUEST_LINE_TEXT: Pixels = px(12.5);

/// The text size of the URL.
pub(crate) const URL_TEXT: Pixels = px(13.0);

/// The opacity of the wash behind a `{{name}}` reference in the URL.
pub(crate) const VARIABLE_WASH: f32 = 0.12;

/// The wash behind a `{{name}}` reference: the color at `VARIABLE_WASH`.
pub(crate) fn variable_wash(color: Hsla) -> Hsla {
    wash(color, VARIABLE_WASH)
}

/// The horizontal and vertical padding of the keycap on the Send button.
pub(crate) const KEYCAP_PADDING_X: Pixels = px(6.0);
pub(crate) const KEYCAP_PADDING_Y: Pixels = px(2.0);

/// The text size of keycaps and sub-tab row counts.
pub(crate) const SMALL_MONO_TEXT: Pixels = px(10.5);

/// The keycap fill on the accent: the accent darkened. The accent fill and the text on it are
/// the same on both palettes, so the keycap is too.
pub(crate) fn keycap_on_accent_fill() -> Hsla {
    hex_alpha(0x000000, 0.25)
}

/// The keycap text on the accent.
pub(crate) fn keycap_on_accent_text() -> Hsla {
    hex_alpha(0xFFFFFF, 0.82)
}

/// The height of the request sub-tab bar.
pub(crate) const SUB_TAB_BAR: Pixels = px(42.0);

/// The text size of the unresolved notice at the right of the sub-tab bar.
pub(crate) const NOTICE_TEXT: Pixels = px(12.0);

/// The heights of the Params and Headers grid header and data rows.
pub(crate) const GRID_HEADER: Pixels = px(32.0);
pub(crate) const GRID_ROW: Pixels = px(29.0);

/// The text sizes of the grid header and data rows.
pub(crate) const GRID_HEADER_TEXT: Pixels = px(12.0);
pub(crate) const GRID_TEXT: Pixels = px(12.5);

/// The side of the enable box at the start of a Params or Headers row.
pub(crate) const GRID_ENABLED_BOX: Pixels = px(14.0);

/// The grid columns: row number, key and value; the description fills the rest.
pub(crate) const GRID_NUMBER_COLUMN: Pixels = px(46.0);
pub(crate) const GRID_KEY_COLUMN: Pixels = px(200.0);
pub(crate) const GRID_VALUE_COLUMN: Pixels = px(420.0);

/// The Variables document columns: shared and current value, and the secret toggle; the
/// key uses `GRID_KEY_COLUMN` and the description fills the rest.
pub(crate) const VARIABLES_VALUE_COLUMN: Pixels = px(220.0);
pub(crate) const VARIABLES_SECRET_COLUMN: Pixels = px(64.0);

/// The text and icon size of the SECRET badge.
pub(crate) const BADGE_TEXT: Pixels = px(10.0);

/// The height of a code line in the Body and Scripts panes.
pub(crate) const CODE_LINE: Pixels = px(20.0);

/// The width of the label column of the Auth and Settings rows.
pub(crate) const PANE_LABEL_COLUMN: Pixels = px(160.0);

/// The opacity of the warning wash on a grid row with an unresolved reference.
const UNRESOLVED_ROW_WASH: f32 = 0.05;

/// The wash of a grid row with an unresolved reference: the warning, faint.
pub(crate) fn unresolved_row_wash(colors: &Colors) -> Hsla {
    Hsla {
        a: UNRESOLVED_ROW_WASH,
        ..colors.warning
    }
}

/// The height of the response tab row.
pub(crate) const RESPONSE_TAB_ROW: Pixels = px(46.0);

/// The text size of the response status chip and of the time and size at the right.
pub(crate) const RESPONSE_STATUS_TEXT: Pixels = px(11.0);
pub(crate) const RESPONSE_META_TEXT: Pixels = px(11.5);

/// The opacity of the wash behind the response status chip, in the chip's color.
pub(crate) const RESPONSE_STATUS_WASH: f32 = 0.133;

/// The height of the indeterminate progress bar under the tab row while sending.
pub(crate) const RESPONSE_PROGRESS: Pixels = px(4.0);

/// The width of the moving progress segment, as a fraction of the track.
pub(crate) const RESPONSE_PROGRESS_SEGMENT: f32 = 0.22;

/// How long the progress segment takes to cross the track.
pub(crate) const RESPONSE_PROGRESS_PERIOD: Duration = Duration::from_millis(1400);

/// The opacity of the previous response while a new send is in flight.
pub(crate) const RESPONSE_DIMMED: f32 = 0.62;

/// The height of the error banner, the width of its left edge and its text size.
pub(crate) const RESPONSE_ERROR_BANNER: Pixels = px(44.0);
pub(crate) const RESPONSE_ERROR_EDGE: Pixels = px(3.0);
pub(crate) const RESPONSE_ERROR_TEXT: Pixels = px(13.0);

/// The opacity of the danger wash behind the error banner.
pub(crate) const RESPONSE_ERROR_WASH: f32 = 0.08;

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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bolt::Palette;
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
