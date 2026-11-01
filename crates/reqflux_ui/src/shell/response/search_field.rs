//! The search field at the end of the response views row (response-views/02), element id
//! `response-search`: closed, the search icon, `Search in response` and the `Ctrl F` keycap
//! (`response-search-keycap`), opening the search when clicked; open, the text field
//! `response-search-input` with the focus ring, the count `response-search-count` and the
//! previous and next buttons `response-search-previous` and `response-search-next`.

use bolt::controls::{Button, Input, InputEnter, InputEscape};
use bolt::primitives::{Chamfer, ChamferRing, Icon, Kbd};
use bolt::tokens::{ChamferCut, Fields, ResultMetrics};
use gpui::{ClickEvent, Context, Div, ParentElement, Stateful, Styled, div, prelude::*};
use gpui_kit_assets::IconName;

use super::search::count_label;
use super::views::{SEARCH_PLACEHOLDER, hits};
use crate::shell::request::ui;
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text_region};

/// The search field of the response of the request `id`.
pub(super) fn render(id: &str, send: &RequestSend, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let open = send
        .views
        .search
        .as_ref()
        .map(|search| search.input.clone());
    let mut shape = Chamfer::new(ChamferCut::CONTROL)
        .fill(colors.background)
        .border(colors.line);
    if open.is_some() {
        shape = shape.ring(ChamferRing::focus(bolt::brand::brand(cx).accent_tint));
    }
    let field = region("response-search")
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .w(ResultMetrics::SEARCH_WIDTH)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(Fields::TEXT)
        .child(shape)
        .child(
            Icon::new(ui::icon(IconName::Search))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        );
    let Some(input) = open else {
        let request = id.to_owned();
        return field
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
            .child(Kbd::new("Ctrl F").id("response-search-keycap"));
    };
    let total = hits(send, cx).len();
    let count = count_label(send.views.current, total);
    let (escape, enter, previous, next) =
        (id.to_owned(), id.to_owned(), id.to_owned(), id.to_owned());
    field
        .capture_action(cx.listener(move |this, _: &InputEscape, window, cx| {
            this.close_response_search(&escape, window, cx);
            cx.stop_propagation();
        }))
        .capture_action(cx.listener(move |this, action: &InputEnter, _window, cx| {
            this.step_response_search(&enter, !action.shift, cx);
            cx.stop_propagation();
        }))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(&input)
                    .id("response-search-input")
                    .aria_label(SEARCH_PLACEHOLDER)
                    .small()
                    .appearance(false),
            ),
        )
        .child(
            text_region("response-search-count")
                .aria_label(count.clone())
                .flex_none()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(ResultMetrics::STATEMENT_FONT)
                .text_color(colors.muted)
                .child(count),
        )
        .child(
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
        .child(
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
        )
}
