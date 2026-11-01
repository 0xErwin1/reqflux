//! The pane menus of the request (method, auth type, raw language): `bolt`'s menu frame and
//! menu rows, anchored under their trigger above the islands, over a backdrop that closes the
//! menu on a click outside. Escape closes it too.

use bolt::composites::{MenuItem, menu_frame, menu_row};
use gpui::{
    AnyElement, Context, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Point, Role, Styled, Window, anchored, deferred, div, prelude::*,
};

use crate::shell::region;
use crate::tokens;

/// The option row `id` named `name`, highlighted when it is the `selected` one; a click runs
/// `pick`.
pub(super) fn option<V: 'static>(
    id: String,
    name: String,
    selected: bool,
    pick: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let selector = id.clone();
    menu_row(id, &MenuItem::new(name.clone()), selected, cx)
        .debug_selector(move || selector)
        .aria_label(name)
        .aria_selected(selected)
        .on_click(cx.listener(move |this, _event, window, cx| pick(this, window, cx)))
        .into_any_element()
}

/// The open menu `id` of `options`, focused through `focus`, anchored under its trigger;
/// Escape and a click on its `backdrop` run `close`.
pub(super) fn render<V: 'static>(
    (id, backdrop): (&'static str, &'static str),
    focus: &FocusHandle,
    options: Vec<AnyElement>,
    close: fn(&mut V, &mut Window, &mut Context<V>),
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let menu = menu_frame(cx)
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(Role::Menu)
        .track_focus(focus)
        .occlude()
        .w(tokens::AUTH_MENU_WIDTH)
        .children(options)
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                close(this, window, cx);
            }
        }));
    let backdrop = region(backdrop)
        .occlude()
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| close(this, window, cx)),
        );
    div()
        .absolute()
        .top_full()
        .left_0()
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(deferred(anchored().child(menu)).with_priority(2))
        .into_any_element()
}
