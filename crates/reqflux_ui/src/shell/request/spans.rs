//! A text with its `{{name}}` references styled as the engine marks them: resolving ones in
//! the brand tint on its wash, unresolved ones in warning text on a warning wash, underlined.

use bolt::tokens::border;
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, div};
use reqflux_engine::{Inspection, Span};

use crate::tokens::variable_wash;

pub(crate) fn render(inspection: &Inspection, text: &str, cx: &App) -> Vec<AnyElement> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    inspection
        .spans(text)
        .into_iter()
        .map(|span| match span {
            Span::Literal(literal) => div().child(literal).into_any_element(),
            Span::Reference {
                raw,
                resolves: true,
                ..
            } => div()
                .text_color(tint)
                .bg(variable_wash(tint))
                .child(raw)
                .into_any_element(),
            Span::Reference { raw, .. } => div()
                .text_color(colors.warning)
                .bg(variable_wash(colors.warning))
                .border_b(border::THIN)
                .border_color(colors.warning)
                .child(raw)
                .into_any_element(),
        })
        .collect()
}
