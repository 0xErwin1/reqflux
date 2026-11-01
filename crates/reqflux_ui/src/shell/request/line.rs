//! The request line: the method chip, which opens the method menu, the URL with its references styled, and
//! the Send button, which sends the request, or the Cancel button while it is in flight; both
//! have the same width, so the line does not move. Clicking the URL swaps it for an editable
//! field drawn in the same place, padding, font and height, so the text does not move.
//! Enter in the field commits the URL and sends the request.

use bolt::controls::{Button, GpuiInput, Input, InputEnter, InputEscape, InputState};
use bolt::primitives::{Chamfer, Icon, TextVariant};
use bolt::tokens::{ChamferCut, Fields, Spacing};
use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Pixels, Role, Stateful, Styled, Window, div,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::model::RequestState;
use super::{method_menu, spans, ui};
use crate::shell::{Shell, region};
use crate::tokens::{self, method_hue};

/// The line of the request `id`, element id `request-line`.
pub(super) fn render(
    id: &str,
    state: &RequestState,
    sending: bool,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let action = if sending {
        cancel(id, cx)
    } else {
        send(id, cx)
    };
    region("request-line")
        .flex_none()
        .h(tokens::REQUEST_LINE)
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .child(method(id, state, shell, window, cx))
        .child(url(id, state, shell, cx))
        .child(action)
}

/// The Cancel button shown while the request is in flight, element id `request-cancel`.
fn cancel(id: &str, cx: &mut Context<Shell>) -> Div {
    let id = id.to_owned();
    div().flex_none().w(tokens::SEND_BUTTON_WIDTH).child(
        Button::new("request-cancel", "Cancel")
            .secondary()
            .icon(ui::icon(IconName::Loader))
            .kbd("Esc")
            .w_full()
            .on_click(cx.listener(move |this, _event, _window, cx| this.cancel(&id, cx))),
    )
}

/// The method chip, element id `request-method`, showing the draft's verb, else the file's.
/// A click opens the method menu under it; while the custom verb field of this request is
/// open, the field takes the chip's place.
// bolt gap: drawn as `bolt::controls::SelectTrigger` draws its field, whose label takes no
// color, weight or face, while the verb is bold mono in its method's hue.
fn method(
    id: &str,
    state: &RequestState,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    if let Some(edit) = shell
        .cell_edit
        .as_ref()
        .filter(|_| shell.method_edit_of(id))
    {
        return editing_method(edit.input.clone(), cx);
    }
    let colors = bolt::colors(cx);
    let current = shell
        .displayed_request(id, &state.inspection.request.request)
        .method
        .clone();
    let verb = current.to_string();
    let request = id.to_owned();
    region("request-method")
        .role(Role::Button)
        .aria_label(verb.clone())
        .relative()
        .flex_none()
        .w(tokens::METHOD_CHIP_WIDTH)
        .h(Fields::HEIGHT)
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.raised)
                .border(colors.line_2),
        )
        .child(
            div()
                .flex_1()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(Fields::TEXT)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(&verb, cx))
                .child(verb),
        )
        .child(
            Icon::new(ui::icon(IconName::ChevronDown))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        )
        .cursor_pointer()
        .on_click(cx.listener(|this, _event, window, cx| {
            this.toggle_method_menu(window, cx);
        }))
        .when(shell.method_menu_open, |anchor| {
            anchor.child(method_menu::render(&request, &current, shell, window, cx))
        })
}

/// The custom verb field, element id `request-method-input`, over the input `input`: Enter
/// commits it, Escape discards it and Ctrl+Enter sends.
fn editing_method(input: Entity<InputState>, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    region("request-method-input")
        .role(Role::TextInput)
        .relative()
        .flex_none()
        .w(tokens::METHOD_CHIP_WIDTH)
        .h(Fields::HEIGHT)
        .flex()
        .items_center()
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            GpuiInput::new(&input)
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .flex_1()
                .min_w_0()
                .text_size(Fields::TEXT)
                .font_family(bolt::fonts::editor_family(cx))
                .text_color(colors.strong),
        )
}

/// The URL field, element id `request-url`: the URL as written with the params that are sent
/// appended, styled by its references, in `request-url-text`, which a click swaps for the
/// editable field `request-url-input`. While the field of this request is open, it takes the
/// URL's place and takes Enter and Escape while focused.
fn url(id: &str, state: &RequestState, shell: &Shell, cx: &mut Context<Shell>) -> Stateful<Div> {
    if let Some(edit) = shell.url_edit.as_ref().filter(|edit| edit.request_id == id) {
        return editing_url(edit.input.clone(), cx);
    }
    let url = shell.shown_url(id);
    let request = id.to_owned();
    url_field(cx)
        .role(Role::Label)
        .aria_label(url.clone())
        .overflow_hidden()
        .whitespace_nowrap()
        .cursor_text()
        .child(
            region("request-url-text")
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .children(spans::render(&state.inspection, &url, cx)),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.start_url_edit(&request, window, cx);
        }))
}

/// The box of the URL, element id `request-url`: a field filling the line, in the data face.
fn url_field(cx: &Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    ui::role(region("request-url"), TextVariant::Code, cx)
        .relative()
        .flex_1()
        .min_w_0()
        .h(Fields::HEIGHT)
        .flex()
        .items_center()
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .text_color(colors.strong)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
}

/// The editable URL field, element id `request-url-input`, over the input `input`.
fn editing_url(input: Entity<InputState>, cx: &mut Context<Shell>) -> Stateful<Div> {
    url_field(cx)
        .role(Role::TextInput)
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends with the field open; Enter commits it and sends.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_url_edit_and_send(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            this.discard_url_edit(window, cx);
        }))
        // The frameless input keeps the URL's text size and the field's padding, so the text
        // stays where the URL shows it.
        .child(
            Input::new(&input)
                .id("request-url-input")
                .appearance(false)
                .padding(Pixels::ZERO, Pixels::ZERO)
                .text_size(TextVariant::Code.density_size(cx))
                .w_full(),
        )
}

/// The Send button, element id `request-send`, which sends the request `id`.
fn send(id: &str, cx: &mut Context<Shell>) -> Div {
    let id = id.to_owned();
    div().flex_none().w(tokens::SEND_BUTTON_WIDTH).child(
        Button::new("request-send", "Send")
            .primary()
            .icon(ui::icon(IconName::Send))
            .kbd("Ctrl ↵")
            .w_full()
            .on_click(cx.listener(move |this, _event, window, cx| this.send(&id, window, cx))),
    )
}
