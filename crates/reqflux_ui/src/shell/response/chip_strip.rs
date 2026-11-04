//! The horizontally scrollable strips of the response's chips: the tab row's tabs and the
//! views row's segmented control. A pane too narrow for a strip never cuts a chip: the strip
//! keeps the pane's width and scrolls (wheel or trackpad), and the selected chip is scrolled
//! into view when the selection or the pane's width changes.

use std::cell::Cell;

use gpui::{Div, Pixels, ScrollHandle, Stateful, StatefulInteractiveElement, Styled, point};

use crate::shell::region;

// bolt gap: `bolt`'s tab bar and segmented control have no overflow behavior (scroll or a
// "more" menu); this strip stands in until they do.
/// A scroll viewport `id` at most as wide as its parent, following `handle`.
pub(super) fn strip(id: &'static str, handle: &ScrollHandle) -> Stateful<Div> {
    region(id)
        .flex()
        .flex_none()
        .max_w_full()
        .overflow_x_scroll()
        .track_scroll(handle)
}

/// What a strip last revealed: the chip and the pane width it was revealed for.
#[derive(Default)]
pub(crate) struct Revealed(Cell<Option<(usize, Pixels)>>);

impl Revealed {
    /// `true` the first time it is asked for chip `index` at `pane`, and again once either
    /// changes, so a scroll the user made is kept until then.
    fn due(&self, index: usize, pane: Pixels) -> bool {
        let key = Some((index, pane));
        let due = self.0.get() != key;
        self.0.set(key);
        due
    }

    /// Scrolls `handle`, whose children are the chips, so chip `index` shows.
    pub(super) fn child(&self, handle: &ScrollHandle, index: usize, pane: Pixels) {
        if self.due(index, pane) {
            handle.scroll_to_item(index);
        }
    }

    /// Scrolls `handle`, whose single child holds `count` chips of alike width, so chip
    /// `index` shows: the first at the start, the last at the end, the others between.
    pub(super) fn share(&self, handle: &ScrollHandle, index: usize, count: usize, pane: Pixels) {
        let last = count.saturating_sub(1);
        if last == 0 {
            return;
        }
        // Nothing to scroll yet: the strip fits, or it has not been laid out.
        let hidden = handle.max_offset().x;
        if hidden > Pixels::ZERO && self.due(index, pane) {
            let share = index as f32 / last as f32;
            handle.set_offset(point(-(hidden * share), Pixels::ZERO));
        }
    }
}
