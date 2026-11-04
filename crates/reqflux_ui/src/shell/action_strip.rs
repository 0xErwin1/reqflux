//! The vertical strip of request actions at the left of the active request, drawn as DBFlux's
//! editor action strip: Save, Revert, Undo, Redo, then the split toggle, as inline icon-only
//! ghost buttons. Save and Discard changes are enabled while the active request has a
//! draft; Undo and Redo while its draft history has a step in that direction; Split stays
//! disabled. Every tooltip names the action and its key.

use bolt::controls::Button;
use bolt::tokens::{Borders, EditorMetrics};
use gpui::{
    AnyElement, Context, Div, ElementId, IntoElement, ParentElement, Stateful, Styled, div,
};
use gpui_kit_assets::IconName;

use super::commands::{self, CommandId};
use super::drafts::draft_history::DraftHistories;
use super::request::ui;
use super::{Shell, region};

/// The actions of the strip: id suffix, icon and label, in order. `None` is the separator.
pub(super) const ACTIONS: [Option<(&str, IconName, &str)>; 6] = [
    Some(("save", IconName::Save, "Save")),
    Some(("revert", IconName::RotateCcw, "Discard changes")),
    Some(("undo", IconName::Undo2, "Undo")),
    Some(("redo", IconName::Redo2, "Redo")),
    None,
    Some(("split", IconName::Columns2, "Split request and response")),
];

/// Whether the action `id` can run on the request `active`: Save and Discard changes with a
/// draft, Undo and Redo with a step to take.
fn enabled(id: &str, active: &str, drafted: bool, cx: &Context<Shell>) -> bool {
    let histories = cx.try_global::<DraftHistories>();
    match id {
        "save" | "revert" => drafted,
        "undo" => histories.is_some_and(|histories| histories.can_undo(active)),
        "redo" => histories.is_some_and(|histories| histories.can_redo(active)),
        _ => false,
    }
}

/// The command whose key the action `id` shows in its tooltip.
fn command_of(id: &str) -> Option<CommandId> {
    match id {
        "save" => Some(CommandId::SaveRequest),
        "revert" => Some(CommandId::RevertRequest),
        "undo" => Some(CommandId::UndoDraft),
        "redo" => Some(CommandId::RedoDraft),
        _ => None,
    }
}

/// The tooltip of the action `id`: its label and, when it has a command, its key.
fn tooltip(id: &str, label: &str, cx: &Context<Shell>) -> String {
    let key = command_of(id)
        .map(|command| commands::effective_key(cx, command))
        .filter(|key| !key.is_empty());
    match key {
        Some(key) => format!("{label} ({})", commands::key_label(&key)),
        None => label.to_owned(),
    }
}

/// The strip, element id `request-actions`; each action is `request-action-<id>`. Save and
/// Revert act on the active request `active`.
pub(super) fn render(
    active: Option<&str>,
    drafted: bool,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let line = bolt::colors(cx).line;
    let mut actions: Vec<AnyElement> = Vec::new();
    for action in ACTIONS {
        match action {
            Some((id, icon, label)) => {
                let request = active.filter(|active| enabled(id, active, drafted, cx));
                actions.push(action_button(id, icon, label, request, cx));
            }
            None => actions.push(
                div()
                    .flex_none()
                    .w(EditorMetrics::ACTION_STRIP_SEPARATOR_WIDTH)
                    .h(Borders::THIN)
                    .my(EditorMetrics::ACTION_STRIP_SEPARATOR_MARGIN_Y)
                    .bg(line)
                    .into_any_element(),
            ),
        }
    }
    region("request-actions")
        .flex_none()
        .w(EditorMetrics::ACTION_STRIP_WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(EditorMetrics::ACTION_STRIP_GAP)
        .pt(EditorMetrics::ACTION_STRIP_PADDING_TOP)
        .border_r(Borders::THIN)
        .border_color(line)
        .children(actions)
}

/// The action `id`: enabled on the request `request`, whose draft Save saves, Discard
/// changes discards and Undo and Redo step, else disabled.
fn action_button(
    id: &str,
    icon: IconName,
    label: &'static str,
    request: Option<&str>,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let button = Button::new(
        ElementId::Name(format!("request-action-{id}").into()),
        label,
    )
    .icon(ui::icon(icon))
    .icon_only()
    .inline()
    .ghost()
    .tooltip(tooltip(id, label, cx))
    .tab_stop(false);
    match request {
        Some(request) => {
            let request = request.to_owned();
            let id = id.to_owned();
            button
                .on_click(
                    cx.listener(move |this, _event, window, cx| match id.as_str() {
                        "save" => this.save(&request, window, cx),
                        "revert" => this.revert(&request, window, cx),
                        "undo" => this.undo_draft(&request, window, cx),
                        _ => this.redo_draft(&request, window, cx),
                    }),
                )
                .into_any_element()
        }
        None => button.disabled(true).into_any_element(),
    }
}
