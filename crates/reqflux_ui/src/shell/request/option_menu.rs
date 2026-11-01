//! A context menu opened at the pointer: `bolt`'s menu frame and menu rows, drawn above the
//! islands over a backdrop that closes it on a click outside. Escape closes it too. The
//! context menus of a tree row and of an environment are this menu, opened by
//! [`render_at`]; the request's pane menus, anchored under their trigger, are
//! `request::menu`.

use bolt::composites::{MenuItem, menu_frame, menu_row};
use gpui::{
    AnyElement, Context, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Pixels, Point, Role, Styled, Window, anchored, deferred, div, prelude::*,
};

use crate::shell::region;
use crate::tokens;

/// The option row `id` named `name`, highlighted and checked when `selected`; a click runs
/// `pick`.
pub(in crate::shell) fn option<V: 'static>(
    id: String,
    name: String,
    selected: bool,
    pick: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    menu_row(
        id,
        &MenuItem::new(name.clone()).checked(selected),
        selected,
        cx,
    )
    .aria_label(name)
    .aria_selected(selected)
    .on_click(cx.listener(move |this, _event, window, cx| pick(this, window, cx)))
    .into_any_element()
}

/// The open context menu `id` of `rows` with its top-left corner at `position`, in window
/// coordinates, moved back inside the window when it would leave it. Opened at the pointer,
/// as DBFlux does: a right click passes the click point, a `…` button the click on it.
/// Escape and a click on its `backdrop` run `close`.
pub(in crate::shell) fn render_at<V: 'static>(
    (id, backdrop): (&'static str, &'static str),
    position: Point<Pixels>,
    focus: &FocusHandle,
    rows: Vec<AnyElement>,
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
        .children(rows)
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
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(
            deferred(anchored().position(position).snap_to_window().child(menu)).with_priority(2),
        )
        .into_any_element()
}
