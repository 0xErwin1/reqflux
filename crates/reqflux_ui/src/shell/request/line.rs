//! The request line: the method chip, which opens the method menu, the URL with its references styled, and
//! the Send button, which sends the request, or the Cancel button while it is in flight; both
//! have the same width, so the line does not move. Clicking the URL swaps it for an editable
//! field drawn in the same place, padding, font and height, so the text does not move.
//! Enter in the field commits the URL and sends the request.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::Keycap;
use bolt_legacy::tokens::{border, chrome, cut, family, icon, space};
use bolt_legacy::{Icon, IconName};
use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, Window, div,
    prelude::*,
};
use gpui_component::input::{Enter, Escape, Input, InputState};

use super::model::RequestState;
use super::{method_menu, spans};
use crate::shell::{Shell, region};
use crate::tokens::{self, method_color};

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
        cancel(id, cx).into_any_element()
    } else {
        send(id, cx).into_any_element()
    };
    region("request-line")
        .flex_none()
        .h(tokens::REQUEST_LINE)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .child(method(id, state, shell, window, cx))
        .child(url(id, state, shell, cx))
        .child(action)
}

/// The Cancel button shown while the request is in flight, element id `request-cancel`.
fn cancel(id: &str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    let brand = bolt_legacy::brand(cx);
    let id = id.to_owned();
    region("request-cancel")
        .role(Role::Button)
        .aria_label("Cancel")
        .relative()
        .flex_none()
        .w(tokens::SEND_BUTTON_WIDTH)
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .justify_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.raised)
                .edge(colors.line, border::THIN),
        )
        .child(
            Icon::new(IconName::Loader)
                .size(icon::MD)
                .color(brand.accent_tint),
        )
        .child("Cancel")
        .child(Keycap::new("request-cancel-keycap", "Esc"))
        .on_click(cx.listener(move |this, _event, _window, cx| this.cancel(&id, cx)))
}

/// The method chip, element id `request-method`, showing the draft's verb, else the file's.
/// A click opens the method menu under it; while the custom verb field of this request is
/// open, the field takes the chip's place.
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
    let colors = bolt_legacy::colors(cx);
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
        .h(chrome::CONTROL)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.raised)
                .edge(colors.line, border::THIN),
        )
        .child(
            div()
                .flex_1()
                .font_family(family::MONO)
                .text_size(tokens::REQUEST_LINE_TEXT)
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&verb, bolt_legacy::palette(cx)))
                .child(verb),
        )
        .child(
            Icon::new(IconName::ChevronDown)
                .size(icon::SM)
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
    let colors = bolt_legacy::colors(cx);
    region("request-method-input")
        .role(Role::TextInput)
        .relative()
        .flex_none()
        .w(tokens::METHOD_CHIP_WIDTH)
        .h(chrome::CONTROL)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
        .on_action(cx.listener(|this, action: &Enter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            Input::new(&input)
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .flex_1()
                .min_w_0()
                .text_size(tokens::REQUEST_LINE_TEXT)
                .font_family(family::MONO)
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
    let colors = bolt_legacy::colors(cx);
    let url = shell.shown_url(id);
    let request = id.to_owned();
    region("request-url")
        .role(Role::Label)
        .aria_label(url.clone())
        .relative()
        .flex_1()
        .min_w_0()
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(family::MONO)
        .text_size(tokens::URL_TEXT)
        .text_color(colors.strong)
        .cursor_text()
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
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

/// The editable URL field, element id `request-url-input`, over the input `input`.
fn editing_url(input: Entity<InputState>, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    region("request-url")
        .role(Role::TextInput)
        .relative()
        .flex_1()
        .min_w_0()
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .font_family(family::MONO)
        .text_size(tokens::URL_TEXT)
        .text_color(colors.strong)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
        .on_action(cx.listener(|this, action: &Enter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus:
            // it sends with the field open; Enter commits it and sends.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_url_edit_and_send(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.discard_url_edit(window, cx);
        }))
        .child(
            Input::new(&input)
                .id("request-url-input")
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .flex_1()
                .min_w_0()
                .text_size(tokens::URL_TEXT)
                .font_family(family::MONO)
                .text_color(colors.strong),
        )
}

/// The Send button, element id `request-send`, which sends the request `id`.
fn send(id: &str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let id = id.to_owned();
    let brand = bolt_legacy::brand(cx);
    region("request-send")
        .role(Role::Button)
        .aria_label("Send")
        .relative()
        .flex_none()
        .w(tokens::SEND_BUTTON_WIDTH)
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .justify_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(brand.on_accent)
        .child(Chamfer::new(cut::CONTROL).fill(brand.accent))
        .child(
            Icon::new(IconName::Send)
                .size(icon::MD)
                .color(brand.on_accent),
        )
        .child("Send")
        .child(accent_keycap())
        .on_click(cx.listener(move |this, _event, window, cx| this.send(&id, window, cx)))
}

/// The `Ctrl ↵` keycap on an accent button.
pub(crate) fn accent_keycap() -> Div {
    div()
        .relative()
        .pl(tokens::KEYCAP_PADDING_X)
        .pr(tokens::KEYCAP_PADDING_X)
        .py(tokens::KEYCAP_PADDING_Y)
        .font_family(family::MONO)
        .text_size(tokens::SMALL_MONO_TEXT)
        .font_weight(FontWeight::NORMAL)
        .text_color(tokens::keycap_on_accent_text())
        .child(Chamfer::new(cut::KEYCAP).fill(tokens::keycap_on_accent_fill()))
        .child("Ctrl ↵")
}
