//! A text with its `{{name}}` references styled as the engine marks them: resolving ones in
//! the brand tint on its wash, unresolved ones in warning text on a warning wash, underlined.

use bolt::tokens::Borders;
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div,
};
use reqflux_engine::{Inspection, Span};

use super::variable_popover_view;
use crate::shell::Shell;
use crate::tokens::{variable_hot_wash, variable_wash};

pub(crate) fn render(inspection: &Inspection, text: &str, cx: &App) -> Vec<AnyElement> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
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
                .border_b(Borders::THIN)
                .border_color(colors.warning)
                .child(raw)
                .into_any_element(),
        })
        .collect()
}

/// [`render`], with each reference of the request `request_id` a mark the variable popover
/// opens from, element id `<prefix>-var-<n>` for the 1-based reference `n`: a click opens it
/// without reaching the field's own click, and a 400 ms hover opens it too. The open popover
/// hangs under its mark.
pub(crate) fn render_marked(
    inspection: &Inspection,
    text: &str,
    request_id: &str,
    prefix: &str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let mut n = 0;
    inspection
        .spans(text)
        .into_iter()
        .map(|span| match span {
            Span::Literal(literal) => div().child(literal).into_any_element(),
            Span::Reference {
                raw,
                name,
                resolves,
                ..
            } => {
                n += 1;
                mark(
                    &raw,
                    &name,
                    resolves,
                    request_id,
                    &format!("{prefix}-var-{n}"),
                    shell,
                    cx,
                )
            }
        })
        .collect()
}

fn mark(
    raw: &str,
    name: &str,
    resolves: bool,
    request_id: &str,
    anchor: &str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let open = shell
        .variable_popover
        .as_ref()
        .is_some_and(|popover| popover.anchor == anchor);
    let selector = anchor.to_owned();
    let styled = div()
        .id(SharedString::from(anchor.to_owned()))
        .debug_selector(move || selector)
        .relative()
        .cursor_pointer();
    let styled = if resolves {
        styled.text_color(tint).bg(if open {
            variable_hot_wash(tint)
        } else {
            variable_wash(tint)
        })
    } else {
        styled
            .text_color(colors.warning)
            .bg(variable_wash(colors.warning))
            .border_b(Borders::THIN)
            .border_color(colors.warning)
    };
    let (click_request, click_anchor, click_name) =
        (request_id.to_owned(), anchor.to_owned(), name.to_owned());
    let (hover_request, hover_anchor, hover_name) =
        (request_id.to_owned(), anchor.to_owned(), name.to_owned());
    styled
        .child(raw.to_owned())
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation()
        })
        .on_click(cx.listener(move |this, _event, window, cx| {
            cx.stop_propagation();
            this.open_variable_popover(&click_request, &click_anchor, &click_name, window, cx);
        }))
        .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
            this.hover_variable_mark(
                &hover_request,
                &hover_anchor,
                &hover_name,
                *hovered,
                window,
                cx,
            );
        }))
        .children(variable_popover_view::anchored_at(anchor, shell, cx))
        .into_any_element()
}
