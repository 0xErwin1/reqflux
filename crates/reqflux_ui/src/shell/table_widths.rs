//! Column resize for every table of the app, through one mechanism. A table has a stable
//! id (`variables`, `cookies`, `request-headers`, ...) and its resizable columns are numbered
//! from 0 in the order the table lists them. A view reads a column's width with [`width`],
//! wraps its root in [`resizable`] so the drag follows the pointer, and decorates each
//! resizable header cell with [`with_handle`]; nothing else is per view. The widths live in a
//! global for the session, not in a file, and a column nobody dragged keeps the default its
//! view passes. `bolt` holds no widths, only the drag and the clamp.

use std::collections::HashMap;

use bolt::components::grid::{
    ColumnResize, ColumnResizeDrag, clamp_column_width, column_resize_handle, track_column_resize,
};
use gpui::{
    App, Div, Global, InteractiveElement, ParentElement, Pixels, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div,
};

/// The widths the user dragged, per table and column, and the drag in progress.
#[derive(Default)]
pub(crate) struct TableWidths {
    drag: ColumnResizeDrag,
    widths: HashMap<(&'static str, usize), f32>,
}

impl Global for TableWidths {}

impl TableWidths {
    /// Takes the width a drag asked for, clamped, and leaves every other column as it was.
    fn apply(&mut self, table: &'static str, resize: ColumnResize) {
        self.widths
            .insert((table, resize.column), clamp_column_width(resize.width));
    }
}

/// Installs the store when the app has none, so a view can read it from any `&App`.
pub(crate) fn install(cx: &mut App) {
    if !cx.has_global::<TableWidths>() {
        cx.set_global(TableWidths::default());
    }
}

/// The width of `column` of `table`: the dragged one, else `default`.
pub(crate) fn width(table: &'static str, column: usize, default: Pixels, cx: &App) -> Pixels {
    cx.try_global::<TableWidths>()
        .and_then(|store| store.widths.get(&(table, column)).copied())
        .map_or(default, Pixels::from)
}

/// `root`, the element that holds the whole table, following the drag of any of the table's
/// handles: each move stores the width it asks for and redraws.
pub(crate) fn resizable<E: InteractiveElement>(root: E, table: &'static str, cx: &App) -> E {
    let drag = cx
        .try_global::<TableWidths>()
        .map(|store| store.drag.clone())
        .unwrap_or_default();
    track_column_resize(root, &drag, move |resize, _window, app| {
        if !app.has_global::<TableWidths>() {
            return;
        }
        app.global_mut::<TableWidths>().apply(table, resize);
        app.refresh_windows();
    })
}

/// `cell`, a header cell of `column` of `table` whose width is [`width`] of `default`, with
/// `bolt`'s resize handle on its right edge.
pub(crate) fn with_handle<E: Styled + ParentElement>(
    cell: E,
    table: &'static str,
    column: usize,
    default: Pixels,
    cx: &App,
) -> E {
    let drag = cx
        .try_global::<TableWidths>()
        .map(|store| store.drag.clone())
        .unwrap_or_default();
    // The hover wash of the handle, as DBFlux's data table draws it.
    let wash = bolt::brand::brand(cx).accent_tint.opacity(0.3);
    let current = f32::from(width(table, column, default, cx));
    cell.relative()
        .child(column_resize_handle(column, current, &drag, wash))
}

// bolt gap: `bolt`'s grid has no horizontal overflow; this viewport stands in until it does.
/// `table`, a table that needs `min_width`, in a viewport `id` that scrolls sideways when its
/// pane is narrower. Columns that fill the rest (a value, a URL) keep their minimum instead of
/// collapsing under the cells beside them. A vertical wheel stays with the pane.
pub(crate) fn scrollable(id: String, table: Div, min_width: Pixels) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .flex_none()
        .w_full()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .child(table.min_w(min_width))
}

/// [`scrollable`] for a table that fills the height its parent leaves: the viewport and the
/// table take it, and the table's own rows scroll vertically inside.
pub(crate) fn scrollable_fill(id: String, table: Div, min_width: Pixels) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .child(table.min_w(min_width).h_full())
}

/// The drag every table shares and the wash its handles take, for a header `bolt` draws
/// itself (`render_header` with a `HeaderResize`).
pub(crate) fn header_resize(cx: &App) -> (ColumnResizeDrag, gpui::Hsla) {
    let drag = cx
        .try_global::<TableWidths>()
        .map(|store| store.drag.clone())
        .unwrap_or_default();
    (drag, bolt::brand::brand(cx).accent_tint.opacity(0.3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resize_changes_one_column_of_one_table_and_clamps() {
        let mut store = TableWidths::default();
        store.apply(
            "a",
            ColumnResize {
                column: 1,
                width: 10.0,
            },
        );
        assert_eq!(
            store.widths.get(&("a", 1)),
            Some(&bolt::components::grid::MIN_COLUMN_WIDTH)
        );
        assert_eq!(store.widths.get(&("a", 0)), None);
        assert_eq!(store.widths.get(&("b", 1)), None);
    }
}
