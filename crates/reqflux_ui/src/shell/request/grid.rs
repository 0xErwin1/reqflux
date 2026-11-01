//! The Params and Headers grids as spreadsheet cells (request-panes/01): a header row, then
//! one row per entry with its enable box, the key, the value with its references styled, the
//! description and the delete control, each cell with a line on every side. Disabled rows are
//! muted and struck through, rows with an unresolved reference carry a warning wash, and a
//! value that references a secret shows `••••••••` with a SECRET badge. Clicking a row's
//! enable box toggles the row in the draft; clicking its key or value cell turns the cell
//! into a well with a text field. The delete control removes the row, and the add row under
//! the grid appends one.

use bolt::chamfer::Chamfer;
use bolt::tokens::{Colors, border, cut, family, space};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, ParentElement, Role, Stateful, Styled, Toggled,
    div, prelude::*,
};
use gpui_component::input::{Enter, Escape, Input};
use reqflux_core::model::KeyValue;
use reqflux_engine::Inspection;

use super::grid_model::{
    delete_icon_colors, has_unresolved, masked_text, placeholder, row_id, row_label, value_hint,
};
use super::spans;
use crate::shell::Shell;
use crate::shell::cell_edit::{CellEdit, CellField};
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// The hover group of a row's delete control.
const DELETE_GROUP: &str = "request-grid-delete";

/// A grid column: fixed width, or the value column, which fills the rest.
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
    fn cell(self, line: Hsla) -> Div {
        self.shape(div(), line)
    }

    /// `element` shaped as a cell of this column: the row's full height, its width, the line
    /// under it and the line on its right for every column but the last; the neighbors draw
    /// the other two sides.
    fn shape<E: Styled>(self, element: E, line: Hsla) -> E {
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
            Column::Key => cell.w(tokens::GRID_CELL_KEY_COLUMN).flex_none(),
            Column::Value => cell.flex_1(),
            Column::Description => cell.w(tokens::GRID_DESCRIPTION_COLUMN).flex_none(),
            Column::Actions => cell
                .w(tokens::GRID_ACTIONS_COLUMN)
                .flex_none()
                .justify_center(),
        };
        let cell = match self {
            Column::Key | Column::Value | Column::Description => cell
                .pl(tokens::GRID_CELL_PADDING)
                .pr(tokens::GRID_CELL_PADDING)
                .gap(space::SM),
            Column::Enable | Column::Actions => cell,
        };
        let cell = cell.border_b(border::THIN).border_color(line);
        match self {
            Column::Actions => cell,
            _ => cell.border_r(border::THIN),
        }
    }
}

/// The grid of `rows`, a `table` of `kind` (`params` and `param`, `headers` and `header`, or
/// `body`). A row whose entry in `files` is true holds a file path and shows a File icon.
/// When `request_id` is `Some`, every row's enable box toggles the row in the request's
/// draft, and its key and value cells open the editable cell field; with `None` the grid is
/// read-only.
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
    let add = request_id.map(|id| add_row(id, table, kind, cx));
    if rows.is_empty() {
        let colors = bolt::colors(cx);
        let empty = part("request-grid-empty".to_owned())
            .pl(space::MD)
            .pt(space::MD)
            .pb(space::MD)
            .text_size(tokens::GRID_TEXT)
            .text_color(colors.muted)
            .child(format!("No {table}"));
        return div()
            .flex()
            .flex_col()
            .child(empty)
            .children(add)
            .into_any_element();
    }
    let hint = value_hint(shell.environment_name().as_deref());
    let header = header(table, kind, hint, bolt::colors(cx));
    div()
        .flex()
        .flex_col()
        .child(header)
        .children(rows.iter().enumerate().map(|(index, row)| {
            let file = files.get(index).copied().unwrap_or(false);
            data_row(
                inspection,
                table,
                request_id,
                index + 1,
                row,
                file,
                shell,
                cx,
            )
        }))
        .children(add)
        .into_any_element()
}

/// The header row, element id `request-<table>-header`: the column names, the key's `kind`
/// and, when an environment is active, the value's `resolved in <environment>` as
/// `request-<table>-value-hint`.
fn header(
    table: &str,
    kind: &'static str,
    value_hint: Option<String>,
    colors: &Colors,
) -> Stateful<Div> {
    let note = |text: String| {
        div()
            .ml(space::HALF)
            .font_weight(FontWeight::NORMAL)
            .text_size(tokens::SMALL_MONO_TEXT)
            .text_color(colors.muted)
            .child(text)
    };
    let line = colors.line_2;
    part(format!("request-{table}-header"))
        .flex()
        .items_center()
        .flex_none()
        .h(tokens::GRID_HEADER)
        .bg(colors.background)
        .font_family(family::MONO)
        .text_size(tokens::GRID_HEADER_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(Column::Enable.cell(line))
        .child(
            Column::Key
                .cell(line)
                .child("key")
                .child(note(kind.to_owned())),
        )
        .child(Column::Value.cell(line).child("value").children(
            value_hint.map(|hint| part(format!("request-{table}-value-hint")).child(note(hint))),
        ))
        .child(Column::Description.cell(line).child("description"))
        .child(Column::Actions.cell(line))
}

#[allow(clippy::too_many_arguments)]
fn data_row(
    inspection: &Inspection,
    table: &str,
    request_id: Option<&str>,
    n: usize,
    row: &KeyValue,
    file: bool,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    // The box first: it takes the context mutably for its click listener, before the row
    // borrows it for the colors.
    let enable = request_id.map(|id| enable_box(id, table, n, &row.key, row.enabled, cx));
    let delete = request_id.map(|id| delete_control(id, table, n, &row.key, cx));
    let value_spans = inspection.spans(&row.value);
    let unresolved = has_unresolved(&value_spans) || has_unresolved(&inspection.spans(&row.key));
    let secret = inspection.references_secret(&row.value);
    let off = !row.enabled;
    let tone = |colors: &Colors, on: Hsla| if off { colors.muted } else { on };
    let editing = request_id.and_then(|id| {
        shell.cell_edit.as_ref().and_then(|edit| {
            (edit.request_id == id && edit.table == table && edit.n == n).then_some(edit)
        })
    });

    // The cells take the context mutably for their listeners, before the row borrows it
    // for the colors; each arm styles itself with its own colors borrow.
    let key_cell: AnyElement = match editing.filter(|edit| edit.field == CellField::Key) {
        Some(edit) => editing_cell(edit, cx).into_any_element(),
        None => {
            let cell = cell_part(request_id, table, n, CellField::Key, cx);
            let colors = bolt::colors(cx);
            cell.text_color(tone(colors, colors.strong))
                .when(off, |this| this.line_through())
                .child(cell_text(table, n, CellField::Key).child(row.key.clone()))
                .into_any_element()
        }
    };

    let value_cell: AnyElement = match editing.filter(|edit| edit.field == CellField::Value) {
        Some(edit) => editing_cell(edit, cx).into_any_element(),
        None => {
            let cell = cell_part(request_id, table, n, CellField::Value, cx);
            let colors = bolt::colors(cx);
            let value = cell
                .text_color(tone(colors, colors.foreground))
                .when(off, |this| this.line_through())
                .when(file, |this| {
                    this.child(
                        Icon::new(IconName::File)
                            .size(tokens::GRID_TEXT)
                            .color(colors.muted),
                    )
                });
            let text = cell_text(table, n, CellField::Value);
            let value = if secret {
                value
                    .child(text.child(masked_text(&value_spans)))
                    .child(part(format!("request-{table}-secret-{n}")).child(secret_badge(colors)))
            } else {
                value.child(text.children(spans::render(inspection, &row.value, cx)))
            };
            value.into_any_element()
        }
    };

    let colors = bolt::colors(cx);
    let line = colors.line;
    let description = Column::Description
        .shape(part(format!("request-{table}-description-{n}")), line)
        .font_family(family::INTERFACE)
        .text_color(colors.muted)
        .when(off, |this| this.line_through())
        .child(row.description.clone().unwrap_or_default());

    labeled_row(row_id(table, n), row_label(&row.key, &row.value, secret))
        .flex()
        .items_center()
        .flex_none()
        .h(tokens::GRID_CELL_ROW)
        .when(unresolved, |this| {
            this.bg(tokens::unresolved_row_wash(colors))
        })
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .child(Column::Enable.cell(line).children(enable))
        .child(key_cell)
        .child(value_cell)
        .child(description)
        .child(match delete {
            Some(delete) => delete.into_any_element(),
            None => Column::Actions.cell(line).into_any_element(),
        })
        .into_any_element()
}

/// The delete control in the actions cell of the 1-based row `n` of `table`, element id
/// `request-<table>-delete-<n>`: a muted trash icon that turns to the body color under the
/// pointer; clicking it removes the row from the request's draft.
fn delete_control(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let (rest, hover) = delete_icon_colors(bolt::colors(cx));
    let request = request_id.to_owned();
    let table = table.to_owned();
    part(format!("request-{table}-delete-{n}"))
        .role(Role::Button)
        .aria_label(format!("Delete {key}"))
        .group(DELETE_GROUP)
        .relative()
        .cursor_pointer()
        .flex_none()
        .w(tokens::GRID_ACTIONS_COLUMN)
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(IconName::Trash)
                .size(tokens::GRID_ACTION_ICON)
                .color(rest),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(DELETE_GROUP, |style| style.visible())
                .child(
                    Icon::new(IconName::Trash)
                        .size(tokens::GRID_ACTION_ICON)
                        .color(hover),
                ),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.delete_grid_row(&request, &table, n, window, cx);
        }))
}

/// The add row under the grid of `table`, element id `request-<table>-add`: a grid row with
/// a plus in the enable cell and the placeholders of a new `kind` entry. Clicking it appends
/// an empty enabled row to the request's draft and opens its key cell.
fn add_row(request_id: &str, table: &str, kind: &str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let line = colors.line;
    let request = request_id.to_owned();
    let table = table.to_owned();
    part(format!("request-{table}-add"))
        .role(Role::Button)
        .aria_label(format!("Add to {table}"))
        .cursor_pointer()
        .flex()
        .items_center()
        .flex_none()
        .h(tokens::GRID_CELL_ROW)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .text_color(colors.muted)
        .child(
            Column::Enable.cell(line).child(
                Icon::new(IconName::Plus)
                    .size(tokens::GRID_CHECK_ICON)
                    .color(colors.line_2),
            ),
        )
        .child(Column::Key.cell(line).child(placeholder(kind)))
        .child(Column::Value.cell(line).child("Value"))
        .child(
            Column::Description
                .cell(line)
                .font_family(family::INTERFACE)
                .child("Description"),
        )
        .child(Column::Actions.cell(line))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.add_grid_row(&request, &table, window, cx);
        }))
}

/// The key or value cell of the 1-based row `n` of `table`, element id
/// `request-<table>-<field>-<n>`. When `request_id` is `Some`, clicking it opens the
/// editable cell field in the cell's place.
fn cell_part(
    request_id: Option<&str>,
    table: &str,
    n: usize,
    field: CellField,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let cell = cell_shell(table, n, field, bolt::colors(cx).line);
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

/// The key or value cell of row `n` of `table`, with its element id; the cell fills the
/// row's height, so an empty key or value still has a place to click.
fn cell_shell(table: &str, n: usize, field: CellField, line: Hsla) -> Stateful<Div> {
    let (name, column) = match field {
        CellField::Key => ("key", Column::Key),
        CellField::Value => ("value", Column::Value),
    };
    column.shape(part(format!("request-{table}-{name}-{n}")), line)
}

/// The text of the key or value cell of row `n` of `table`, element id
/// `request-<table>-<field>-<n>-text`: where the open field's text starts too.
fn cell_text(table: &str, n: usize, field: CellField) -> Stateful<Div> {
    let name = match field {
        CellField::Key => "key",
        CellField::Value => "value",
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
fn editing_cell(edit: &CellEdit, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let cell = cell_shell(&edit.table, edit.n, edit.field, colors.line);
    cell.relative()
        .bg(colors.background)
        .child(
            div()
                .absolute()
                .inset_0()
                .border(border::FOCUS_RING)
                .border_color(tint),
        )
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .text_color(colors.foreground)
        .on_action(cx.listener(|this, action: &Enter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            Input::new(&edit.input)
                .id("request-cell-input")
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .flex_1()
                .min_w_0()
                .text_size(tokens::GRID_TEXT)
                .font_family(family::MONO)
                .text_color(colors.foreground),
        )
        .when(edit.secret, |this| {
            this.child(
                part(format!("request-{}-secret-{}", edit.table, edit.n))
                    .child(secret_badge(colors)),
            )
        })
}

/// The enable box in the enable cell of the 1-based row `n` of `table`, element id
/// `request-<table>-enabled-<n>`: a cut checkbox filled with the tint and a dark check when
/// the row is enabled, an empty well when it is not. Clicking it toggles the row in the
/// request's draft.
fn enable_box(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    enabled: bool,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let request = request_id.to_owned();
    let table = table.to_owned();
    let face = if enabled {
        Chamfer::new(cut::KEYCAP).fill(tint)
    } else {
        Chamfer::new(cut::KEYCAP)
            .fill(colors.background)
            .edge(colors.line_2, border::THIN)
    };
    part(format!("request-{table}-enabled-{n}"))
        .role(Role::CheckBox)
        .aria_label(format!("Enable {key}"))
        .aria_toggled(if enabled {
            Toggled::True
        } else {
            Toggled::False
        })
        .cursor_pointer()
        .relative()
        .flex_none()
        .size(tokens::GRID_CHECKBOX)
        .flex()
        .items_center()
        .justify_center()
        .child(face)
        .when(enabled, |this| {
            this.child(
                Icon::new(IconName::Check)
                    .size(tokens::GRID_CHECK_ICON)
                    .color(colors.panel),
            )
        })
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.toggle_grid_row(&request, &table, n, cx);
        }))
}

pub(crate) fn secret_badge(colors: &Colors) -> Div {
    div()
        .relative()
        .flex()
        .items_center()
        .gap(space::XS)
        .pl(space::HALF)
        .pr(space::HALF)
        .font_family(family::INTERFACE)
        .text_size(tokens::BADGE_TEXT)
        .font_weight(FontWeight::BOLD)
        .text_color(colors.muted)
        .child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
        .child(
            Icon::new(IconName::Lock)
                .size(tokens::BADGE_TEXT)
                .color(colors.muted),
        )
        .child("SECRET")
}
