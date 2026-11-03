//! The JSONPath field beside the body search (response-views/02), element id
//! `response-jsonpath`: closed, the funnel icon and `Filter with JSONPath`, opening the
//! filter when clicked; open, the text field `response-jsonpath-input` and the count
//! `response-jsonpath-count` (`<n> matches`, or `error` with the ring in danger). Under the
//! views row, the danger banner `response-jsonpath-error` carries the engine's message
//! while the body shows unfiltered.

use bolt::controls::{InputEscape, SearchCount, SearchField};
use bolt::primitives::{BadgeTone, BannerBlock, BannerVariant, Chamfer, Icon};
use bolt::tokens::{ChamferCut, Fields, ResultMetrics, Spacing};
use gpui::{AnyElement, App, ClickEvent, Context, ParentElement, Role, Styled, div, prelude::*};
use gpui_kit_assets::IconName;

use super::jsonpath_model::{Filtered, JSONPATH_PLACEHOLDER};
use crate::shell::request::{Received, ui};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region};

/// The JSONPath field of the response `received` of the request `id`: open, `bolt`'s
/// `SearchField` with the match count.
pub(super) fn render(
    id: &str,
    send: &RequestSend,
    received: &Received,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let open = send
        .views
        .jsonpath
        .as_ref()
        .map(|field| field.input.clone());
    let Some(input) = open else {
        return closed(id, cx);
    };
    let count = send.views.filtered(received, cx).map(|filtered| {
        let tone = match filtered.as_ref() {
            Filtered::Matches { count: 0, .. } => BadgeTone::Neutral,
            Filtered::Matches { .. } => BadgeTone::Accent,
            Filtered::Error(_) => BadgeTone::Danger,
        };
        SearchCount::new(filtered.count_label()).tone(tone)
    });
    let escape = id.to_owned();
    div()
        .flex_none()
        .capture_action(cx.listener(move |this, _: &InputEscape, window, cx| {
            this.close_response_jsonpath(&escape, window, cx);
            cx.stop_propagation();
        }))
        .child(
            SearchField::new("response-jsonpath", &input)
                .icon(ui::icon(IconName::Funnel))
                .aria_label(JSONPATH_PLACEHOLDER)
                .width(ResultMetrics::SEARCH_WIDTH)
                .count(count),
        )
        .into_any_element()
}

/// The closed field: the funnel icon and the placeholder, opening the filter when clicked.
// bolt gap: `SearchField` has no closed (trigger) state; drawn with its frame and metrics.
fn closed(id: &str, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let request = id.to_owned();
    region("response-jsonpath")
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
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.open_response_jsonpath(&request, window, cx);
        }))
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
        .child(
            Icon::new(ui::icon(IconName::Funnel))
                .size(Fields::LEADING_ICON)
                .color(colors.muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(colors.muted)
                .child(JSONPATH_PLACEHOLDER),
        )
        .into_any_element()
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
