//! The Scripts pane, as artboard request-panes/01 frame B draws it: a note on which script
//! runs, then the pre-request and test scripts stacked, each in an editor box with a header
//! strip (its name, its sidecar file and a delete button) over the script with line numbers,
//! the open editor, or an empty state whose button starts one. Every edit lands in the
//! request's draft; see [`crate::shell::scripts_edit`].

use bolt::chamfer::Chamfer;
use bolt::controls::IconButton;
use bolt::tokens::{border, chrome, cut, family, field, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*,
};
use gpui_component::input::{Enter, Escape, Textarea};
use reqflux_engine::Inspection;

use super::code;
use crate::shell::Shell;
use crate::shell::rows::part;
use crate::shell::scripts_edit::ScriptSlot;
use crate::shell::text as text_role;
use crate::tokens;

/// The heading, the empty-state title and text, and the empty state's button of `slot`.
fn copy(slot: ScriptSlot) -> (&'static str, &'static str, &'static str, &'static str) {
    match slot {
        ScriptSlot::PreRequest => (
            "PRE-REQUEST",
            "No pre-request script",
            "A pre-request script runs before the request is sent and can set variables.",
            "Write a script",
        ),
        ScriptSlot::Test => (
            "TESTS",
            "No test script",
            "Tests run after the response arrives and show up in the Tests tab of the \
             response.",
            "Write a test",
        ),
    }
}

/// The pane of the request `id`: its draft's scripts when it has edited ones, else its
/// sidecars'.
pub(super) fn render(
    id: &str,
    inspection: &Inspection,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let saved = &inspection.request.scripts;
    let shown = shell.displayed_scripts(id, saved).clone();
    let muted = bolt::colors(cx).muted;
    let note = part("request-script-note".to_owned())
        .role(Role::Label)
        .flex()
        .items_center()
        .gap(space::HALF)
        .h(chrome::CONTROL)
        .text_size(tokens::NOTICE_TEXT)
        .text_color(muted)
        .child(Icon::new(IconName::Info).size(icon::CHIP).color(muted))
        .child("A script here replaces the one its folder or collection sets.");
    let editors = ScriptSlot::ALL.map(|slot| {
        let file = slot
            .text(saved)
            .map(|_| format!("{}{}", inspection.request.slug, slot.suffix()));
        editor_box(id, inspection, slot, slot.text(&shown), file, shell, cx)
    });
    vec![
        div()
            .flex()
            .flex_col()
            .gap(space::SM)
            .pl(space::MD)
            .pr(space::MD)
            .child(note)
            .children(editors)
            .into_any_element(),
    ]
}

/// The editor box `request-script-<slot>-editor` of the script `slot` showing `text`, with
/// its sidecar `file` when one is saved.
fn editor_box(
    id: &str,
    inspection: &Inspection,
    slot: ScriptSlot,
    text: Option<&str>,
    file: Option<String>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = *bolt::colors(cx);
    let accent = bolt::brand(cx).accent_tint;
    let editing = shell
        .script_edit
        .as_ref()
        .filter(|edit| edit.request_id == id && edit.slot == slot)
        .map(|edit| edit.input.clone());
    let (heading, ..) = copy(slot);
    let slot_id = slot.id();
    let delete = text.map(|_| {
        let request = id.to_owned();
        IconButton::new(
            format!("request-script-{slot_id}-delete"),
            IconName::Trash,
            format!("Delete {} script", heading.to_lowercase()),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.delete_script(&request, slot, window, cx);
        }))
    });
    let header = part(format!("request-script-{slot_id}-header"))
        .flex()
        .items_center()
        .gap(space::SM)
        .h(tokens::GRID_HEADER)
        .pl(space::MD)
        .pr(space::HALF)
        .border_b(border::THIN)
        .border_color(colors.line)
        .child(
            text_role::role(div(), text::LABEL, cx)
                .text_color(colors.foreground)
                .child(heading),
        )
        .child(
            div()
                .font_family(family::MONO)
                .text_size(tokens::RESPONSE_META_TEXT)
                .text_color(colors.muted)
                .child(file.unwrap_or_else(|| "no file yet".to_owned())),
        )
        .child(div().flex_1())
        .children(delete);
    let body = match (editing, text) {
        (Some(input), _) => div()
            .pl(space::MD)
            .pr(space::MD)
            .py(space::SM)
            .capture_action(cx.listener(|this, action: &Enter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.send_active(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.close_script_edit(window, cx);
            }))
            .child(
                part(format!("request-script-{slot_id}-input")).child(
                    Textarea::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .text_size(tokens::GRID_TEXT)
                        .font_family(family::MONO)
                        .text_color(colors.foreground),
                ),
            )
            .into_any_element(),
        (None, Some(text)) => {
            let request = id.to_owned();
            part(format!("request-script-{slot_id}-code"))
                .py(space::SM)
                .cursor_text()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.start_script_edit(&request, slot, window, cx);
                }))
                .child(code::block(
                    inspection,
                    format!("request-script-{slot_id}"),
                    &format!("request-script-{slot_id}-line"),
                    text,
                    cx,
                ))
                .into_any_element()
        }
        (None, None) => empty_state(id, slot, cx),
    };
    let focused = shell.script_edit_of(id, slot);
    part(format!("request-script-{slot_id}-editor"))
        .relative()
        .flex()
        .flex_col()
        .child(if focused {
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(accent, border::FOCUS_RING)
        } else {
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN)
        })
        .child(header)
        .child(body)
        .into_any_element()
}

/// The empty state `request-script-<slot>-empty` of the missing script `slot`, whose
/// button `request-script-<slot>-create` opens an empty editor.
fn empty_state(id: &str, slot: ScriptSlot, cx: &mut Context<Shell>) -> AnyElement {
    let colors = *bolt::colors(cx);
    let accent = bolt::brand(cx).accent_tint;
    let (_, title, hint, action) = copy(slot);
    let slot_id = slot.id();
    let icon_name = match slot {
        ScriptSlot::PreRequest => IconName::Code,
        ScriptSlot::Test => IconName::CircleCheckBig,
    };
    let request = id.to_owned();
    part(format!("request-script-{slot_id}-empty"))
        .role(Role::Label)
        .aria_label(title)
        .flex()
        .flex_col()
        .items_center()
        .gap(space::SM)
        .p(space::LG)
        .child(Icon::new(icon_name).size(icon::RAIL).color(accent))
        .child(
            text_role::role(div(), text::BODY, cx)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.strong)
                .child(title),
        )
        .child(
            text_role::role(div(), text::CAPTION, cx)
                .text_center()
                .child(hint),
        )
        .child(
            primary_button(format!("request-script-{slot_id}-create"), action, cx).on_click(
                cx.listener(move |this, _event, window, cx| {
                    this.start_script_edit(&request, slot, window, cx);
                }),
            ),
        )
        .into_any_element()
}

/// An accent button `id` with the pencil icon and `label`.
fn primary_button(id: String, label: &'static str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let brand = bolt::brand(cx);
    part(id)
        .role(Role::Button)
        .aria_label(label)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::SM)
        .h(chrome::CONTROL)
        .pl(space::MD)
        .pr(space::MD)
        .text_size(field::FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(brand.on_accent)
        .cursor_pointer()
        .child(Chamfer::new(cut::CONTROL).fill(brand.accent))
        .child(
            Icon::new(IconName::Pencil)
                .size(icon::MD)
                .color(brand.on_accent),
        )
        .child(label)
}
