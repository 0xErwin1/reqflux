//! Applies Bolt text roles to elements.

use bolt_legacy::tokens::{TextRole, TextTone};
use gpui::{App, Styled};

/// Sets the family, size, weight and default color of `role` on `element`.
///
/// Capitals are not a style in GPUI: callers pass the text of an uppercase role already in
/// capitals.
pub(super) fn role<E: Styled>(element: E, role: TextRole, cx: &App) -> E {
    let colors = bolt_legacy::colors(cx);
    let color = match role.tone {
        TextTone::Strong => colors.strong,
        TextTone::Body => colors.foreground,
        TextTone::Muted => colors.muted,
    };
    element
        .font_family(role.family)
        .text_size(role.size)
        .font_weight(role.weight)
        .text_color(color)
}
