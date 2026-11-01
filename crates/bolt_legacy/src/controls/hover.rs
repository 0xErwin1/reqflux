//! The hover wash shared by the clickable controls, and how a control names itself.

use gpui::prelude::*;
use gpui::{Div, ElementId, Hsla, Pixels, Role, SharedString, Stateful, div};

use crate::chamfer::Chamfer;

/// The hover group every clickable control declares on its root.
pub(super) const GROUP: &str = "bolt-control";

/// A cut wash in `color` over the whole control, shown only while the pointer is over the
/// nearest [`GROUP`]. Place it after the rest fill and before the content.
pub(super) fn hover_wash(cut: Pixels, color: Hsla) -> Div {
    div()
        .absolute()
        .inset_0()
        .invisible()
        .group_hover(GROUP, |style| style.visible())
        .child(Chamfer::new(cut).fill(color))
}

/// The root of a control: element id `id`, which tests also select by, `role`, and `label`
/// as the name UI automation reads.
pub(super) fn named(id: &SharedString, role: Role, label: SharedString) -> Stateful<Div> {
    let selector = id.to_string();
    div()
        .id(ElementId::Name(id.clone()))
        .debug_selector(move || selector)
        .role(role)
        .aria_label(label)
}
