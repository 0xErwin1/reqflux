//! The Params and Headers grids as spreadsheet cells (request-panes/01): a header row, then
//! one row per entry with its enable box, the key, the value with its references styled, the
//! description and the delete control, each cell with a line on every side. Disabled rows are
//! muted and struck through, rows with an unresolved reference carry a danger wash, and a
//! value that references a secret shows `••••••••` with a SECRET badge. Clicking a row's
//! enable box toggles the row in the draft; clicking its key, value or description cell turns
//! the cell into a well with a text field. The delete control removes the row, and the add row under
//! the grid appends one; an empty editable grid shows its header and the add row, not an
//! empty state. Rows and the header take the heights and the face of `bolt`'s data
//! grid.

use bolt::controls::{Button, Checkbox, GpuiInput, InputEnter, InputEscape};
use bolt::primitives::{Badge, BadgeTone, Icon};
use bolt::tokens::{Borders, Fields, FontSizes, GridMetrics, Spacing, TabMetrics};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, Hsla, InteractiveElement, ParentElement, Stateful,
    Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::KeyValue;
use reqflux_engine::Inspection;

use super::grid_model::{has_unresolved, masked_text, placeholder, row_id, row_label, value_hint};
use super::{code, spans, ui, variable_popover_view};
use crate::shell::Shell;
use crate::shell::cell_edit::{CellEdit, CellField};
use crate::shell::grid_widths::{self, GridKind, GridWidths};
use crate::shell::rows::{labeled_row, part};
use crate::shell::table_widths;
use crate::tokens;

/// A grid column: fixed width (the key and description widths are the user's, see
/// `GridWidths`), or the value column, which fills the rest.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Enable,
    Key,
    Value,
    Description,
    Actions,
}

impl Column {
    /// A cell of this column.
    fn cell(self, line: Hsla, widths: GridWidths) -> Div {
        self.shape(div(), line, widths)
    }

    /// `element` shaped as a cell of this column: the row's full height, its width, the line
    /// under it and the line on its right for every column but the last; the neighbors draw
    /// the other two sides.
    fn shape<E: Styled>(self, element: E, line: Hsla, widths: GridWidths) -> E {
        let cell = element
            .h_full()
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap();
        let cell = match self {
            Column::Enable => cell
                .w(tokens::GRID_ENABLE_COLUMN)
                .flex_none()
                .justify_center(),
            Column::Key => cell.w(widths.key()).flex_none(),
            Column::Value => cell.flex_1(),
            Column::Description => cell.w(widths.description()).flex_none(),
            Column::Actions => cell
                .w(tokens::GRID_ACTIONS_COLUMN)
                .flex_none()
                .justify_center(),
        };
        let cell = match self {
            Column::Key | Column::Value | Column::Description => cell
                .pl(GridMetrics::CELL_PADDING_X)
                .pr(GridMetrics::CELL_PADDING_X)
                .gap(Spacing::SM),
            Column::Enable | Column::Actions => cell,
        };
        let cell = cell.border_b(Borders::THIN).border_color(line);
        match self {
            Column::Actions => cell,
            _ => cell.border_r(Borders::THIN),
        }
    }
}

/// `element` in the face and size of `bolt`'s data grid.
fn grid_text<E: Styled>(element: E, cx: &App) -> E {
    element
        .font_family(bolt::fonts::grid_family(cx))
        .text_size(bolt::fonts::grid_font_size(cx))
}

/// The grid of `rows`, a `table` of `kind` (`params` and `param`, `headers` and `header`, or
/// `body`). A row whose entry in `files` is true holds a file path and shows a File icon.
/// When `request_id` is `Some`, every row's enable box toggles the row in the request's
/// draft, and its key, value and description cells open the editable cell field; with `None`
/// the grid is read-only.
#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    inspection: &Inspection,
    table: &str,
    kind: &'static str,
    rows: &[KeyValue],
    files: &[bool],
    request_id: Option<&str>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let grid_kind = GridKind::of(table, kind);
    let widths = GridWidths::of(grid_kind, cx);
    let add = request_id.map(|id| add_row(id, table, kind, widths, cx));
    // An editable grid shows its columns and the add row even when empty, so a row is one
    // click away; only a read-only grid has nothing to add and shows the empty state.
    if rows.is_empty() && request_id.is_none() {
        let empty = code::empty(
            "request-grid-empty".to_owned(),
            IconName::List,
            format!("No {table}"),
        );
        return div().flex().flex_col().child(empty).into_any_element();
    }
    let hint = value_hint(shell.environment_name().as_deref());
    let header = header(table, kind, grid_kind, hint, widths, cx);
    // The drag follows the pointer over the whole grid; each move stores the width it asks
    // for in the table store, which every grid of this kind reads. The header and the rows sit
    // in one sideways viewport that takes the height of its own header and rows, since the
    // request pane scrolls as a whole instead of giving the grid a height of its own.
    let rows = div()
        .flex()
        .flex_col()
        .children(rows.iter().enumerate().map(|(index, row)| {
            let file = files.get(index).copied().unwrap_or(false);
            data_row(
                inspection,
                table,
                request_id,
                index + 1,
                row,
                file,
                widths,
                shell,
                cx,
            )
        }))
        .children(add);
    let min_width = widths.min_width();
    let overflow = table_widths::overflow(format!("request-{table}-scroll"), min_width, cx)
        .header(header)
        .rows(rows)
        .content_height()
        .flex_none()
        .w_full();
    table_widths::resizable(
        div().flex().flex_col().flex_none().w_full(),
        grid_kind.table(),
        cx,
    )
    .child(overflow)
    .into_any_element()
}

/// The header row, element id `request-<table>-header`: the column names, the key's `kind`
/// and, when an environment is active, the value's `resolved in <environment>` as
/// `request-<table>-value-hint`. The key and description cells (`request-<table>-header-key`
/// and `-description`, beside `-enable`) carry `bolt`'s resize handle on their right edge.
fn header(
    table: &str,
    kind: &'static str,
    grid_kind: GridKind,
    value_hint: Option<String>,
    widths: GridWidths,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let note = |text: String| {
        div()
            .ml(Spacing::XXS)
            .font_weight(FontWeight::NORMAL)
            .text_size(TabMetrics::RESULT_META_SIZE)
            .text_color(colors.muted)
            .child(text)
    };
    let line = colors.line_2;
    let store = grid_kind.table();
    grid_text(part(format!("request-{table}-header")), cx)
        .flex()
        .items_center()
        .flex_none()
        .h(bolt::fonts::grid_header_height(cx))
        .bg(colors.background)
        .text_size(FontSizes::XS)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(Column::Enable.shape(part(format!("request-{table}-header-enable")), line, widths))
        .child(table_widths::with_handle(
            Column::Key
                .shape(part(format!("request-{table}-header-key")), line, widths)
                .child("key")
                .child(note(kind.to_owned())),
            store,
            grid_widths::KEY,
            tokens::GRID_CELL_KEY_COLUMN,
            cx,
        ))
        .child(
            Column::Value
                .shape(part(format!("request-{table}-header-value")), line, widths)
                .child("value")
                .children(
                    value_hint
                        .map(|hint| part(format!("request-{table}-value-hint")).child(note(hint))),
                ),
        )
        .child(table_widths::with_handle(
            Column::Description
                .shape(
                    part(format!("request-{table}-header-description")),
                    line,
                    widths,
                )
                .child("description"),
            store,
            grid_widths::DESCRIPTION,
            tokens::GRID_DESCRIPTION_COLUMN,
            cx,
        ))
        .child(Column::Actions.cell(line, widths))
}

#[allow(clippy::too_many_arguments)]
fn data_row(
    inspection: &Inspection,
    table: &str,
    request_id: Option<&str>,
    n: usize,
    row: &KeyValue,
    file: bool,
    widths: GridWidths,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    // The box first: it takes the context mutably for its click listener, before the row
    // borrows it for the colors.
    let enable = request_id.map(|id| enable_box(id, table, n, &row.key, row.enabled, cx));
    let delete = request_id.map(|id| delete_control(id, table, n, &row.key, widths, cx));
    let value_spans = inspection.spans(&row.value);
    let unresolved = has_unresolved(&value_spans) || has_unresolved(&inspection.spans(&row.key));
    let secret_value = inspection.references_secret(&row.value);
    let off = !row.enabled;
    let colors = bolt::colors(cx);
    let tone = |on: Hsla| if off { colors.muted } else { on };
    let editing = request_id.and_then(|id| {
        shell.cell_edit.as_ref().and_then(|edit| {
            (edit.request_id == id && edit.table == table && edit.n == n).then_some(edit)
        })
    });

    let key_cell: AnyElement = match editing.filter(|edit| edit.field == CellField::Key) {
        Some(edit) => editing_cell(edit, widths, shell, cx).into_any_element(),
        None => cell_part(request_id, table, n, CellField::Key, widths, cx)
            .text_color(tone(colors.strong))
            .when(off, |this| this.line_through())
            .child(cell_text(table, n, CellField::Key).child(row.key.clone()))
            .into_any_element(),
    };

    let value_cell: AnyElement = match editing.filter(|edit| edit.field == CellField::Value) {
        Some(edit) => editing_cell(edit, widths, shell, cx).into_any_element(),
        None => {
            let value = cell_part(request_id, table, n, CellField::Value, widths, cx)
                .text_color(tone(colors.foreground))
                .when(off, |this| this.line_through())
                .when(file, |this| {
                    this.child(
                        Icon::new(ui::icon(IconName::File))
                            .size(GridMetrics::HEADER_ICON)
                            .color(colors.muted),
                    )
                });
            let text = cell_text(table, n, CellField::Value);
            let value = if secret_value {
                value
                    .child(text.child(masked_text(&value_spans)))
                    .child(part(format!("request-{table}-secret-{n}")).child(secret()))
            } else {
                let spans = match request_id {
                    Some(id) => spans::render_marked(
                        inspection,
                        &row.value,
                        id,
                        &format!("request-{table}-value-{n}"),
                        shell,
                        cx,
                    ),
                    None => spans::render(inspection, &row.value, cx),
                };
                value.child(text.children(spans))
            };
            value.into_any_element()
        }
    };

    let description: AnyElement = match editing.filter(|edit| edit.field == CellField::Description)
    {
        Some(edit) => editing_cell(edit, widths, shell, cx).into_any_element(),
        None => cell_part(request_id, table, n, CellField::Description, widths, cx)
            .font_family(bolt::fonts::ui_family(cx))
            .text_color(colors.muted)
            .when(off, |this| this.line_through())
            .child(
                cell_text(table, n, CellField::Description)
                    .child(row.description.clone().unwrap_or_default()),
            )
            .into_any_element(),
    };
    let line = colors.line;

    grid_text(
        labeled_row(
            row_id(table, n),
            row_label(&row.key, &row.value, secret_value),
        ),
        cx,
    )
    .flex()
    .items_center()
    .flex_none()
    .h(bolt::fonts::grid_row_height(cx))
    .when(unresolved, |this| {
        this.bg(tokens::unresolved_wash(colors.danger))
    })
    .child(Column::Enable.cell(line, widths).children(enable))
    .child(key_cell)
    .child(value_cell)
    .child(description)
    .child(match delete {
        Some(delete) => delete.into_any_element(),
        None => Column::Actions.cell(line, widths).into_any_element(),
    })
    .into_any_element()
}

/// The delete control in the actions cell of the 1-based row `n` of `table`, element id
/// `request-<table>-delete-<n>`: a ghost icon button; clicking it removes the row from the
/// request's draft.
fn delete_control(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    widths: GridWidths,
    cx: &mut Context<Shell>,
) -> Div {
    let line = bolt::colors(cx).line;
    let request = request_id.to_owned();
    let table = table.to_owned();
    let id = format!("request-{table}-delete-{n}");
    Column::Actions.cell(line, widths).child(
        Button::new(id, "")
            .ghost()
            .inline()
            .icon_only()
            .icon(ui::icon(IconName::Trash))
            .aria_label(format!("Delete {key}"))
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.delete_grid_row(&request, &table, n, window, cx);
            })),
    )
}

/// The add row under the grid of `table`, element id `request-<table>-add`: a grid row with
/// a plus in the enable cell and the placeholders of a new `kind` entry, which lights up
/// under the pointer so it reads as the place to type. Clicking it appends
/// an empty enabled row to the request's draft and opens its key cell.
fn add_row(
    request_id: &str,
    table: &str,
    kind: &str,
    widths: GridWidths,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let line = colors.line;
    let request = request_id.to_owned();
    let table = table.to_owned();
    grid_text(part(format!("request-{table}-add")), cx)
        .role(gpui::Role::Button)
        .aria_label(format!("Add to {table}"))
        .cursor_pointer()
        .flex()
        .items_center()
        .flex_none()
        .h(bolt::fonts::grid_row_height(cx))
        .text_color(colors.muted)
        .hover(|style| style.bg(colors.hover).text_color(colors.foreground))
        .child(
            Column::Enable.cell(line, widths).child(
                Icon::new(ui::icon(IconName::Plus))
                    .size(Fields::CHECK_MARK)
                    .color(colors.muted),
            ),
        )
        .child(Column::Key.cell(line, widths).child(placeholder(kind)))
        .child(Column::Value.cell(line, widths).child("Value"))
        .child(
            Column::Description
                .cell(line, widths)
                .font_family(bolt::fonts::ui_family(cx))
                .child("Description"),
        )
        .child(Column::Actions.cell(line, widths))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.add_grid_row(&request, &table, window, cx);
        }))
}

/// The key, value or description cell of the 1-based row `n` of `table`, element id
/// `request-<table>-<field>-<n>`. When `request_id` is `Some`, clicking it opens the
/// editable cell field in the cell's place.
fn cell_part(
    request_id: Option<&str>,
    table: &str,
    n: usize,
    field: CellField,
    widths: GridWidths,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let cell = cell_shell(table, n, field, bolt::colors(cx).line, widths);
    match request_id {
        Some(request) => {
            let request = request.to_owned();
            let table = table.to_owned();
            cell.cursor_text()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.start_cell_edit(&request, &table, n, field, window, cx);
                }))
        }
        None => cell,
    }
}

/// The key, value or description cell of row `n` of `table`, with its element id; the cell
/// fills the row's height, so an empty one still has a place to click.
fn cell_shell(
    table: &str,
    n: usize,
    field: CellField,
    line: Hsla,
    widths: GridWidths,
) -> Stateful<Div> {
    let (name, column) = match field {
        CellField::Key => ("key", Column::Key),
        CellField::Value => ("value", Column::Value),
        CellField::Description => ("description", Column::Description),
    };
    column.shape(part(format!("request-{table}-{name}-{n}")), line, widths)
}

/// The text of the key, value or description cell of row `n` of `table`, element id
/// `request-<table>-<field>-<n>-text`: where the open field's text starts too.
fn cell_text(table: &str, n: usize, field: CellField) -> Stateful<Div> {
    let name = match field {
        CellField::Key => "key",
        CellField::Value => "value",
        CellField::Description => "description",
    };
    part(format!("request-{table}-{name}-{n}-text"))
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .overflow_hidden()
}

/// The open field in the place of the cell `edit` replaces, element id `request-cell-input`
/// on the input: the cell becomes a well, the background with a tint focus ring drawn over
/// the cell's own lines, so the text keeps the place it had. Enter commits the text into the
/// draft and Escape drops what was typed, as the URL field does. A secret value keeps its
/// badge next to the field, which opens empty with a placeholder.
fn editing_cell(
    edit: &CellEdit,
    widths: GridWidths,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let cell = cell_shell(&edit.table, edit.n, edit.field, colors.line, widths).relative();
    let cell = match edit.field {
        CellField::Value => {
            let anchor = format!("request-{}-value-{}", edit.table, edit.n);
            variable_popover_view::editing_keys(
                cell,
                &edit.request_id,
                &anchor,
                &edit.input,
                shell,
                cx,
            )
        }
        CellField::Key | CellField::Description => cell,
    };
    let face = match edit.field {
        CellField::Description => bolt::fonts::ui_family(cx),
        CellField::Key | CellField::Value => bolt::fonts::grid_family(cx),
    };
    cell.bg(colors.background)
        .child(
            div()
                .absolute()
                .inset_0()
                .border(Borders::FOCUS_RING)
                .border_color(tint),
        )
        .text_color(colors.foreground)
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            GpuiInput::new(&edit.input)
                .id("request-cell-input")
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .flex_1()
                .min_w_0()
                .text_size(bolt::fonts::grid_font_size(cx))
                .font_family(face)
                .text_color(colors.foreground),
        )
        .when(edit.secret, |this| {
            this.child(part(format!("request-{}-secret-{}", edit.table, edit.n)).child(secret()))
        })
}

/// The enable box in the enable cell of the 1-based row `n` of `table`, element id
/// `request-<table>-enabled-<n>`: `bolt`'s checkbox, checked when the row is enabled.
/// Clicking it toggles the row in the request's draft.
fn enable_box(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    enabled: bool,
    cx: &mut Context<Shell>,
) -> Div {
    let request = request_id.to_owned();
    let table = table.to_owned();
    let id = format!("request-{table}-enabled-{n}");
    div().flex_none().child(
        Checkbox::new(gpui::ElementId::Name(id.into()))
            .checked(enabled)
            .aria_label(format!("Enable {key}"))
            .on_click(cx.listener(move |this, _checked: &bool, _window, cx| {
                this.toggle_grid_row(&request, &table, n, cx);
            })),
    )
}

/// The SECRET badge of a value that references a secret.
pub(crate) fn secret() -> Badge {
    Badge::new("SECRET", BadgeTone::Neutral).icon(ui::icon(IconName::Lock))
}
