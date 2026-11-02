//! The App > Keymap section of the settings page (v1-features S7,
//! design/layouts/01 D3): every command of the registry with its effective key and its
//! context, a click on a key capturing the next chord, the conflict banner, `Reset to
//! defaults` and the read-only `[keymap]` block of config.toml.
//!
//! bolt gap: no bolt key-capture field exists for "press the new keys" while rebinding;
//! the capturing row draws the capture as a hint in the key column instead.

use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, BannerBlock, BannerVariant, Kbd, TextVariant};
use bolt::tokens::{Borders, HeaderMetrics, Spacing};
use gpui::{
    AnyElement, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    Stateful, Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::super::commands::{self, effective_key, key_label};
use super::super::request::ui;
use super::super::{Shell, region, text_region};
use super::Conflict;
use super::holder_of;
use crate::tokens::KEYMAP_KEY_WIDTH as KEY_COLUMN;

/// A region with a dynamic id, which the UI automation tree and the tests find by `id`.
fn region_of(id: String) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector.clone())
}

/// App > Keymap: the registry rows, the capture, the conflict banner and the read-only
/// `[keymap]` block, under the section title `app_settings_view` draws.
pub(crate) fn content(shell: &Shell, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let editable = shell.settings.app.editable();
    let header = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .pb(Spacing::SM)
        .child(
            ui::role(div(), TextVariant::Label, cx)
                .flex_1()
                .child("COMMANDS"),
        )
        .child(
            Button::new("keymap-reset", "Reset to defaults")
                .secondary()
                .inline()
                .icon(ui::icon(IconName::RotateCcw))
                .disabled(!editable)
                .on_click(cx.listener(|this, _event, _window, cx| this.reset_keymap(cx))),
        );
    let banner = shell
        .settings
        .keymap
        .conflict
        .as_ref()
        .map(|conflict| conflict_banner(conflict, cx));
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .children(banner)
        .child(
            region("keymap-rows")
                .role(Role::Table)
                .aria_label("Commands and keys")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .border_b(Borders::THIN)
                .border_color(colors.line)
                .children(
                    commands::COMMANDS
                        .iter()
                        .map(|command| keymap_row(shell, command, cx)),
                ),
        )
        .child(keymap_config(shell, cx))
}

/// One row of the registry, `keymap-row-<slug>`: the command, its effective key, its
/// context and its status — `default`, `REBOUND` with a Clear button, the capture, or
/// the command that took its key.
fn keymap_row(
    shell: &Shell,
    command: &'static commands::Command,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let slug = command.name;
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let capturing = shell.settings.keymap.capture == Some(command.id);
    let custom = shell
        .settings
        .app
        .config()
        .is_some_and(|config| config.keymap.get(slug).is_some());
    let key = effective_key(cx, command.id);
    let key_cell: AnyElement = if capturing {
        ui::role(text_region("keymap-capture-hint"), TextVariant::Caption, cx)
            .text_color(tint)
            .child("listening… press the new keys, Esc cancels")
            .into_any_element()
    } else if key.is_empty() {
        ui::role(
            div().id(ElementId::Name(format!("keymap-key-{slug}").into())),
            TextVariant::Body,
            cx,
        )
        .text_color(colors.muted)
        .child("\u{2013}")
        .into_any_element()
    } else {
        region_of(format!("keymap-key-{slug}"))
            .role(Role::Button)
            .aria_label(format!("Rebind {}", key_label(&key)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.start_keymap_capture(command.id, window, cx);
            }))
            .child(Kbd::new(key_label(&key)))
            .into_any_element()
    };
    let status: AnyElement = if capturing {
        ui::role(div(), TextVariant::Caption, cx)
            .text_color(tint)
            .child("capturing")
            .into_any_element()
    } else if key.is_empty() {
        ui::role(div(), TextVariant::Caption, cx)
            .text_color(colors.warning)
            .child(format!(
                "taken by {}",
                taken_by(shell, slug, cx).unwrap_or("another command")
            ))
            .into_any_element()
    } else if custom {
        div()
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(Badge::new("REBOUND", BadgeTone::Accent))
            .child(
                Button::new(
                    ElementId::Name(format!("keymap-clear-{slug}").into()),
                    format!("Clear {}", command.label),
                )
                .ghost()
                .inline()
                .icon(ui::icon(IconName::Undo2))
                .disabled(!shell.settings.app.editable())
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.clear_keymap_binding(command.id, cx);
                })),
            )
            .into_any_element()
    } else {
        ui::role(div(), TextVariant::Caption, cx)
            .text_color(colors.muted)
            .child("default")
            .into_any_element()
    };
    region_of(format!("keymap-row-{slug}"))
        .role(Role::Row)
        .aria_label(format!("{} {}", command.label, key_label(&key)))
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .py(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(colors.strong)
                .child(command.label),
        )
        .child(div().flex_none().w(KEY_COLUMN).child(key_cell))
        .child(
            ui::role(div(), TextVariant::Caption, cx)
                .text_color(colors.muted)
                .child(command.context),
        )
        .child(
            div()
                .flex_none()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(status),
        )
}

/// The label of the command that holds the key `slug` lost, as the resolution left the
/// keymap; `None` when the page cannot read the file.
fn taken_by(shell: &Shell, slug: &str, cx: &Context<Shell>) -> Option<&'static str> {
    let command = commands::COMMANDS
        .iter()
        .find(|command| command.name == slug)?;
    let config = shell.settings.app.config()?;
    let chord = config.keymap.get(slug).unwrap_or(command.key);
    let holder = holder_of(chord, command.id, cx)?;
    commands::command(holder).map(|command| command.label)
}

/// The conflict banner, `keymap-conflict`: the chord a capture caught while another
/// command holds it, with Keep mine and Cancel.
fn conflict_banner(conflict: &Conflict, cx: &mut Context<Shell>) -> AnyElement {
    let other =
        commands::command(conflict.other).map_or("another command", |command| command.label);
    let context = commands::command(conflict.id).map_or("shell", |command| command.context);
    let title = format!(
        "{} is already bound to {other} in {context}.",
        key_label(&conflict.chord)
    );
    let body = format!("Pick another key, or keep yours and {other} loses it.");
    let actions = div()
        .flex()
        .gap(Spacing::SM)
        .child(
            Button::new("keymap-keep", "Keep mine")
                .secondary()
                .inline()
                .on_click(cx.listener(|this, _event, _window, cx| this.keep_keymap_binding(cx))),
        )
        .child(
            Button::new("keymap-cancel", "Cancel")
                .secondary()
                .inline()
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.cancel_keymap_capture(window, cx);
                })),
        );
    region("keymap-conflict")
        .role(Role::Alert)
        .aria_label(format!("{title} {body}"))
        .flex_none()
        .py(Spacing::SM)
        .child(
            BannerBlock::new(BannerVariant::Danger, title)
                .default_icon(ui::icon(IconName::TriangleAlert))
                .with_body(body)
                .with_actions(actions),
        )
        .into_any_element()
}

/// The read-only `[keymap]` block of config.toml, `keymap-config`: only the bindings that
/// differ from the defaults are kept there, and Reset removes the table.
fn keymap_config(shell: &Shell, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let lines = shell
        .settings
        .app
        .config()
        .map(|config| &config.keymap)
        .filter(|keymap| !keymap.is_empty())
        .map(|keymap| {
            let mut lines = vec!["[keymap]".to_owned()];
            lines.extend(
                keymap
                    .iter()
                    .map(|(slug, chord)| format!("\"{slug}\" = \"{chord}\"")),
            );
            lines
        })
        .unwrap_or_else(|| vec!["No custom bindings.".to_owned()]);
    let text = lines.join("\n");
    div()
        .flex_none()
        .flex()
        .flex_col()
        .pt(HeaderMetrics::SECTION_GAP)
        .child(
            ui::role(div(), TextVariant::Label, cx)
                .pb(Spacing::XS)
                .child("CONFIG.TOML · KEYMAP BLOCK, READ-ONLY"),
        )
        .child(
            region("keymap-config")
                .role(Role::Code)
                .aria_label(text.clone())
                .flex_none()
                .p(Spacing::MD)
                .bg(colors.background)
                .border_1()
                .border_color(colors.line)
                .font_family(bolt::fonts::editor_family(cx))
                .child(text),
        )
}
