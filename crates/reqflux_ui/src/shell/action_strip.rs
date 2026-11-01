//! The vertical strip of request actions at the left of the active request, drawn as DBFlux's
//! editor action strip: Save, Revert, Undo, Redo, then the split toggle, as inline icon-only
//! ghost buttons. Save and Revert are enabled while the active request has a draft; Undo,
//! Redo and Split stay disabled, with their tooltip.

use bolt::controls::Button;
use bolt::tokens::{Borders, EditorMetrics};
use gpui::{
    AnyElement, Context, Div, ElementId, IntoElement, ParentElement, Stateful, Styled, div,
};
use gpui_kit_assets::IconName;

use super::request::ui;
use super::{Shell, region};

/// The actions of the strip: id suffix, icon and label, in order. `None` is the separator.
pub(super) const ACTIONS: [Option<(&str, IconName, &str)>; 6] = [
    Some(("save", IconName::Save, "Save")),
    Some(("revert", IconName::RotateCcw, "Revert")),
    Some(("undo", IconName::Undo2, "Undo")),
    Some(("redo", IconName::Redo2, "Redo")),
    None,
    Some(("split", IconName::Columns2, "Split request and response")),
];

/// Whether the action `id` acts on a draft, so it is enabled only with one.
fn drafts_only(id: &str) -> bool {
    matches!(id, "save" | "revert")
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
                let request = active.filter(|_| drafted && drafts_only(id));
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

/// The action `id`: enabled on the request `request`, whose draft Save saves and Revert
/// reverts, else disabled.
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
    .tab_stop(false);
    match request {
        Some(request) => {
            let request = request.to_owned();
            let save = id == "save";
            button
                .on_click(cx.listener(move |this, _event, window, cx| {
                    if save {
                        this.save(&request, window, cx);
                    } else {
                        this.revert(&request, window, cx);
                    }
                }))
                .into_any_element()
        }
        None => button.disabled(true).into_any_element(),
    }
}
