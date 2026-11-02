//! The Auth pane menus: the type menu under the type row, one row per auth type, and the
//! digest Algorithm and OAuth 2.0 Grant type menus under their selects. Clicking a row
//! writes it to the draft; Escape or a click outside closes the menu.

use gpui::{AnyElement, Context, Window};
use reqflux_core::model::{Auth, DigestAlgorithm, OAuth2Grant};

use super::menu;
use super::pane_model::{ALGORITHMS, GRANTS, algorithm_name, auth_type, grant_name};
use crate::shell::Shell;
use crate::shell::auth_edit::{AUTH_KINDS, empty_auth};

/// The open type menu for the request `request_id` whose auth is `auth`.
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

/// The open Algorithm menu of the digest auth of `request_id`, `auth-algorithm-<id>` rows.
pub(super) fn algorithm(
    request_id: &str,
    current: DigestAlgorithm,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let options: Vec<_> = ALGORITHMS
        .into_iter()
        .map(|(algorithm, id)| {
            let request = request_id.to_owned();
            menu::option(
                format!("auth-algorithm-{id}"),
                algorithm_name(algorithm).to_owned(),
                algorithm == current,
                move |this, window, cx| this.set_digest_algorithm(&request, algorithm, window, cx),
                cx,
            )
        })
        .collect();
    menu::render(
        ("auth-algorithm-menu", "auth-algorithm-menu-backdrop"),
        &shell.auth_menu_focus,
        options,
        Shell::close_auth_menu,
        window,
        cx,
    )
}

/// The open Grant type menu of the OAuth 2.0 auth of `request_id`, `auth-grant-<id>` rows.
pub(super) fn grant(
    request_id: &str,
    current: OAuth2Grant,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let options: Vec<_> = GRANTS
        .into_iter()
        .map(|(grant, id)| {
            let request = request_id.to_owned();
            menu::option(
                format!("auth-grant-{id}"),
                grant_name(grant).to_owned(),
                grant == current,
                move |this, window, cx| this.set_oauth_grant(&request, grant, window, cx),
                cx,
            )
        })
        .collect();
    menu::render(
        ("auth-grant-menu", "auth-grant-menu-backdrop"),
        &shell.auth_menu_focus,
        options,
        Shell::close_auth_menu,
        window,
        cx,
    )
}
