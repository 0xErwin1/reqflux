//! ReqFlux's own tokens, on top of Bolt: method labels, the draft marker, shell
//! sizes and the main window size. The only `reqflux_ui` file besides `brand.rs` that holds raw values.

use std::time::Duration;

#[cfg(test)]
use bolt::Colors;
use bolt::theme::rgb_to_hsla as hex;
use gpui::{App, Hsla, Pixels, Size, px, size};

/// The palette `bolt` draws: dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Palette {
    Dark,
    Light,
}

impl Palette {
    /// `bolt`'s color roles of this palette.
    #[cfg(test)]
    pub(crate) fn colors(self) -> Colors {
        match self {
            Self::Dark => Colors::dark(),
            Self::Light => Colors::light(),
        }
    }

    /// The palette `bolt` has installed.
    pub(crate) fn current(cx: &App) -> Self {
        match bolt::palette::resolved_mode(cx) {
            bolt::palette::PaletteMode::Light => Self::Light,
            bolt::palette::PaletteMode::Dark | bolt::palette::PaletteMode::System => Self::Dark,
        }
    }
}

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
    let palette = Palette::current(cx);
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

/// The width of the variable popover (variable-popover/01).
pub(crate) const VARIABLE_POPOVER_WIDTH: Pixels = px(340.0);

/// The tallest the "Add to" list of the variable popover grows before it scrolls.
pub(crate) const VARIABLE_POPOVER_TARGET_LIST_HEIGHT: Pixels = px(160.0);

/// The width of the Auth pane type menu.
pub(crate) const AUTH_MENU_WIDTH: Pixels = px(240.0);

/// The height of the document path bar under the tab row.
pub(crate) const PATH_BAR: Pixels = px(28.0);

/// The side of the chevron between path bar segments.
pub(crate) const PATH_SEPARATOR: Pixels = px(10.0);

/// The height of the request line: method, URL and Send.
pub(crate) const REQUEST_LINE: Pixels = px(48.0);

/// The width of the request-as-written side of the example tab (design/examples/01).
pub(crate) const EXAMPLE_REQUEST_COLUMN: Pixels = px(430.0);

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

/// The wash of the mark whose variable popover is open (variable-popover/01 `.var.hot`).
pub(crate) fn variable_hot_wash(color: Hsla) -> Hsla {
    wash(color, VARIABLE_WASH * 2.0)
}

/// The grid columns: key and value; the description fills the rest.
pub(crate) const GRID_KEY_COLUMN: Pixels = px(200.0);

/// The response cookie table columns (response-views/02): name, value, domain, path and
/// expires; the flags fill the rest.
pub(crate) const COOKIE_NAME_COLUMN: Pixels = px(200.0);
pub(crate) const COOKIE_VALUE_COLUMN: Pixels = px(190.0);
pub(crate) const COOKIE_DOMAIN_COLUMN: Pixels = px(200.0);
pub(crate) const COOKIE_PATH_COLUMN: Pixels = px(50.0);
pub(crate) const COOKIE_EXPIRES_COLUMN: Pixels = px(150.0);
/// The narrowest the flags column, the one that fills the rest, gets before the table
/// scrolls sideways.
pub(crate) const COOKIE_FLAGS_MIN_COLUMN: Pixels = px(120.0);

/// The filter bar over the response's Tests and Console panes (response-views/02): its
/// height, side padding and gap.
pub(crate) const RESULTS_BAR_HEIGHT: Pixels = px(36.0);
pub(crate) const RESULTS_BAR_PADDING_X: Pixels = px(14.0);
pub(crate) const RESULTS_BAR_GAP: Pixels = px(10.0);

/// The wash behind a search match in the response body other than the current one: the
/// color at `VARIABLE_WASH`, as the artboard draws both.
pub(crate) fn search_wash(color: Hsla) -> Hsla {
    wash(color, VARIABLE_WASH)
}

/// How narrow the response pane gets before the Copy, Save as example and Export buttons of
/// its bottom bar shrink to icons with a tooltip (response-views/04).
pub(crate) const RESPONSE_COMPACT_WIDTH: Pixels = px(360.0);

/// The variables grid columns of the workspace settings page (variables/01): row number,
/// enable box, key, initial value, current value and where the value is stored; the
/// description fills the rest.
pub(crate) const VARIABLES_NUMBER_COLUMN: Pixels = px(24.0);
pub(crate) const VARIABLES_ON_COLUMN: Pixels = px(34.0);
pub(crate) const VARIABLES_KEY_COLUMN: Pixels = px(140.0);
pub(crate) const VARIABLES_VALUE_COLUMN: Pixels = px(220.0);
pub(crate) const VARIABLES_CURRENT_COLUMN: Pixels = px(150.0);
pub(crate) const VARIABLES_STORED_COLUMN: Pixels = px(120.0);
/// The narrowest the description column, the one that fills the rest, gets before the grid
/// scrolls sideways.
pub(crate) const VARIABLES_DESCRIPTION_MIN_COLUMN: Pixels = px(160.0);
/// The delete control at the end of a declared row: its icon button and the padding around it.
pub(crate) const VARIABLES_DELETE_COLUMN: Pixels = px(32.0);
/// The width of the `stored in` menu, which fits each option's one-line explanation.
pub(crate) const VARIABLES_STORED_MENU_WIDTH: Pixels = px(480.0);
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

/// The narrowest the text column of a settings section header gets before its buttons wrap
/// onto a second line.
pub(crate) const SETTINGS_HEADER_TEXT_MIN: Pixels = px(240.0);
/// The narrowest a column of the Network section gets before the columns stack.
pub(crate) const NETWORK_COLUMN_MIN: Pixels = px(340.0);

/// The label column of an App > General row (app-settings/02 `.fr`).
pub(crate) const APP_SETTINGS_LABEL_WIDTH: Pixels = px(240.0);
/// The label column of an MCP policy rule row.
pub(crate) const APP_SETTINGS_RULE_LABEL_WIDTH: Pixels = px(150.0);
/// The width of the interface font size select.
pub(crate) const APP_SETTINGS_FONT_SIZE_WIDTH: Pixels = px(120.0);
/// What a control that does not write is drawn at while `config.toml` does not parse
/// (bolt gap: `SegmentedControl` has no disabled state).
pub(crate) const READ_ONLY_OPACITY: f32 = 0.45;
/// The gap between the two columns of MCP policy.
pub(crate) const APP_SETTINGS_COLUMN_GAP: Pixels = px(36.0);
/// The workspace column of the policy overrides table.
pub(crate) const APP_SETTINGS_OVERRIDE_WORKSPACE: Pixels = px(150.0);
/// Each rule column of the policy overrides table.
pub(crate) const APP_SETTINGS_OVERRIDE_RULE: Pixels = px(70.0);
/// The narrowest the lists column, the one that fills the rest, gets before the overrides
/// table scrolls sideways.
pub(crate) const APP_SETTINGS_OVERRIDE_LISTS_MIN: Pixels = px(140.0);

/// The key column of a Keymap row (design/layouts/01 D3 `.krow`).
pub(crate) const KEYMAP_KEY_WIDTH: Pixels = px(240.0);

/// The opacity of the danger wash on a grid row with an unresolved reference.
const UNRESOLVED_ROW_WASH: f32 = 0.05;

/// The wash of a grid row with an unresolved reference, from the palette's `danger`.
pub(crate) fn unresolved_wash(danger: Hsla) -> Hsla {
    wash(danger, UNRESOLVED_ROW_WASH)
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
/// The narrowest the value column gets: a pane too narrow for the columns around it scrolls
/// the grid sideways instead.
pub(crate) const GRID_VALUE_MIN_COLUMN: Pixels = px(160.0);

/// The width of the Settings pane's timeout field (request-panes/01 frame C).
pub(crate) const SETTING_NUMBER_FIELD: Pixels = px(160.0);

/// The status column of a History panel row (history/01).
pub(crate) const HISTORY_STATUS_COLUMN: Pixels = px(26.0);

/// The audit table's columns (bolt-v2/13); bolt gap: `DocumentMetrics` has no audit column
/// widths. The target column fills the rest.
pub(crate) const AUDIT_CHEVRON_COLUMN: Pixels = px(24.0);
pub(crate) const AUDIT_TIME_COLUMN: Pixels = px(110.0);
pub(crate) const AUDIT_CLASS_COLUMN: Pixels = px(100.0);
pub(crate) const AUDIT_TOOL_COLUMN: Pixels = px(190.0);
pub(crate) const AUDIT_WORKSPACE_COLUMN: Pixels = px(130.0);
pub(crate) const AUDIT_OUTCOME_COLUMN: Pixels = px(110.0);
/// The narrowest the target column, the one that fills the rest, gets before the table
/// scrolls sideways.
pub(crate) const AUDIT_TARGET_MIN_COLUMN: Pixels = px(200.0);

/// The width of the workspace menu's `New workspace` name popover (workspace-menu/01).
pub(crate) const NEW_WORKSPACE_POPOVER_WIDTH: Pixels = px(340.0);

/// The run document (design/runner/01-run-document): the width of the setup's select fields
/// and its environment menu, the run order column, and the result's tests column.
pub(crate) const RUNNER_FIELD_WIDTH: Pixels = px(360.0);
pub(crate) const RUNNER_ORDER_WIDTH: Pixels = px(340.0);
pub(crate) const RUNNER_TESTS_WIDTH: Pixels = px(420.0);
/// The run results grid's columns after the row number, as the header names them; bolt gap:
/// `GridMetrics` has no run results column widths.
pub(crate) const RUNNER_COLUMNS: [(&str, Pixels); 7] = [
    ("method", px(80.0)),
    ("request", px(240.0)),
    ("folder", px(140.0)),
    ("status", px(170.0)),
    ("time", px(90.0)),
    ("size", px(90.0)),
    ("tests", px(180.0)),
];

/// The width of the Generate code dialog (codegen/01).
pub(crate) const CODEGEN_DIALOG_WIDTH: Pixels = px(880.0);

/// The width of the Export… dialog (export/01).
pub(crate) const EXPORT_DIALOG_WIDTH: Pixels = px(620.0);

/// The width of the Postman import dialog (bolt-v2/11).
pub(crate) const IMPORT_DIALOG_WIDTH: Pixels = px(1000.0);

/// The width of the import dialog's Files column; the counts take the rest (bolt-v2/11).
pub(crate) const IMPORT_FILES_WIDTH: Pixels = px(430.0);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
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
}
