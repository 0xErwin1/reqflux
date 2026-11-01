//! The segmented controls of the request and response panes: the body mode, the auth source
//! and location, the settings' Default / On / Off, the response views and the body format,
//! each `bolt::primitives::SegmentedControl` with ReqFlux's element ids on its segments.

use bolt::primitives::{SegmentedControl, SegmentedItem};
use gpui::{App, Div, ParentElement, Role, SharedString, Stateful, Styled, Window, prelude::*};

use crate::shell::rows::part;

/// The control `id` of `items`, `active` raised: a tab list when `tabs`, else a radio group.
/// `label`, when set, names it in the automation tree. `on_select` gets the id of the
/// segment clicked.
// bolt gap: SegmentedControl names its track `segmented-<group>` and takes no accessible
// name, so a group around it keeps the caller's id `id` and `label`.
pub(crate) fn control(
    id: &str,
    label: Option<SharedString>,
    tabs: bool,
    items: Vec<SegmentedItem>,
    active: impl Into<SharedString>,
    on_select: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    part(id.to_owned())
        .flex_none()
        .when_some(label, |group, label| {
            group.role(Role::Group).aria_label(label)
        })
        .child(
            SegmentedControl::new(items, active, on_select)
                .group(id.to_owned())
                .tabs(tabs),
        )
}
