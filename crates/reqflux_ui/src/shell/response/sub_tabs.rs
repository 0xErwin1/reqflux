//! The response views row, `Body`, `Headers <n>` and `Cookies <n>` with the content type
//! and the body search field, and the Headers grid.

use bolt::primitives::SegmentedItem;
use bolt::tokens::{Borders, GridMetrics, Heights, ResultMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, ParentElement, SharedString, Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::cookies::cookie_count;
use super::model::{BodyView, ResponseTab};
use super::search_field;
use crate::shell::request::Received;
use crate::shell::request::segmented::control;
use crate::shell::request::ui;
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
/// and `Cookies <n>` segments, element id `response-views`, then the content type and, at
/// the end, the search field.
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
        (ResponseTab::Body, "body", "Body", IconName::Braces, None),
        (
            ResponseTab::Headers,
            "headers",
            "Headers",
            IconName::List,
            Some(headers),
        ),
        (
            ResponseTab::Cookies,
            "cookies",
            "Cookies",
            IconName::Cookie,
            Some(cookie_count),
        ),
    ];
    let active = views
        .iter()
        .find(|(view_tab, ..)| *view_tab == tab)
        .map_or("", |(_, key, ..)| *key);
    let keys = views.clone().map(|(view_tab, key, ..)| (view_tab, key));
    let segments: Vec<_> = views
        .into_iter()
        .map(|(_, key, label, icon_name, count)| {
            let item = SegmentedItem::new(key, label)
                .element_id(format!("response-tab-{key}"))
                .icon(ui::icon(icon_name));
            match count {
                Some(count) => item.count(count),
                None => item,
            }
        })
        .collect();
    let request = id.to_owned();
    let on_select = cx.listener(move |this, picked: &SharedString, _window, cx| {
        if let Some((view_tab, _)) = keys.iter().find(|(_, key)| *key == picked.as_ref()) {
            this.select_response_tab(&request, *view_tab, cx);
        }
    });
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
        .child(control(
            "response-views",
            None,
            true,
            segments,
            active,
            on_select,
        ))
        .children(received.content_type().map(|content_type| {
            div()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(ResultMetrics::STATEMENT_FONT)
                .text_color(colors.muted)
                .child(content_type.to_owned())
        }))
        .child(div().flex_1())
        .child(search_field::render(id, send, cx))
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
