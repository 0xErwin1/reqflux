//! Column resize for every table of the app, through one mechanism. A table has a stable
//! id (`variables`, `cookies`, `request-headers`, ...) and its resizable columns are numbered
//! from 0 in the order the table lists them. A view reads a column's width with [`width`],
//! wraps its root in [`resizable`] so the drag follows the pointer, decorates each resizable
//! header cell with [`with_handle`], and puts header and rows in [`overflow`] so a pane narrower
//! than the columns scrolls them sideways together; nothing else is per view. The widths and the
//! viewports' sideways offsets live in a global for the session, not in a file, and a column
//! nobody dragged keeps the default its view passes. `bolt` holds no widths, only the drag, the
//! clamp and the overflow.

use std::cell::RefCell;
use std::collections::HashMap;

use bolt::components::grid::{
    ColumnResize, ColumnResizeDrag, GridOverflow, clamp_column_width, column_resize_handle,
    track_column_resize,
};
use gpui::{
    App, Div, Global, InteractiveElement, ParentElement, Pixels, ScrollHandle, SharedString,
    Stateful, StatefulInteractiveElement, Styled, div,
};

/// The widths the user dragged, per table and column, the drag in progress, and the sideways
/// scroll of each table's viewport, so it keeps its offset across renders.
#[derive(Default)]
pub(crate) struct TableWidths {
    drag: ColumnResizeDrag,
    widths: HashMap<(&'static str, usize), f32>,
    scroll: RefCell<HashMap<String, ScrollHandle>>,
}

impl Global for TableWidths {}

impl TableWidths {
    /// Takes the width a drag asked for, clamped, and leaves every other column as it was.
    fn apply(&mut self, table: &'static str, resize: ColumnResize) {
        self.widths
            .insert((table, resize.column), clamp_column_width(resize.width));
    }

    /// The handle the viewport `id` scrolls sideways on: made the first time it is asked for
    /// and kept since, so a redraw keeps the offset.
    fn viewport(&self, id: &str) -> ScrollHandle {
        self.scroll
            .borrow_mut()
            .entry(id.to_owned())
            .or_default()
            .clone()
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

// The sideways viewport of a table whose pane has a height of its own, or `scrollable` below
// where the height is the content's. `bolt`'s `GridOverflow` keeps no widths and no offset of
// its own: the handle and the widths stay here.
/// `table`'s header and rows inside `bolt`'s `GridOverflow`: a viewport `id` that scrolls them
/// sideways together, for a pane narrower than `total_width`, the width the columns add up to.
/// While the columns overflow, the rows end one `GridMetrics::SCROLLBAR_WIDTH` above the
/// viewport's bottom so its bar covers no row; a table that fits draws as it would without the
/// wrapper. It needs a pane with a height of its own: chain `.flex_1().min_h_0()` on it, as a
/// flex item of a column that has one.
pub(crate) fn overflow(id: String, total_width: Pixels, cx: &App) -> GridOverflow {
    let handle = cx
        .try_global::<TableWidths>()
        .map(|store| store.viewport(&id))
        .unwrap_or_default();
    GridOverflow::new(id, &handle, f32::from(total_width))
}

// bolt gap: `GridOverflow` takes its height from the pane it is given (its viewport is
// `size_full` under a `flex_1().min_h_0()` wrapper), so a page that takes its content's height
// — the request panes, the variables page, the app settings page, the history page — collapses
// it to nothing. This viewport stands in for the tables drawn there until `bolt` has an overflow
// that takes its content's height.
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
