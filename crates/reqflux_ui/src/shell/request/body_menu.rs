//! The language menu under the language select of a raw body: one row per language.
//! Clicking a row switches the draft's raw body to it, keeping the text; Escape or a click
//! outside closes the menu.

use gpui::{AnyElement, Context, Window};
use reqflux_core::model::RawLanguage;

use super::option_menu;
use crate::shell::Shell;
use crate::shell::body_edit::RAW_LANGUAGES;

/// The open menu for the request `request_id` whose raw body is in `current`.
pub(super) fn render(
    request_id: &str,
    current: RawLanguage,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let options: Vec<_> = RAW_LANGUAGES
        .into_iter()
        .map(|(id, name, language)| {
            let request = request_id.to_owned();
            option_menu::option(
                format!("body-language-{id}"),
                name.to_owned(),
                current == language,
                move |this, window, cx| this.set_raw_language(&request, language, window, cx),
                cx,
            )
        })
        .collect();
    option_menu::render(
        ("body-language-menu", "body-language-menu-backdrop"),
        &shell.body_menu_focus,
        options,
        Shell::close_body_menu,
        window,
        cx,
    )
}
