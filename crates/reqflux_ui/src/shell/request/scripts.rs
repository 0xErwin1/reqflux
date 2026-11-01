//! The Scripts pane, as artboard request-panes/01 frame B draws it: a note on which script
//! runs, then the pre-request and test scripts stacked, each in an editor box with a header
//! strip (its name, its sidecar file and a delete button) over the script with line numbers,
//! the open editor, or an empty state whose button starts one. The accent focus ring,
//! `request-script-<slot>-focus-ring`, shows only while the open editor has the focus. Every edit lands in the
//! request's draft; see [`crate::shell::scripts_edit`].

use bolt::composites::EmptyState;
use bolt::controls::{Button, InputEnter, InputEscape};
use bolt::primitives::{Chamfer, Icon, TextVariant};
use bolt::tokens::{Borders, ChamferCut, EditorMetrics, Fields, HeaderMetrics, Spacing};
use bolt_legacy::IconName;
use gpui::{AnyElement, Context, ParentElement, Role, Styled, div, prelude::*};
use gpui_component::input::Textarea;
use reqflux_engine::Inspection;

use super::body::controls::focus_ring;
use super::{code, ui};
use crate::shell::Shell;
use crate::shell::rows::part;
use crate::shell::scripts_edit::ScriptSlot;

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
    let note = ui::role(
        part("request-script-note".to_owned()),
        TextVariant::Caption,
        cx,
    )
    .role(Role::Label)
    .flex()
    .items_center()
    .gap(Spacing::XXS)
    .h(Fields::HEIGHT)
    .child(
        Icon::new(ui::icon(IconName::Info))
            .size(Fields::SEGMENT_ICON)
            .color(muted),
    )
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
            .gap(Spacing::SM)
            .pl(Spacing::MD)
            .pr(Spacing::MD)
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
    let colors = bolt::colors(cx);
    let editing = shell
        .script_edit
        .as_ref()
        .filter(|edit| edit.request_id == id && edit.slot == slot)
        .map(|edit| edit.input.clone());
    let (heading, ..) = copy(slot);
    let slot_id = slot.id();
    let delete = text.map(|_| {
        let request = id.to_owned();
        let delete_id = format!("request-script-{slot_id}-delete");
        Button::new(
            gpui::ElementId::Name(delete_id.into()),
            format!("Delete {} script", heading.to_lowercase()),
        )
        .ghost()
        .inline()
        .icon_only()
        .icon(ui::icon(IconName::Trash))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.delete_script(&request, slot, window, cx);
        }))
    });
    let header = part(format!("request-script-{slot_id}-header"))
        .flex()
        .items_center()
        .gap(HeaderMetrics::BAR_GAP)
        .h(HeaderMetrics::BAR_HEIGHT)
        .pl(HeaderMetrics::BAR_PADDING_X)
        .pr(Spacing::XS)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            ui::role(div(), TextVariant::Label, cx)
                .text_color(colors.foreground)
                .child(heading),
        )
        .child(
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(EditorMetrics::FILE_FONT)
                .text_color(colors.muted)
                .child(file.unwrap_or_else(|| "no file yet".to_owned())),
        )
        .child(div().flex_1())
        .children(delete);
    let body = match (editing, text) {
        (Some(input), _) => div()
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .py(Spacing::SM)
            .capture_action(cx.listener(|this, action: &InputEnter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.send_active(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.close_script_edit(window, cx);
            }))
            .child(
                part(format!("request-script-{slot_id}-input")).child(
                    Textarea::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .text_size(EditorMetrics::CODE_FONT)
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_color(colors.foreground),
                ),
            )
            .into_any_element(),
        (None, Some(text)) => {
            let request = id.to_owned();
            part(format!("request-script-{slot_id}-code"))
                .py(Spacing::SM)
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
    let focused = shell.script_edit_focused(id, slot);
    part(format!("request-script-{slot_id}-editor"))
        .relative()
        .flex()
        .flex_col()
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
        .child(header)
        .child(body)
        .when(focused, |editor| {
            editor.child(focus_ring(
                format!("request-script-{slot_id}-focus-ring"),
                cx,
            ))
        })
        .into_any_element()
}

/// The empty state `request-script-<slot>-empty` of the missing script `slot`, whose
/// button `request-script-<slot>-create` opens an empty editor.
fn empty_state(id: &str, slot: ScriptSlot, cx: &mut Context<Shell>) -> AnyElement {
    let (_, title, hint, action) = copy(slot);
    let slot_id = slot.id();
    let icon_name = match slot {
        ScriptSlot::PreRequest => IconName::Code,
        ScriptSlot::Test => IconName::CircleCheckBig,
    };
    let request = id.to_owned();
    let create = format!("request-script-{slot_id}-create");
    let button = Button::new(gpui::ElementId::Name(create.into()), action)
        .primary()
        .icon(ui::icon(IconName::Pencil))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.start_script_edit(&request, slot, window, cx);
        }));
    div()
        .p(Spacing::LG)
        .child(
            EmptyState::new(ui::icon(icon_name), hint)
                .id(gpui::ElementId::Name(
                    format!("request-script-{slot_id}-empty").into(),
                ))
                .title(title)
                .primary_action(button),
        )
        .into_any_element()
}
