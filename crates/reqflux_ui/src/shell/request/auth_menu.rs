//! The Auth pane type menu under the type row: one row per auth type. Clicking a row
//! switches the draft's auth to it; Escape or a click outside closes the menu.

use gpui::{AnyElement, Context, Window};
use reqflux_core::model::Auth;

use super::menu;
use super::pane_model::auth_type;
use crate::shell::Shell;
use crate::shell::auth_edit::{AUTH_KINDS, empty_auth};

/// The open menu for the request `request_id` whose auth is `auth`.
pub(super) fn render(
    request_id: &str,
    auth: &Auth,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let options: Vec<_> = AUTH_KINDS
        .into_iter()
        .map(|kind| {
            let name = empty_auth(kind)
                .as_ref()
                .map_or(kind, |auth| auth_type(auth))
                .to_owned();
            let request = request_id.to_owned();
            menu::option(
                format!("auth-type-{kind}"),
                name,
                auth.kind() == kind,
                move |this, window, cx| this.set_auth_type(&request, kind, window, cx),
                cx,
            )
        })
        .collect();
    menu::render(
        ("auth-type-menu", "auth-type-menu-backdrop"),
        &shell.auth_menu_focus,
        options,
        Shell::close_auth_menu,
        window,
        cx,
    )
}
