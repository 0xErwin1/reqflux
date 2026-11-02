//! The JSONPath field beside the body search (response-views/02), element id
//! `response-jsonpath`: closed, the funnel icon and `Filter with JSONPath`, opening the
//! filter when clicked; open, the text field `response-jsonpath-input` and the count
//! `response-jsonpath-count` (`<n> matches`, or `error` with the ring in danger). Under the
//! views row, the danger banner `response-jsonpath-error` carries the engine's message
//! while the body shows unfiltered.

use bolt::controls::{Input, InputEscape};
use bolt::primitives::{BannerBlock, BannerVariant, Chamfer, ChamferRing, Icon};
use bolt::tokens::{ChamferCut, Fields, ResultMetrics, Spacing};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, ParentElement, Role, Stateful, Styled, div,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::jsonpath_model::{Filtered, JSONPATH_PLACEHOLDER};
use crate::shell::request::{Received, ui};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text_region};

/// The JSONPath field of the response `received` of the request `id`.
// bolt gap: a search field with a trailing count (B1); this mirrors `response-search`.
pub(super) fn render(
    id: &str,
    send: &RequestSend,
    received: &Received,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let open = send
        .views
        .jsonpath
        .as_ref()
        .map(|field| field.input.clone());
    let filtered = open
        .as_ref()
        .and_then(|_| send.views.filtered(received, cx));
    let mut shape = Chamfer::new(ChamferCut::CONTROL)
        .fill(colors.background)
        .border(colors.line);
    if open.is_some() {
        let ring = match filtered.as_deref() {
            Some(Filtered::Error(_)) => colors.danger,
            _ => tint,
        };
        shape = shape.ring(ChamferRing::focus(ring));
    }
    let field = region("response-jsonpath")
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .w(ResultMetrics::SEARCH_WIDTH)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(Fields::TEXT)
        .child(shape)
        .child(
            Icon::new(ui::icon(IconName::Funnel))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        );
    let Some(input) = open else {
        let request = id.to_owned();
        return field
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_response_jsonpath(&request, window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(colors.muted)
                    .child(JSONPATH_PLACEHOLDER),
            );
    };
    let count = filtered.as_deref().map(|filtered| {
        let color = match filtered {
            Filtered::Matches { count: 0, .. } => colors.muted,
            Filtered::Matches { .. } => tint,
            Filtered::Error(_) => colors.danger,
        };
        let label = filtered.count_label();
        text_region("response-jsonpath-count")
            .aria_label(label.clone())
            .flex_none()
            .whitespace_nowrap()
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(ResultMetrics::STATEMENT_FONT)
            .text_color(color)
            .child(label)
    });
    let escape = id.to_owned();
    field
        .capture_action(cx.listener(move |this, _: &InputEscape, window, cx| {
            this.close_response_jsonpath(&escape, window, cx);
            cx.stop_propagation();
        }))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(&input)
                    .id("response-jsonpath-input")
                    .aria_label(JSONPATH_PLACEHOLDER)
                    .small()
                    .appearance(false),
            ),
        )
        .children(count)
}

/// The danger banner of a failed filter under the views row, `None` unless the filter of
/// `send` failed on `received`.
pub(super) fn error(send: &RequestSend, received: &Received, cx: &App) -> Option<AnyElement> {
    let filtered = send.views.filtered(received, cx)?;
    let Filtered::Error(message) = filtered.as_ref() else {
        return None;
    };
    Some(
        region("response-jsonpath-error")
            .role(Role::Alert)
            .aria_label(message.clone())
            .flex_none()
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .pb(Spacing::SM)
            .child(
                BannerBlock::new(BannerVariant::Danger, message.clone())
                    .default_icon(ui::icon(IconName::TriangleAlert)),
            )
            .into_any_element(),
    )
}
