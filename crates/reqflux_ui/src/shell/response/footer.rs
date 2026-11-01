//! The bottom bar of the response body, element id `response-footer`: the `Pretty` / `Raw` /
//! `Preview` switch and the line count of a text body.

use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Borders, Fields, ResultMetrics};
use bolt_legacy::IconName;
use gpui::{Context, Div, ParentElement, Role, Stateful, Styled, prelude::*};

use super::model::BodyView;
use super::sub_tabs::effective_view;
use crate::shell::request::segmented::{segment, track};
use crate::shell::request::{Received, ui};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text_region};

/// The bar of the body of `received`, shown in the response of the request `id`.
pub(super) fn render(
    id: &str,
    send: &RequestSend,
    received: &Received,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let selected = effective_view(send.view, received);
    let mut views = vec![
        (BodyView::Pretty, "Pretty", IconName::Braces),
        (BodyView::Raw, "Raw", IconName::Code),
    ];
    if received.preview.is_some() {
        views.push((BodyView::Preview, "Preview", IconName::Eye));
    }
    let segments: Vec<_> = views
        .into_iter()
        .map(|(view, label, icon_name)| {
            let request = id.to_owned();
            segment(
                format!("response-view-{}", label.to_lowercase()),
                label,
                Some(icon_name),
                Vec::new(),
                selected == view,
                Role::Tab,
                cx,
            )
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.select_body_view(&request, view, cx);
            }))
        })
        .collect();
    let lines = match selected {
        BodyView::Preview => None,
        view => received.text.lines(view).map(|(lines, _)| lines.len()),
    };
    region("response-footer")
        .flex_none()
        .h(ResultMetrics::FOOTER_HEIGHT)
        .flex()
        .items_center()
        .gap(ResultMetrics::FOOTER_GAP)
        .pl(ResultMetrics::FOOTER_PADDING_X)
        .pr(ResultMetrics::FOOTER_PADDING_X)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(track("response-format", Role::TabList, segments, cx))
        .children(lines.map(|count| {
            let label = match count {
                1 => "1 line".to_owned(),
                count => format!("{count} lines"),
            };
            ui::role(
                text_region("response-footer-lines"),
                TextVariant::Caption,
                cx,
            )
            .aria_label(label.clone())
            .flex()
            .items_center()
            .gap(ResultMetrics::FOOTER_ITEM_GAP)
            .whitespace_nowrap()
            .child(
                Icon::new(ui::icon(IconName::List))
                    .size(Fields::CHEVRON)
                    .color(colors.muted),
            )
            .child(label)
        }))
}
