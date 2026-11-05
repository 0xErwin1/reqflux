//! The search field at the end of the response views row (response-views/02), element id
//! `response-search`: closed, the search icon, `Search in response` and the `Ctrl F` keycap
//! (`response-search-key`), opening the search when clicked; open, the text field
//! `response-search-input` with the focus ring, the count `response-search-count` and the
//! previous and next buttons `response-search-previous` and `response-search-next`. Either
//! state is `bolt`'s: `SearchTrigger` closed, `SearchField` open.

use bolt::controls::{Button, InputEnter, InputEscape, SearchCount, SearchField, SearchTrigger};
use bolt::tokens::ResultMetrics;
use gpui::{AnyElement, ClickEvent, Context, ParentElement, Styled, div, prelude::*};
use gpui_kit_assets::IconName;

use super::search::count_label;
use super::views::{SEARCH_PLACEHOLDER, hits};
use crate::shell::Shell;
use crate::shell::request::ui;
use crate::shell::send::RequestSend;

/// The search field of the response of the request `id`.
pub(super) fn render(id: &str, send: &RequestSend, cx: &mut Context<Shell>) -> AnyElement {
    let open = send
        .views
        .search
        .as_ref()
        .map(|search| search.input.clone());
    let Some(input) = open else {
        return closed(id, cx);
    };
    let total = hits(send, cx).len();
    let count = count_label(send.views.current, total);
    let (escape, enter, previous, next) =
        (id.to_owned(), id.to_owned(), id.to_owned(), id.to_owned());
    let field = SearchField::new("response-search", &input)
        .icon(ui::icon(IconName::Search))
        .aria_label(SEARCH_PLACEHOLDER)
        .count(SearchCount::new(count))
        .trailing(
            Button::new("response-search-previous", "Previous match")
                .ghost()
                .inline()
                .icon_only()
                .icon(ui::icon(IconName::ChevronUp))
                .aria_label("Previous match")
                .disabled(total == 0)
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.step_response_search(&previous, false, cx);
                })),
        )
        .trailing(
            Button::new("response-search-next", "Next match")
                .ghost()
                .inline()
                .icon_only()
                .icon(ui::icon(IconName::ChevronDown))
                .aria_label("Next match")
                .disabled(total == 0)
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.step_response_search(&next, true, cx);
                })),
        );
    div()
        .w(ResultMetrics::SEARCH_WIDTH)
        .min_w_0()
        .capture_action(cx.listener(move |this, _: &InputEscape, window, cx| {
            this.close_response_search(&escape, window, cx);
            cx.stop_propagation();
        }))
        .capture_action(cx.listener(move |this, action: &InputEnter, _window, cx| {
            this.step_response_search(&enter, !action.shift, cx);
            cx.stop_propagation();
        }))
        .child(field)
        .into_any_element()
}

/// The closed field, `bolt`'s `SearchTrigger`: the search icon, the placeholder and the `Ctrl F`
/// keycap, opening the search when clicked. The field fills the width the views row gives it,
/// so it narrows with the row as the open field does and clips its parts first.
fn closed(id: &str, cx: &mut Context<Shell>) -> AnyElement {
    let request = id.to_owned();
    div()
        .w(ResultMetrics::SEARCH_WIDTH)
        .min_w_0()
        .child(
            SearchTrigger::new("response-search", SEARCH_PLACEHOLDER)
                .icon(ui::icon(IconName::Search))
                .shortcut("Ctrl F")
                .aria_label(SEARCH_PLACEHOLDER)
                .on_open(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_response_search(&request, window, cx);
                })),
        )
        .into_any_element()
}
