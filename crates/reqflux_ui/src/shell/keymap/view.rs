//! The App > Keymap section of the settings page (v1-features S7, feedback-nov3 S15,
//! design/app-settings/04-keymap-page.html), laid out as DBFlux's Keyboard shortcuts page: a
//! filter (`keymap-filter`) and a context select (`keymap-context`), then the registry's
//! commands grouped by context under a header that folds (`keymap-group-<context>`). A row
//! shows its chord as one keycap per part (`keymap-chord-<slug>`), its label and a pencil
//! (`keymap-record-<slug>`) that captures the next chord; a click on the keycaps does the same.
//! A rebound command carries `REBOUND` and a Clear button, the conflict banner stands above
//! the list, and the `[keymap]` block of config.toml is a collapsed, bounded, read-only
//! section at the foot. The capturing row draws bolt's `KeyCapture` in its key column.

pub(in crate::shell) mod list;

use bolt::controls::{Button, SearchField};
use bolt::primitives::{Badge, BadgeTone, BannerBlock, BannerVariant, Icon, Kbd, TextVariant};
use bolt::tokens::{Borders, FontSizes, GridMetrics, HeaderMetrics, Spacing};
use gpui::{
    AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, Stateful, Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::super::commands::{self, effective_key, key_label};
use super::super::request::ui;
use super::super::{Shell, region};
use super::Conflict;
use super::holder_of;
use crate::tokens::KEYMAP_KEY_WIDTH as KEY_COLUMN;
use list::{Group, chord_parts, group_id, groups};

/// A region with a dynamic id, which the UI automation tree and the tests find by `id`.
fn region_of(id: String) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector.clone())
}

/// The rows of the grid the `[keymap]` block of config.toml is as tall as at most, before it
/// scrolls.
const CONFIG_MAX_ROWS: f32 = 5.0;

/// App > Keymap: the filter row, the groups of rows, the conflict banner and the `[keymap]`
/// block, under the section title `app_settings_view` draws.
pub(crate) fn content(shell: &Shell, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let editable = shell.settings.app.editable();
    let list = shell.settings.keymap_list.as_ref();
    let filter = list.map(|list| list.filter_text(cx)).unwrap_or_default();
    let context = list.and_then(|list| list.context());
    let narrowed = !filter.is_empty() || context.is_some();
    let groups = {
        let key = |command: &commands::Command| effective_key(cx, command.id);
        groups(&filter, context, &key)
    };
    let toolbar = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .pt(Spacing::MD)
        .pb(Spacing::SM)
        .children(list.map(|list| {
            SearchField::new("keymap-filter", &list.filter)
                .icon(ui::icon(IconName::Search))
                .aria_label("Filter by command or key")
        }))
        .children(list.map(|list| div().flex_none().w(KEY_COLUMN).child(list.context.clone())))
        .when(list.is_none(), |toolbar| toolbar.child(div().flex_1()))
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
    let sections: Vec<AnyElement> = groups
        .iter()
        .map(|group| {
            let open = list.is_none_or(|list| list.is_open(group.context, narrowed));
            group_section(shell, group, open, cx)
        })
        .collect();
    let empty = sections.is_empty().then(|| {
        ui::role(region("keymap-empty"), TextVariant::Caption, cx)
            .aria_label("No command matches the filter")
            .py(Spacing::LG)
            .text_color(colors.muted)
            .child("No command matches the filter")
    });
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(toolbar)
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
                .children(sections)
                .children(empty),
        )
        .child(keymap_config(shell, cx))
}

/// One context, `keymap-group-<context>`: the header with its chevron, name and count, and
/// under it the rows while it is open.
fn group_section(shell: &Shell, group: &Group, open: bool, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let context = group.context;
    let count = match group.commands.len() {
        1 => "1 binding".to_owned(),
        count => format!("{count} bindings"),
    };
    let header = region_of(format!("keymap-group-{}", group_id(context)))
        .role(Role::Button)
        .aria_label(format!(
            "{context}, {count}, {}",
            if open { "open" } else { "folded" }
        ))
        .cursor_pointer()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .mt(Spacing::SM)
        .py(Spacing::SM)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            Icon::new(ui::icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }))
            .size(HeaderMetrics::LABEL_ICON)
            .color(colors.muted),
        )
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(context),
        )
        .child(ui::role(div(), TextVariant::Caption, cx).child(count))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.toggle_keymap_group(context, cx);
        }));
    div()
        .flex_none()
        .flex()
        .flex_col()
        .child(header)
        .children(
            open.then(|| {
                group
                    .commands
                    .iter()
                    .map(|command| keymap_row(shell, command, cx))
                    .collect::<Vec<_>>()
            })
            .into_iter()
            .flatten(),
        )
        .into_any_element()
}

/// One row of the registry, `keymap-row-<slug>`: the chord as keycaps, the command with its
/// pencil, and its status: `REBOUND` with a Clear button, the capture, or the command that
/// took its key.
fn keymap_row(
    shell: &Shell,
    command: &'static commands::Command,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let slug = command.name;
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let capturing = shell.settings.keymap.capture == Some(command.id);
    let editable = shell.settings.app.editable();
    let custom = shell
        .settings
        .app
        .config()
        .is_some_and(|config| config.keymap.get(slug).is_some());
    let key = effective_key(cx, command.id);
    let field = shell.settings.keymap.field.clone().filter(|_| capturing);
    let key_cell: AnyElement = if let Some(field) = field {
        field.into_any_element()
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
            .child(Kbd::chord(chord_parts(&key)).id(format!("keymap-chord-{slug}")))
            .into_any_element()
    };
    let pencil = Button::new(
        ElementId::Name(format!("keymap-record-{slug}").into()),
        format!("Rebind {}", command.label),
    )
    .ghost()
    .inline()
    .icon_only()
    .icon(ui::icon(IconName::Pencil))
    .disabled(!editable)
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.start_keymap_capture(command.id, window, cx);
    }));
    let status: Option<AnyElement> = if capturing {
        Some(
            ui::role(div(), TextVariant::Caption, cx)
                .text_color(tint)
                .child("capturing")
                .into_any_element(),
        )
    } else if key.is_empty() {
        Some(
            ui::role(div(), TextVariant::Caption, cx)
                .text_color(colors.warning)
                .child(format!(
                    "taken by {}",
                    taken_by(shell, slug, cx).unwrap_or("another command")
                ))
                .into_any_element(),
        )
    } else if custom {
        Some(
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
                    .disabled(!editable)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.clear_keymap_binding(command.id, cx);
                    })),
                )
                .into_any_element(),
        )
    } else {
        None
    };
    region_of(format!("keymap-row-{slug}"))
        .role(Role::Row)
        .aria_label(format!("{} {}", command.label, key_label(&key)))
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .pl(Spacing::XL)
        .py(Spacing::SM)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(div().flex_none().w(KEY_COLUMN).child(key_cell))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.strong)
                        .child(command.label),
                )
                .child(pencil),
        )
        .children(status.map(|status| {
            div()
                .flex_none()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(status)
        }))
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

/// The `[keymap]` block of config.toml, read-only: only the bindings that differ from the
/// defaults live there. A header (`keymap-config-toggle`) opens it into a bounded block
/// (`keymap-config`) that scrolls; with no custom binding there is only a line
/// (`keymap-config-empty`).
fn keymap_config(shell: &Shell, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let lines: Vec<String> = shell
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
        .unwrap_or_default();
    let section = div()
        .flex_none()
        .flex()
        .flex_col()
        .pt(Spacing::SM)
        .border_t(Borders::THIN)
        .border_color(colors.line);
    if lines.is_empty() {
        return section.child(
            ui::role(region("keymap-config-empty"), TextVariant::Caption, cx)
                .aria_label("No custom bindings in config.toml")
                .py(Spacing::SM)
                .child("No custom bindings in config.toml"),
        );
    }
    let open = shell
        .settings
        .keymap_list
        .as_ref()
        .is_some_and(|list| list.config_open);
    let count = lines.len() - 1;
    let text = lines.join("\n");
    let summary = format!(
        "config.toml \u{b7} [keymap] \u{b7} {count} custom {}, read-only",
        if count == 1 { "binding" } else { "bindings" }
    );
    let toggle = region("keymap-config-toggle")
        .role(Role::Button)
        .aria_label(summary.clone())
        .cursor_pointer()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .py(Spacing::SM)
        .child(
            Icon::new(ui::icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }))
            .size(HeaderMetrics::LABEL_ICON)
            .color(colors.muted),
        )
        .child(ui::role(div(), TextVariant::Caption, cx).child(summary))
        .on_click(cx.listener(|this, _event, _window, cx| this.toggle_keymap_config(cx)));
    section.child(toggle).children(open.then(|| {
        region("keymap-config")
            .role(Role::Code)
            .aria_label(text.clone())
            .flex_none()
            .max_h(GridMetrics::ROW_HEIGHT * CONFIG_MAX_ROWS)
            .overflow_y_scroll()
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .py(Spacing::SM)
            .mb(Spacing::SM)
            .bg(colors.hover)
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(FontSizes::XS)
            .text_color(colors.muted)
            .child(text)
    }))
}
