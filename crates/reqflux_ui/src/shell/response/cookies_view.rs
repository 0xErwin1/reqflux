//! The Cookies sub-tab: one row per `Set-Cookie` header, as received, so a live response
//! shows cookie values unmasked (history masks them when it records the send).

use bolt::primitives::Icon;
use bolt::tokens::{Borders, Fields, GridMetrics, Heights, Spacing};
use bolt_legacy::IconName;
use gpui::{AnyElement, App, FontWeight, ParentElement, Styled, div, prelude::*};

use super::cookies::{Cookie, cookies};
use crate::shell::region;
use crate::shell::request::{Received, ui};
use crate::shell::rows::labeled_row;
use crate::tokens;

/// The cookies of `received`, element id `response-cookies`, one row
/// `response-cookie-<n>` each: name, value and attributes, or the raw header with a
/// warning mark when it does not parse.
pub(super) fn render(received: &Received, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let rows = cookies(&received.headers)
        .into_iter()
        .enumerate()
        .map(|(index, cookie)| {
            let id = format!("response-cookie-{}", index + 1);
            match cookie {
                Cookie::Parsed(cookie) => {
                    let attributes = cookie.attributes();
                    let label = match attributes.is_empty() {
                        true => format!("{}={}", cookie.name, cookie.value),
                        false => format!("{}={} · {attributes}", cookie.name, cookie.value),
                    };
                    labeled_row(id, label)
                        .flex()
                        .items_center()
                        .h(Heights::ROW)
                        .border_b(Borders::THIN)
                        .border_color(colors.row_divider)
                        .child(
                            div()
                                .w(tokens::GRID_KEY_COLUMN)
                                .flex_none()
                                .pl(Spacing::MD)
                                .pr(Spacing::MD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.strong)
                                .child(cookie.name),
                        )
                        .child(
                            div()
                                .w(tokens::GRID_VALUE_COLUMN)
                                .flex_none()
                                .pr(Spacing::MD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(colors.foreground)
                                .child(cookie.value),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .pr(Spacing::MD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(colors.muted)
                                .child(attributes),
                        )
                }
                Cookie::Raw(raw) => labeled_row(id, format!("Could not parse: {raw}"))
                    .flex()
                    .items_center()
                    .gap(Spacing::SM)
                    .h(Heights::ROW)
                    .pl(Spacing::MD)
                    .border_b(Borders::THIN)
                    .border_color(colors.row_divider)
                    .child(
                        Icon::new(ui::icon(IconName::TriangleAlert))
                            .size(Fields::CHEVRON)
                            .color(colors.warning),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr(Spacing::MD)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(colors.foreground)
                            .child(raw),
                    ),
            }
        });
    region("response-cookies")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .children(rows)
        .into_any_element()
}
