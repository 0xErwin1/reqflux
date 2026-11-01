//! The response under the request: the tab row with the `Response`, `Tests` and `Console`
//! tabs, the status, time and size, then the empty state before the first send, the progress
//! bar while sending over the previous outcome dimmed, and the outcome: the unresolved line,
//! then the Body, Headers and Cookies views of a response (the body with its Pretty / Raw /
//! Preview bottom bar, the search in the views row and Copy) or its filtered Tests and
//! Console panes, or the error banner of a failure. A response reopened from history carries
//! the `From history` bar with Resend under the tab row. Clicking the content gives the
//! response the keyboard focus, where Ctrl F opens the search (see [`views`]).

mod body;
mod cookies;
mod cookies_view;
mod error;
mod filters;
mod footer;
mod history_bar;
mod model;
mod preview;
mod preview_model;
mod results;
mod search;
mod search_field;
mod sub_tabs;
mod tab_row;
mod views;

#[cfg(test)]
pub(crate) use cookies::cookie_count;
pub(crate) use model::{BodyText, BodyView, ResponseTab};
pub(crate) use preview::{Preview, PreviewHost};
pub(crate) use preview_model::{PreviewKind, preview_kind};
pub(crate) use views::{ViewState, bind_keys};

use bolt::composites::{EmptyState, EmptyStateAction};
use bolt::tokens::{Borders, Spacing};
use gpui::{
    AnyElement, App, Context, Div, MouseButton, ParentElement, Stateful, Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::request::ui;
use super::request::{Completed, SendState};
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
fn completed(id: &str, send: &RequestSend, outcome: &Completed, cx: &mut Context<Shell>) -> Div {
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
                    view => {
                        let found = body::Found {
                            hits: views::hits(send, cx).into(),
                            current: send.views.current,
                        };
                        body::render(received, view, found, &send.views.scroll, cx)
                    }
                },
                ResponseTab::Headers => sub_tabs::headers(received, cx),
                ResponseTab::Cookies => cookies_view::render(received, &send.masker, cx),
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
