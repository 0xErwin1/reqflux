//! The method menu under the method chip: one row per standard verb and `Custom…`. Clicking
//! a verb puts it in the draft; `Custom…` opens the custom verb field; Escape or a click
//! outside closes the menu.

use gpui::{AnyElement, Context, Window};
use reqflux_core::model::Method;

use super::menu;
use crate::shell::Shell;
use crate::shell::method_edit::VERBS;

/// The open menu for the request `request_id` whose verb is `current`.
pub(super) fn render(
    request_id: &str,
    current: &Method,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let mut options: Vec<_> = VERBS
        .into_iter()
        .map(|verb| {
            let request = request_id.to_owned();
            let name = verb.to_string();
            menu::option(
                format!("method-{}", name.to_lowercase()),
                name,
                &verb == current,
                move |this, window, cx| this.pick_method(&request, verb.clone(), window, cx),
                cx,
            )
        })
        .collect();
    let request = request_id.to_owned();
    options.push(menu::option(
        "method-custom".to_owned(),
        "Custom…".to_owned(),
        !VERBS.contains(current),
        move |this, window, cx| this.start_method_edit(&request, window, cx),
        cx,
    ));
    menu::render(
        ("method-menu", "method-menu-backdrop"),
        &shell.method_menu_focus,
        options,
        Shell::close_method_menu,
        window,
        cx,
    )
}
