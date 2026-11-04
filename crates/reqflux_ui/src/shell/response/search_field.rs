//! The search field at the end of the response views row (response-views/02), element id
//! `response-search`: closed, the search icon, `Search in response` and the `Ctrl F` keycap
//! (`response-search-keycap`), opening the search when clicked; open, the text field
//! `response-search-input` with the focus ring, the count `response-search-count` and the
//! previous and next buttons `response-search-previous` and `response-search-next`. Open, it is
//! `bolt`'s `SearchField`.

use bolt::controls::{Button, InputEnter, InputEscape, SearchCount, SearchField};
use bolt::primitives::{Chamfer, Icon, Kbd};
use bolt::tokens::{ChamferCut, Fields, ResultMetrics};
use gpui::{AnyElement, ClickEvent, Context, ParentElement, Styled, div, prelude::*};
use gpui_kit_assets::IconName;

use super::search::count_label;
use super::views::{SEARCH_PLACEHOLDER, hits};
use crate::shell::request::ui;
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region};

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

/// The closed field: the search icon, the placeholder and the keycap, opening the search
/// when clicked.
// bolt gap: `SearchField` has no closed (trigger) state; drawn with its frame and metrics.
fn closed(id: &str, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let request = id.to_owned();
    region("response-search")
        .relative()
        .min_w_0()
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .w(ResultMetrics::SEARCH_WIDTH)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(Fields::TEXT)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
        .child(
            Icon::new(ui::icon(IconName::Search))
                .size(Fields::LEADING_ICON)
                .color(colors.muted),
        )
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.open_response_search(&request, window, cx);
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(colors.muted)
                .child(SEARCH_PLACEHOLDER),
        )
        .child(Kbd::new("Ctrl F").id("response-search-keycap"))
        .into_any_element()
}
