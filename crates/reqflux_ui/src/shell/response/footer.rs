//! The bottom bar of the response body, element id `response-footer`: the `Pretty` / `Raw` /
//! `Preview` switch and the line count of a text body.

use bolt::primitives::{Icon, SegmentedItem, TextVariant};
use bolt::tokens::{Borders, Fields, ResultMetrics};
use gpui::{Context, Div, ParentElement, SharedString, Stateful, Styled, prelude::*};
use gpui_kit_assets::IconName;

use super::model::BodyView;
use super::sub_tabs::effective_view;
use crate::shell::request::segmented::control;
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
    let active = views
        .iter()
        .find(|(view, ..)| *view == selected)
        .map_or(String::new(), |(_, label, _)| label.to_lowercase());
    let keys: Vec<_> = views
        .iter()
        .map(|(view, label, _)| (*view, label.to_lowercase()))
        .collect();
    let segments: Vec<_> = views
        .into_iter()
        .map(|(_, label, icon_name)| {
            let key = label.to_lowercase();
            SegmentedItem::new(key.clone(), label)
                .element_id(format!("response-view-{key}"))
                .icon(ui::icon(icon_name))
        })
        .collect();
    let request = id.to_owned();
    let on_select = cx.listener(move |this, picked: &SharedString, _window, cx| {
        if let Some((view, _)) = keys.iter().find(|(_, key)| key == picked.as_ref()) {
            this.select_body_view(&request, *view, cx);
        }
    });
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
        .child(control(
            "response-format",
            None,
            true,
            segments,
            active,
            on_select,
        ))
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
