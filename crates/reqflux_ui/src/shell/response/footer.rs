//! The bottom bar of the response body, element id `response-footer`: the `Pretty` / `Raw` /
//! `Preview` switch and the line count of a text body.

use bolt::tokens::{border, chrome, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{Context, Div, ParentElement, Stateful, Styled, prelude::*};

use super::model::BodyView;
use super::segmented::{segment, track};
use super::sub_tabs::effective_view;
use crate::shell::request::Received;
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text as text_role, text_region};

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
                icon_name,
                Vec::new(),
                selected == view,
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
        .h(chrome::DOC_TAB_ROW)
        .flex()
        .items_center()
        .gap(space::MD)
        .pl(space::MD)
        .pr(space::MD)
        .border_t(border::THIN)
        .border_color(colors.line)
        .child(track("response-format", segments, cx))
        .children(lines.map(|count| {
            let label = match count {
                1 => "1 line".to_owned(),
                count => format!("{count} lines"),
            };
            text_role::role(text_region("response-footer-lines"), text::CAPTION, cx)
                .aria_label(label.clone())
                .flex()
                .items_center()
                .gap(space::HALF)
                .whitespace_nowrap()
                .child(Icon::new(IconName::List).size(icon::SM).color(colors.muted))
                .child(label)
        }))
}
