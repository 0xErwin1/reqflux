//! The Params and Headers grids, as DBFlux draws data: a header row, then one row per entry
//! with its enable box, the 1-based number, the key, the value with its references styled
//! and the description. Disabled rows are muted, rows with an unresolved reference carry a
//! warning wash, and a value that references a secret shows `••••••••` with a SECRET badge.
//! Clicking a row's enable box toggles the row in the draft; clicking its key or value cell
//! swaps the cell for a text field. The delete control at the end of a row removes it, and
//! the `Add` row under the grid appends one.

use bolt::chamfer::Chamfer;
use bolt::tokens::{Colors, border, cut, family, space};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, ParentElement, Pixels, Role, Stateful, Styled,
    Toggled, div, prelude::*,
};
use gpui_component::input::{Enter, Escape, Input};
use reqflux_core::model::KeyValue;
use reqflux_engine::Inspection;

use super::grid_model::{has_unresolved, masked_text, row_id, row_label};
use super::spans;
use crate::shell::Shell;
use crate::shell::cell_edit::{CellEdit, CellField};
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// The grid of `rows`, a `table` of `kind` (`params` and `param`, `headers` and `header`, or
/// `body`). A row whose entry in `files` is true holds a file path and shows a File icon.
/// When `request_id` is `Some`, every row starts with an enable box that toggles the row in
/// the request's draft, and its key and value cells open the editable cell field; with
/// `None` the grid is read-only.
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
    let add = request_id.map(|id| add_row(id, table, cx));
    if rows.is_empty() {
        let colors = bolt::colors(cx);
        let empty = part("request-grid-empty".to_owned())
            .pl(space::MD)
            .pt(space::MD)
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
    let header = header(kind, bolt::colors(cx));
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

fn header(kind: &'static str, colors: &Colors) -> Div {
    let hint = div()
        .ml(space::HALF)
        .font_weight(FontWeight::NORMAL)
        .text_size(tokens::SMALL_MONO_TEXT)
        .text_color(colors.muted)
        .child(kind);
    div()
        .flex()
        .items_center()
        .h(tokens::GRID_HEADER)
        .bg(colors.background)
        .font_family(family::MONO)
        .text_size(tokens::GRID_HEADER_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(cell(tokens::GRID_NUMBER_COLUMN))
        .child(
            cell(tokens::GRID_KEY_COLUMN)
                .flex()
                .child("key")
                .child(hint),
        )
        .child(cell(tokens::GRID_VALUE_COLUMN).child("value"))
        .child(cell_fill().child("description"))
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
    let tone = |colors: &Colors, on: Hsla| if off { colors.line_2 } else { on };
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
            let cell = cell_part(
                tokens::GRID_KEY_COLUMN,
                format!("request-{table}-key-{n}"),
                request_id,
                table,
                n,
                CellField::Key,
                cx,
            );
            let colors = bolt::colors(cx);
            cell.text_color(tone(colors, colors.strong))
                .child(row.key.clone())
                .into_any_element()
        }
    };

    let value_cell: AnyElement = match editing.filter(|edit| edit.field == CellField::Value) {
        Some(edit) => editing_cell(edit, cx).into_any_element(),
        None => {
            let cell = cell_part(
                tokens::GRID_VALUE_COLUMN,
                format!("request-{table}-value-{n}"),
                request_id,
                table,
                n,
                CellField::Value,
                cx,
            )
            .flex()
            .items_center()
            .gap(space::SM);
            let colors = bolt::colors(cx);
            let value = cell
                .text_color(tone(colors, colors.foreground))
                .when(file, |this| {
                    this.child(
                        Icon::new(IconName::File)
                            .size(tokens::GRID_TEXT)
                            .color(colors.muted),
                    )
                });
            let value = if secret {
                value
                    .child(masked_text(&value_spans))
                    .child(part(format!("request-{table}-secret-{n}")).child(secret_badge(colors)))
            } else {
                value.children(spans::render(inspection, &row.value, cx))
            };
            value.into_any_element()
        }
    };

    let colors = bolt::colors(cx);
    let number_color = if unresolved {
        colors.warning
    } else {
        colors.line_2
    };

    labeled_row(row_id(table, n), row_label(&row.key, &row.value, secret))
        .flex()
        .items_center()
        .h(tokens::GRID_ROW)
        .border_b(border::THIN)
        .border_color(colors.row_divider)
        .when(unresolved, |this| {
            this.bg(tokens::unresolved_row_wash(colors))
        })
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .children(enable)
        .child(
            div()
                .w(tokens::GRID_NUMBER_COLUMN)
                .flex_none()
                .pr(space::MD)
                .text_right()
                .text_color(number_color)
                .child(n.to_string()),
        )
        .child(key_cell)
        .child(value_cell)
        .child(
            cell_fill()
                .font_family(family::INTERFACE)
                .text_color(tone(colors, colors.muted))
                .child(row.description.clone().unwrap_or_default()),
        )
        .children(delete)
        .into_any_element()
}

/// The delete control at the end of the 1-based row `n` of `table`, element id
/// `request-<table>-delete-<n>`: clicking it removes the row from the request's draft.
fn delete_control(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let request = request_id.to_owned();
    let table = table.to_owned();
    part(format!("request-{table}-delete-{n}"))
        .role(Role::Button)
        .aria_label(format!("Delete {key}"))
        .cursor_pointer()
        .flex_none()
        .pl(space::MD)
        .pr(space::MD)
        .child(
            Icon::new(IconName::Close)
                .size(tokens::GRID_TEXT)
                .color(colors.muted),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.delete_grid_row(&request, &table, n, window, cx);
        }))
}

/// The `Add` row under the grid of `table`, element id `request-<table>-add`: clicking it
/// appends an empty enabled row to the request's draft and opens its key cell.
fn add_row(request_id: &str, table: &str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let request = request_id.to_owned();
    let table = table.to_owned();
    part(format!("request-{table}-add"))
        .role(Role::Button)
        .aria_label(format!("Add to {table}"))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(tokens::GRID_ROW)
        .pl(space::MD)
        .text_size(tokens::GRID_TEXT)
        .text_color(colors.muted)
        .child(
            Icon::new(IconName::Plus)
                .size(tokens::GRID_TEXT)
                .color(colors.muted),
        )
        .child("Add")
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.add_grid_row(&request, &table, window, cx);
        }))
}

/// The key or value cell of the 1-based row `n` of `table`, element id
/// `request-<table>-<field>-<n>`. When `request_id` is `Some`, clicking it opens the
/// editable cell field over the cell.
#[allow(clippy::too_many_arguments)]
fn cell_part(
    width: Pixels,
    id: String,
    request_id: Option<&str>,
    table: &str,
    n: usize,
    field: CellField,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    // The cell fills the row's height, so an empty key or value still has a place to click.
    let cell = part(id)
        .w(width)
        .h_full()
        .flex()
        .items_center()
        .flex_none()
        .pl(space::MD)
        .pr(space::MD)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap();
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

/// The open field over the cell `edit` replaces, element id `request-cell-input` on the
/// input; the cell keeps its width and its place in the row. Enter commits the text into
/// the draft and Escape drops what was typed, as the URL field does. A secret value keeps
/// its badge next to the field, which opens empty with a placeholder.
fn editing_cell(edit: &CellEdit, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let (width, id) = match edit.field {
        CellField::Key => (
            tokens::GRID_KEY_COLUMN,
            format!("request-{}-key-{}", edit.table, edit.n),
        ),
        CellField::Value => (
            tokens::GRID_VALUE_COLUMN,
            format!("request-{}-value-{}", edit.table, edit.n),
        ),
    };
    part(id)
        .w(width)
        .flex_none()
        .min_w_0()
        .pl(space::MD)
        .pr(space::MD)
        .flex()
        .items_center()
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

/// The enable box at the start of the 1-based row `n` of `table`, element id
/// `request-<table>-enabled-<n>`: a cut box with a tint check when the row is enabled and
/// an empty one when it is not. Clicking it toggles the row in the request's draft.
fn enable_box(
    request_id: &str,
    table: &str,
    n: usize,
    key: &str,
    enabled: bool,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let request = request_id.to_owned();
    let table = table.to_owned();
    part(format!("request-{table}-enabled-{n}"))
        .role(Role::CheckBox)
        .aria_label(format!("Enable {key}"))
        .aria_toggled(if enabled {
            Toggled::True
        } else {
            Toggled::False
        })
        .cursor_pointer()
        .flex_none()
        .ml(space::MD)
        .mr(space::SM)
        .w(tokens::GRID_ENABLED_BOX)
        .h(tokens::GRID_ENABLED_BOX)
        .flex()
        .items_center()
        .justify_center()
        .child(
            Chamfer::new(cut::KEYCAP)
                .fill(colors.raised)
                .edge(colors.line, border::THIN),
        )
        .when(enabled, |this| {
            this.child(
                Icon::new(IconName::Check)
                    .size(tokens::BADGE_TEXT)
                    .color(brand.accent_tint),
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

fn cell(width: Pixels) -> Div {
    div()
        .w(width)
        .flex_none()
        .pl(space::MD)
        .pr(space::MD)
        .overflow_hidden()
        .whitespace_nowrap()
}

fn cell_fill() -> Div {
    div()
        .flex_1()
        .min_w_0()
        .pl(space::MD)
        .pr(space::MD)
        .overflow_hidden()
        .whitespace_nowrap()
}
