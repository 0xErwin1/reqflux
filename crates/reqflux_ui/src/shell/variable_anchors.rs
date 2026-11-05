//! Where each element a variable popover opens from sits in the window, by element id: a
//! `{{name}}` mark or the field under edit, recorded as the element is laid out, so the
//! surface can point at it.
//!
//! A mark records its rectangle in the frame that draws it, and the shell drops every anchor
//! no mark recorded in that frame once it is drawn: the map keeps what the frame shows, so a
//! request's marks go with its tab or its request and never pile up.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use gpui::{Bounds, Pixels};

/// The window rectangle of each anchor, with the frame it was last recorded in. Cloned as an
/// `Rc`, so the shell, each mark's tracker and the frame that prunes share one map.
#[derive(Default)]
pub(crate) struct VariableAnchors {
    frame: Cell<u64>,
    anchors: RefCell<HashMap<String, (Bounds<Pixels>, u64)>>,
}

impl VariableAnchors {
    /// Starts the frame the shell is drawing: the anchors its marks record carry its number.
    pub(crate) fn begin_frame(&self) {
        self.frame.set(self.frame.get().wrapping_add(1));
    }

    /// Records the rectangle of `anchor` as the frame being drawn lays it out.
    pub(crate) fn record(&self, anchor: &str, bounds: Bounds<Pixels>) {
        if let Ok(mut anchors) = self.anchors.try_borrow_mut() {
            anchors.insert(anchor.to_owned(), (bounds, self.frame.get()));
        }
    }

    /// The rectangle `anchor` was drawn at, from the frame that last drew it.
    pub(crate) fn bounds(&self, anchor: &str) -> Option<Bounds<Pixels>> {
        self.anchors.borrow().get(anchor).map(|(bounds, _)| *bounds)
    }

    /// Drops every anchor no mark recorded in the frame being drawn: the marks of a closed
    /// request, and of a tab that left the screen, are gone with it.
    pub(crate) fn retain_current_frame(&self) {
        let frame = self.frame.get();
        if let Ok(mut anchors) = self.anchors.try_borrow_mut() {
            anchors.retain(|_, (_, recorded)| *recorded == frame);
        }
    }

    /// The ids of the anchors the frame kept, for tests.
    #[cfg(test)]
    pub(crate) fn ids(&self) -> Vec<String> {
        self.anchors.borrow().keys().cloned().collect()
    }
}
