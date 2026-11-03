//! The response under the request: the tab row with the `Response`, `Tests` and `Console`
//! tabs, the status, time and size, then the empty state before the first send, the progress
//! bar while sending over the previous outcome dimmed, and the outcome: the unresolved line,
//! then the Body, Headers, Cookies and Timeline views of a response (the body with its
//! Pretty / Raw / Preview bottom bar, the JSONPath filter and the search in the views row
//! and Copy) or its filtered Tests and
//! Console panes, or the error banner of a failure. A response reopened from history carries
//! the `From history` bar with Resend under the tab row. Clicking the content gives the
//! response the keyboard focus, where Ctrl F opens the search (see [`views`]).
//!
//! The Document layout draws the same views, not in a split, under the section the cursor
//! is in: [`document_block`] is that block, with its own status chip and Retry.

mod body;
mod cookies;
mod cookies_view;
mod error;
mod filters;
mod footer;
mod history_bar;
mod jsonpath_field;
mod jsonpath_model;
mod model;
mod preview;
mod preview_model;
mod results;
mod search;
mod search_field;
mod sub_tabs;
mod syntax;
mod tab_row;
mod timeline;
mod timeline_model;
mod views;

#[cfg(test)]
pub(crate) use cookies::cookie_count;
pub(crate) use model::{BodyText, BodyView, ResponseTab};
pub(crate) use preview::{Preview, PreviewHost};
pub(crate) use preview_model::{PreviewKind, preview_kind};
pub(crate) use views::{ViewState, bind_keys};

use bolt::composites::{EmptyState, EmptyStateAction};
use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, BannerBlock, BannerVariant};
use bolt::tokens::{Borders, EditorMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, MouseButton, ParentElement, Role, Stateful, Styled, div,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::request::ui;
use super::request::{Completed, SendState, StatusClass, elapsed_text, size_text};
use super::send::RequestSend;
use super::{Shell, region, text_region};
use crate::tokens;

/// The response of the request `id`, element id `response`, filling the column under the
/// request; `send` is `None` before its first send.
pub(super) fn render(
    id: &str,
    send: Option<&RequestSend>,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let state = send.map(|send| &send.state);
    let content = match (send, state) {
        (Some(send), Some(SendState::Sending { previous, .. })) => {
            let previous = previous
                .as_ref()
                .map(|previous| completed(id, send, previous, cx).opacity(tokens::RESPONSE_DIMMED));
            let mut parts = vec![tab_row::progress(cx).into_any_element()];
            parts.extend(previous.map(IntoElement::into_any_element));
            parts
        }
        (Some(send), Some(SendState::Done(outcome))) => {
            vec![completed(id, send, outcome, cx).into_any_element()]
        }
        _ => vec![empty()],
    };
    let line_2 = bolt::colors(cx).line_2;
    let focus = send.and_then(|send| send.views.focus.clone());
    let request = id.to_owned();
    region("response")
        .key_context(views::RESPONSE_CONTEXT)
        .when_some(focus, |region, focus| region.track_focus(&focus))
        .on_action(
            cx.listener(move |this, _: &views::FindInResponse, window, cx| {
                this.open_response_search(&request, window, cx);
            }),
        )
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .border_t(Borders::THIN)
        .border_color(line_2)
        .child(tab_row::render(id, send, cx))
        .children(
            send.and_then(|send| send.history.as_ref())
                .map(|reopened| history_bar::render(id, reopened, cx)),
        )
        .children(content)
}

/// A finished send: the unresolved line, then the body or headers of a response or the
/// error banner of a failure.
pub(in crate::shell) fn completed(
    id: &str,
    send: &RequestSend,
    outcome: &Completed,
    cx: &mut Context<Shell>,
) -> Div {
    let unresolved = match outcome {
        Completed::Response(received) => &received.unresolved,
        Completed::Failure(failure) => &failure.unresolved,
    };
    let column = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .children(model::unresolved_text(unresolved).map(|text| unresolved_line(text, cx)));
    match outcome {
        Completed::Response(received) => {
            let content = match sub_tabs::effective_tab(send.tab, received) {
                ResponseTab::Body => match sub_tabs::effective_view(send.view, received) {
                    BodyView::Preview => preview::render(id, send, cx),
                    _ => {
                        let found = body::Found {
                            hits: views::hits(send, cx).into(),
                            current: send.views.current,
                        };
                        let lines = views::shown_lines(send, received, cx);
                        let body = body::render(received, lines, found, &send.views, cx);
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .children(jsonpath_field::error(send, received, cx))
                            .child(body)
                            .into_any_element()
                    }
                },
                ResponseTab::Headers => sub_tabs::headers(received, cx),
                ResponseTab::Cookies => cookies_view::render(received, &send.masker, cx),
                ResponseTab::Timeline => timeline::render(received, cx),
                ResponseTab::Tests => results::tests(id, received, send.views.tests, cx),
                ResponseTab::Console => results::console(id, received, send.views.console, cx),
            };
            let request = id.to_owned();
            // A click on the content gives the response the keyboard focus; the views row
            // and the bars keep theirs, so a click in the search field stays there.
            let content = div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.focus_response(&request, window, cx);
                    }),
                )
                .child(content);
            column
                .when(
                    !matches!(
                        sub_tabs::effective_tab(send.tab, received),
                        ResponseTab::Tests | ResponseTab::Console
                    ),
                    |column| column.child(sub_tabs::render(id, send, received, cx)),
                )
                .child(content)
                .when(
                    sub_tabs::effective_tab(send.tab, received) == ResponseTab::Body,
                    |column| column.child(footer::render(id, send, received, cx)),
                )
        }
        Completed::Failure(failure) => column.child(error::render(id, failure, cx)),
    }
}

/// `Sent with unresolved variables: <names>`, element id `response-unresolved`.
fn unresolved_line(text: String, cx: &App) -> Stateful<Div> {
    text_region("response-unresolved")
        .aria_label(text.clone())
        .flex_none()
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pb(Spacing::SM)
        .text_size(bolt::density::font_xs(cx))
        .text_color(bolt::colors(cx).warning)
        .child(text)
}

/// The height of the block's views: the tab row, the footer and a few body lines, so the
/// block stays a block under the section instead of taking the island over.
fn block_height() -> gpui::Pixels {
    EditorMetrics::CODE_LINE_HEIGHT * 8.0
}

/// The response block the Document layout draws under a section (v1-features S7,
/// design/layouts/02): the status chip with the time and size, then what the response pane
/// draws, reused — the progress bar over the previous outcome dimmed while a send runs,
/// the Body, Headers, Cookies, Timeline, Tests and Console views of a response, or the
/// error banner with Retry of a failure. The caller wraps it in
/// `document-section-<n>-response` under the section it belongs to.
pub(in crate::shell) fn document_block(
    id: &str,
    name: &str,
    send: &RequestSend,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let (label, tone, meta) = status_meta(&send.state);
    let content: Vec<AnyElement> = match &send.state {
        SendState::Sending { previous, .. } => {
            let mut parts = vec![tab_row::progress(cx).into_any_element()];
            parts.extend(previous.as_ref().map(|previous| {
                div()
                    .h(block_height())
                    .flex_none()
                    .flex()
                    .flex_col()
                    .opacity(tokens::RESPONSE_DIMMED)
                    .child(completed(id, send, previous, cx))
                    .into_any_element()
            }));
            parts
        }
        SendState::Done(outcome @ Completed::Response(_)) => {
            vec![
                region("document-response-body")
                    .h(block_height())
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(completed(id, send, outcome, cx))
                    .into_any_element(),
            ]
        }
        SendState::Done(Completed::Failure(failure)) => {
            vec![
                region("document-response-error")
                    .role(Role::Alert)
                    .aria_label(format!("Request failed. {}", failure.error))
                    .flex_none()
                    .child(
                        BannerBlock::new(BannerVariant::Danger, "Request failed.")
                            .default_icon(ui::icon(IconName::TriangleAlert))
                            .with_body(failure.error.clone())
                            .with_actions(retry(id, cx)),
                    )
                    .into_any_element(),
            ]
        }
        SendState::Idle => Vec::new(),
    };
    region("document-response")
        .flex_none()
        .flex()
        .flex_col()
        .mx(Spacing::MD)
        .my(Spacing::SM)
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .pl(Spacing::MD)
                .pr(Spacing::MD)
                .pt(Spacing::SM)
                .pb(Spacing::SM)
                .child(
                    div()
                        .text_size(bolt::density::font_xs(cx))
                        .text_color(bolt::colors(cx).muted)
                        .child(format!("Response · {name}")),
                )
                .child(
                    text_region("document-response-status")
                        .aria_label(label.clone())
                        .flex_none()
                        .child(Badge::new(label, tone)),
                )
                .children(meta.map(|meta| {
                    text_region("document-response-meta")
                        .aria_label(meta.clone())
                        .flex_none()
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_color(bolt::colors(cx).muted)
                        .child(meta)
                })),
        )
        .children(content)
}

/// The Retry of the block's error banner, `document-response-retry`, which sends the
/// section again, from the document as it shows now.
fn retry(id: &str, cx: &mut Context<Shell>) -> Button {
    let id = id.to_owned();
    Button::new("document-response-retry", "Retry")
        .primary()
        .icon(ui::icon(IconName::RotateCcw))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.resend_document_section(&id, window, cx);
        }))
}

/// The label, tone and `time · size` meta of the block's status chip.
fn status_meta(state: &SendState) -> (String, BadgeTone, Option<String>) {
    match state {
        SendState::Idle | SendState::Sending { .. } => {
            ("Sending".to_owned(), BadgeTone::Accent, None)
        }
        SendState::Done(Completed::Response(received)) => (
            received.status_text(),
            tone_of(received.status),
            Some(format!(
                "{} · {}",
                elapsed_text(received.elapsed),
                size_text(received.size())
            )),
        ),
        SendState::Done(Completed::Failure(failure)) => (
            "Failed".to_owned(),
            BadgeTone::Danger,
            Some(format!(
                "{} · {}",
                elapsed_text(failure.elapsed),
                size_text(0)
            )),
        ),
    }
}

/// The badge tone of a status, the one the response tab row colors it with.
fn tone_of(status: u16) -> BadgeTone {
    match StatusClass::of(status) {
        StatusClass::Success => BadgeTone::Success,
        StatusClass::Info => BadgeTone::Info,
        StatusClass::Warning => BadgeTone::Warning,
        StatusClass::Danger => BadgeTone::Danger,
    }
}

/// The empty state before the first send, element id `response-empty`.
fn empty() -> AnyElement {
    let text = "Send the request to see its response";
    text_region("response-empty")
        .aria_label(text)
        .flex_1()
        .min_h_0()
        .flex()
        .child(
            EmptyState::new(ui::icon(IconName::Send), text).action(EmptyStateAction::new(
                ui::icon(IconName::Send),
                "Send",
                ["Ctrl ↵"],
            )),
        )
        .into_any_element()
}
