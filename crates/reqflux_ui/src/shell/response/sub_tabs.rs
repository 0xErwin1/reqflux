//! The response views row, `Body`, `Headers <n>` and `Cookies <n>` with the content type,
//! and the Headers grid.

use bolt::tokens::{Borders, GridMetrics, Heights, ResultMetrics, Spacing};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, Div, FontWeight, ParentElement, Role, Styled, div, prelude::*,
};

use super::cookies::cookie_count;
use super::model::{BodyView, ResponseTab};
use crate::shell::request::Received;
use crate::shell::request::segmented::{segment, track};
use crate::shell::rows::labeled_row;
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region};
use crate::tokens;

/// The tab the response view shows: a `Tests` or `Console` tab with nothing to show falls
/// back to `Body`, so a response without script results never leaves the pane without a
/// selected tab.
pub(super) fn effective_tab(tab: ResponseTab, received: &Received) -> ResponseTab {
    match tab {
        ResponseTab::Tests if received.scripts.tests.is_empty() => ResponseTab::Body,
        ResponseTab::Console if received.scripts.console.is_empty() => ResponseTab::Body,
        other => other,
    }
}

/// The body view the response shows: `Preview` of a body with nothing to preview falls
/// back to `Pretty`, so the bottom bar keeps a selected segment.
pub(super) fn effective_view(view: BodyView, received: &Received) -> BodyView {
    match view {
        BodyView::Preview if received.preview.is_none() => BodyView::Pretty,
        other => other,
    }
}

/// The views row of the response `received` of the request `id`: the `Body`, `Headers <n>`
/// and `Cookies <n>` segments, element id `response-views`, then the content type.
pub(super) fn render(
    id: &str,
    send: &RequestSend,
    received: &Received,
    cx: &mut Context<Shell>,
) -> Div {
    let colors = bolt::colors(cx);
    let tab = effective_tab(send.tab, received);
    let headers = received.headers.len().to_string();
    let cookie_count = cookie_count(&received.headers).to_string();
    let views = [
        (ResponseTab::Body, "Body", IconName::Braces, Vec::new()),
        (
            ResponseTab::Headers,
            "Headers",
            IconName::List,
            vec![(headers, colors.muted)],
        ),
        (
            ResponseTab::Cookies,
            "Cookies",
            IconName::Cookie,
            vec![(cookie_count, colors.muted)],
        ),
    ];
    let segments: Vec<_> = views
        .into_iter()
        .map(|(view_tab, label, icon_name, counts)| {
            let request = id.to_owned();
            segment(
                format!("response-tab-{}", label.to_lowercase()),
                label,
                Some(icon_name),
                counts,
                tab == view_tab,
                Role::Tab,
                cx,
            )
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.select_response_tab(&request, view_tab, cx);
            }))
        })
        .collect();
    div()
        .flex_none()
        .h(ResultMetrics::VIEW_ROW_HEIGHT)
        .flex()
        .items_center()
        .gap(ResultMetrics::VIEW_ROW_GAP)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(track("response-views", Role::TabList, segments, cx))
        .children(received.content_type().map(|content_type| {
            div()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(ResultMetrics::STATEMENT_FONT)
                .text_color(colors.muted)
                .child(content_type.to_owned())
        }))
}

/// The headers of `received`, one row `response-headers-row-<n>` each, name then value.
pub(super) fn headers(received: &Received, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    region("response-headers")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .children(
            received
                .headers
                .iter()
                .enumerate()
                .map(|(index, (name, value))| {
                    labeled_row(
                        format!("response-headers-row-{}", index + 1),
                        format!("{name}: {value}"),
                    )
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
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr(Spacing::MD)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(colors.foreground)
                            .child(value.clone()),
                    )
                }),
        )
        .into_any_element()
}
