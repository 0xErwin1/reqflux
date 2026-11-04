//! Reveals the selected segment of a scrolling `bolt` segmented control once per selection.

use std::cell::RefCell;

use bolt::composites::StripReveal;
use bolt::primitives::SegmentBounds;
use gpui::{Pixels, ScrollHandle};

// bolt gap: `StripReveal::segment` reads segment bounds painted one frame before the scroll
// offset, so after a wheel step it computes a shifted range, counts it as a new target and
// snaps the strip back to the selection. This gate runs it only when the selected segment or
// the pane's width changes, a frame whose bounds and offset agree.
/// A [`StripReveal`] for a segmented control that reveals the selected segment on a change of
/// selection or pane width only, and keeps any scroll the user makes in between.
#[derive(Default)]
pub(crate) struct SegmentReveal {
    reveal: StripReveal,
    last: RefCell<Option<(String, Pixels)>>,
}

impl SegmentReveal {
    /// Scrolls `handle` so the segment `id`, measured by `segments`, shows, when `id` or
    /// `width` differ from the last reveal. Waits, remembering nothing, until the strip and the
    /// segment have been laid out and painted.
    pub(super) fn segment(
        &self,
        handle: &ScrollHandle,
        segments: &SegmentBounds,
        id: &str,
        width: Pixels,
    ) {
        let unchanged = self
            .last
            .borrow()
            .as_ref()
            .is_some_and(|(last, last_width)| last == id && *last_width == width);
        if unchanged || handle.bounds().size.width <= Pixels::ZERO || segments.get(id).is_none() {
            return;
        }
        self.reveal.segment(handle, segments, id, width);
        *self.last.borrow_mut() = Some((id.to_owned(), width));
    }
}
