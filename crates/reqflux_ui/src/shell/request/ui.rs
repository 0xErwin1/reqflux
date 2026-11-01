//! What the request and response panes need to draw with `bolt`: an icon of the Lucide
//! catalog as a `bolt` icon source and a `bolt` text role applied to a container.

use bolt::icon::IconSource;
use bolt::primitives::{TextDefaultColor, TextVariant};
use bolt_legacy::IconName;
use gpui::{App, Styled};

/// The `bolt` icon source of the Lucide icon `name`.
pub(crate) fn icon(name: IconName) -> IconSource {
    IconSource::Svg(name.path())
}

/// `element` with the family, size, weight and default color of the `bolt` text role
/// `variant`, for a container whose children are the text.
///
/// Capitals are not a style in GPUI: callers pass the text of the `Label` role already in
/// capitals.
pub(crate) fn role<E: Styled>(element: E, variant: TextVariant, cx: &App) -> E {
    let contract = variant.role_contract();
    let colors = bolt::colors(cx);
    let color = match contract.color {
        TextDefaultColor::Strong => colors.strong,
        TextDefaultColor::Foreground => colors.foreground,
        TextDefaultColor::MutedForeground => colors.muted,
    };
    element
        .font_family(bolt::fonts::family_for(cx, contract.family))
        .text_size(variant.density_size(cx))
        .font_weight(contract.weight)
        .text_color(color)
}
