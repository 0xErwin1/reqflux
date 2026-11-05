//! The JSONPath field beside the body search (response-views/02), element id
//! `response-jsonpath`: closed, the funnel icon and `Filter with JSONPath`, opening the
//! filter when clicked; open, the text field `response-jsonpath-input` and the count
//! `response-jsonpath-count` (`<n> matches`, or `error` with the ring in danger). Under the
//! views row, the danger banner `response-jsonpath-error` carries the engine's message
//! while the body shows unfiltered.
//!
//! Closed it is `bolt`'s `SearchTrigger`, open `bolt`'s `SearchField`: the two states of one
//! `bolt` control.

use bolt::controls::{InputEscape, SearchCount, SearchField, SearchTrigger};
use bolt::primitives::{BadgeTone, BannerBlock, BannerVariant};
use bolt::tokens::{ResultMetrics, Spacing};
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
        .w(ResultMetrics::SEARCH_WIDTH)
        .min_w_0()
        .capture_action(cx.listener(move |this, _: &InputEscape, window, cx| {
            this.close_response_jsonpath(&escape, window, cx);
            cx.stop_propagation();
        }))
        .child(
            SearchField::new("response-jsonpath", &input)
                .icon(ui::icon(IconName::Funnel))
                .aria_label(JSONPATH_PLACEHOLDER)
                .count(count),
        )
        .into_any_element()
}

/// The closed field, `bolt`'s `SearchTrigger`: the funnel icon, the placeholder and the same
/// frame as the open field, opening the filter when clicked. The field fills the width the
/// views row gives it, so it narrows with the row as the open field does.
fn closed(id: &str, cx: &mut Context<Shell>) -> AnyElement {
    let request = id.to_owned();
    div()
        .w(ResultMetrics::SEARCH_WIDTH)
        .min_w_0()
        .child(
            SearchTrigger::new("response-jsonpath", JSONPATH_PLACEHOLDER)
                .icon(ui::icon(IconName::Funnel))
                .aria_label(JSONPATH_PLACEHOLDER)
                .on_open(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_response_jsonpath(&request, window, cx);
                })),
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
