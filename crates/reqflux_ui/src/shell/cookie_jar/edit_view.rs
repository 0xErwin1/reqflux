//! Drawing the Add cookie / Edit cookie dialog (design/network/01-settings-sections, States:
//! Add cookie, Edit cookie): `bolt`'s `Modal` `cookie-edit-dialog` with the fields
//! `cookie-edit-name`, `-value`, `-domain`, `-path`, `-expires`, the select
//! `cookie-edit-same-site`, the checkboxes `cookie-edit-session`, `-secure`, `-http-only` and
//! `-subdomains`, the engine's refusal `cookie-edit-error`, and `cookie-edit-cancel` and
//! `cookie-edit-save`.

use bolt::controls::{Button, Checkbox, Input};
use bolt::modals::{Modal, modal_field};
use bolt::tokens::{ModalMetrics, Spacing};
use gpui::{AnyElement, Context, Div, ElementId, Role, Window, div, prelude::*};
use gpui_kit_assets::IconName;

use super::edit::CookieFlag;
use super::view::overlay;
use crate::shell::request::ui;
use crate::shell::{Shell, region, text_region};

/// The dialog, while it is open.
pub(super) fn dialog(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let edit = shell.settings.cookies.edit.as_ref()?;
    let adding = edit.original.is_none();
    let title = if adding { "Add cookie" } else { "Edit cookie" };
    let save_label = if adding { "Add cookie" } else { "Save cookie" };
    let value_placeholder = if edit.keeps_hidden_value() {
        "Hidden. Leave empty to keep it"
    } else {
        "Value"
    };
    let input =
        |id: &'static str, label: &'static str, state, disabled: bool, cx: &Context<Shell>| {
            let field = region(id)
                .role(Role::TextInput)
                .aria_label(label)
                .child(Input::new(state).disabled(disabled));
            modal_field(label, field, cx).flex_1().min_w_0()
        };
    let row = |first: Div, second: Div| div().flex().gap(Spacing::MD).child(first).child(second);
    let value_field = {
        let field = region("cookie-edit-value")
            .role(Role::TextInput)
            .aria_label("Value")
            .child(Input::new(&edit.value).placeholder(value_placeholder));
        modal_field("Value", field, cx).flex_1().min_w_0()
    };
    let same_site = modal_field(
        "SameSite",
        region("cookie-edit-same-site-field")
            .role(Role::ComboBox)
            .aria_label("SameSite")
            .child(edit.same_site.clone()),
        cx,
    )
    .flex_1()
    .min_w_0();
    let expires = {
        let session = flag(
            "cookie-edit-session",
            "Session cookie",
            edit.session,
            CookieFlag::Session,
            cx,
        );
        let date = region("cookie-edit-expires")
            .role(Role::TextInput)
            .aria_label("Expires (UTC)")
            .child(Input::new(&edit.expires).disabled(edit.session));
        modal_field(
            "Expires (UTC)",
            div()
                .flex()
                .flex_col()
                .gap(Spacing::SM)
                .child(date)
                .child(session),
            cx,
        )
        .flex_1()
        .min_w_0()
    };
    let flags = div()
        .flex()
        .gap(Spacing::LG)
        .child(flag(
            "cookie-edit-secure",
            "Secure",
            edit.secure,
            CookieFlag::Secure,
            cx,
        ))
        .child(flag(
            "cookie-edit-http-only",
            "HttpOnly",
            edit.http_only,
            CookieFlag::HttpOnly,
            cx,
        ))
        .child(flag(
            "cookie-edit-subdomains",
            "Send to subdomains",
            edit.subdomains,
            CookieFlag::Subdomains,
            cx,
        ));
    let error = edit.error.clone().map(|message| {
        text_region("cookie-edit-error")
            .aria_label(message.clone())
            .text_color(bolt::colors(cx).danger)
            .child(message)
    });
    let body = region("cookie-edit-dialog")
        .role(Role::Dialog)
        .aria_label(title)
        .flex()
        .flex_col()
        .gap(Spacing::MD)
        .child(row(
            input("cookie-edit-name", "Name", &edit.name, false, cx),
            value_field,
        ))
        .child(row(
            input("cookie-edit-domain", "Domain", &edit.domain, false, cx),
            input("cookie-edit-path", "Path", &edit.path, false, cx),
        ))
        .child(row(expires, same_site))
        .child(flags)
        .children(error);
    let footer = div()
        .flex()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            Button::new("cookie-edit-cancel", "Cancel")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.cancel_cookie_edit(cx);
                })),
        )
        .child(
            Button::new("cookie-edit-save", save_label)
                .primary()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.save_cookie_edit(cx);
                })),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(title)
        .id("cookie-edit-backdrop")
        .icon(ui::icon(if adding {
            IconName::Plus
        } else {
            IconName::Pencil
        }))
        .focus_handle(shell.settings.cookies.edit_focus.handle())
        .body(body)
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.cancel_cookie_edit(cx)) {
                log::warn!("cookie edit close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.save_cookie_edit(cx)) {
                log::warn!("cookie edit save: {error}");
            }
        });
    Some(overlay(modal, window))
}

/// A checkbox of the dialog that sets `flag`.
fn flag(
    id: &'static str,
    label: &'static str,
    checked: bool,
    flag: CookieFlag,
    cx: &mut Context<Shell>,
) -> AnyElement {
    Checkbox::new(ElementId::Name(id.into()))
        .label(label)
        .checked(checked)
        .on_click(cx.listener(move |this, on: &bool, _window, cx| {
            this.set_cookie_flag(flag, *on, cx);
        }))
        .into_any_element()
}
