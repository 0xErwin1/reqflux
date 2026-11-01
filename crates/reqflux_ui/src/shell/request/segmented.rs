//! The segmented controls of the request and response panes: the body mode, the auth source
//! and location, the settings' Default / On / Off, the response views and the body format,
//! each `bolt::primitives::SegmentedControl` with ReqFlux's element ids on its segments.

use bolt::primitives::{SegmentedControl, SegmentedItem};
use gpui::{App, SharedString, Window};

/// The control `id` of `items`, `active` raised: a tab list when `tabs`, else a radio group,
/// its track keeping the id `id`. `label`, when set, names it in the automation tree.
/// `on_select` gets the id of the segment clicked.
pub(crate) fn control(
    id: &str,
    label: Option<SharedString>,
    tabs: bool,
    items: Vec<SegmentedItem>,
    active: impl Into<SharedString>,
    on_select: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
) -> SegmentedControl {
    let control = SegmentedControl::new(items, active, on_select)
        .group(id.to_owned())
        .track_id(id.to_owned())
        .tabs(tabs);
    match label {
        Some(label) => control.aria_label(label),
        None => control,
    }
}
