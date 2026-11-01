//! The vertical strip of request actions at the left of the active request: Save, Revert,
//! Undo, Redo, then the split toggle. Save and Revert are enabled while the active request
//! has a draft; Undo, Redo and Split stay disabled behind a blocker that swallows hover and
//! clicks and shows the tooltip.

use bolt_legacy::IconName;
use bolt_legacy::controls::IconButton;
use bolt_legacy::tokens::{action_strip, border};
use gpui::{
    AnyElement, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div,
};
use gpui_component::tooltip::Tooltip;

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
    let line = bolt_legacy::colors(cx).line;
    let mut actions: Vec<AnyElement> = Vec::new();
    for action in ACTIONS {
        match action {
            Some((id, icon, label)) if drafts_only(id) => match active {
                Some(request) if drafted => {
                    actions.push(enabled_action(request, id, icon, label, cx).into_any_element())
                }
                _ => actions.push(disabled_action(id, icon, label).into_any_element()),
            },
            Some((id, icon, label)) => {
                actions.push(disabled_action(id, icon, label).into_any_element())
            }
            None => actions.push(
                div()
                    .flex_none()
                    .w(action_strip::SEPARATOR)
                    .h(border::THIN)
                    .my(action_strip::SEPARATOR_MARGIN)
                    .bg(line)
                    .into_any_element(),
            ),
        }
    }
    region("request-actions")
        .flex_none()
        .w(action_strip::WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(action_strip::GAP)
        .py(action_strip::PADDING_Y)
        .border_r(border::THIN)
        .border_color(line)
        .children(actions)
}

/// An enabled action on the request `request`: the ghost button that saves or reverts its
/// draft.
fn enabled_action(
    request: &str,
    id: &str,
    icon: IconName,
    label: &'static str,
    cx: &mut Context<Shell>,
) -> Div {
    let request = request.to_owned();
    let save = id == "save";
    div().relative().flex_none().child(
        IconButton::new(format!("request-action-{id}"), icon, label).on_click(cx.listener(
            move |this, _event, window, cx| {
                if save {
                    this.save(&request, window, cx);
                } else {
                    this.revert(&request, window, cx);
                }
            },
        )),
    )
}

/// A disabled action: the ghost button under a blocker that takes the pointer.
fn disabled_action(id: &str, icon: IconName, label: &'static str) -> Div {
    div()
        .relative()
        .flex_none()
        .child(IconButton::new(format!("request-action-{id}"), icon, label))
        .child(
            div()
                .id(ElementId::Name(
                    format!("request-action-{id}-disabled").into(),
                ))
                .absolute()
                .inset_0()
                .occlude()
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx)),
        )
}
