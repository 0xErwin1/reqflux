//! The bottom bar of the response body, element id `response-footer`: the `Pretty` / `Raw` /
//! `Preview` switch, the line count of a text body and, while the JSONPath filter shows
//! matches, `filtered view of a <size> body` (`response-footer-filtered`), and at the end
//! Copy (`response-copy`), which puts what shows on the clipboard.

use bolt::controls::Button;
use bolt::primitives::{Icon, SegmentedItem, TextVariant};
use bolt::tokens::{Borders, Fields, ResultMetrics};
use gpui::{Context, Div, ParentElement, SharedString, Stateful, Styled, prelude::*};
use gpui_kit_assets::IconName;

use super::jsonpath_model::{Filtered, filtered_caption};
use super::model::{BodyText, BodyView};
use super::sub_tabs::effective_view;
use super::views::shown_lines;
use crate::shell::request::segmented::control;
use crate::shell::request::{Received, size_text, ui};
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
        _ => shown_lines(send, received, cx).map(|(lines, _)| lines.len()),
    };
    let filtered = (selected != BodyView::Preview
        && matches!(
            send.views.filtered(received, cx).as_deref(),
            Some(Filtered::Matches { .. })
        ))
    .then(|| {
        let caption = filtered_caption(&size_text(received.size()));
        ui::role(
            text_region("response-footer-filtered"),
            TextVariant::Caption,
            cx,
        )
        .aria_label(caption.clone())
        .flex()
        .items_center()
        .gap(ResultMetrics::FOOTER_ITEM_GAP)
        .whitespace_nowrap()
        .child(
            Icon::new(ui::icon(IconName::Funnel))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        )
        .child(caption)
    });
    let copy = (*received.text != BodyText::Binary).then(|| {
        let request = id.to_owned();
        Button::new("response-copy", "Copy")
            .secondary()
            .icon(ui::icon(IconName::Copy))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.copy_response_body(&request, cx);
            }))
    });
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
        .children(filtered)
        .child(gpui::div().flex_1())
        .children(copy)
        .children(crate::shell::examples::response_buttons(id, cx))
}
