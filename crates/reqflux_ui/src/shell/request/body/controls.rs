//! The controls of the Body, Auth and Settings panes besides their segmented controls (see
//! `request::segmented`): a select that opens a pane menu, and the field wells a value shows
//! in until a click opens its editor, drawn as `bolt::controls::Input` draws its field.

use bolt::primitives::{Chamfer, ChamferRing, Icon};
use bolt::tokens::{ChamferCut, Fields, Spacing};
use bolt_legacy::IconName;
use gpui::{App, Div, ParentElement, Role, SharedString, Stateful, Styled, div, prelude::*};

use crate::shell::request::ui;
use crate::shell::rows::part;

/// A select `id` showing `value`, with an optional leading icon and a trailing chevron: a
/// button that opens its menu, drawn as the trigger of `bolt::controls::Dropdown`.
// bolt gap: Dropdown owns its open state and its items' ids, while these menus are the
// shell's (their ids, Escape and the backdrop are tested); a stateless select trigger in
// bolt would replace this.
pub(in crate::shell::request) fn select(
    id: String,
    leading: Option<IconName>,
    value: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let value = value.into();
    part(id)
        .role(Role::Button)
        .aria_label(value.clone())
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(Fields::TEXT)
        .text_color(colors.strong)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.raised)
                .border(colors.line_2),
        )
        .children(leading.map(|name| {
            Icon::new(ui::icon(name))
                .size(Fields::LEADING_ICON)
                .color(colors.muted)
        }))
        .child(div().flex_1().min_w_0().overflow_hidden().child(value))
        .child(
            Icon::new(ui::icon(IconName::ChevronDown))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        )
}

/// A field well `id`: a field high sunken box with an edge, in mono, around the value its
/// caller adds.
pub(in crate::shell::request) fn well(id: String, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id)
        .relative()
        .flex()
        .items_center()
        .gap(Fields::GAP)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(Fields::TEXT)
        .text_color(colors.strong)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(field_shape(cx))
}

/// The focus ring `id` of an editor box: the tint ring over the box's line, drawn only while
/// the editor has focus.
pub(in crate::shell::request) fn focus_ring(id: String, cx: &App) -> Stateful<Div> {
    let tint = bolt::brand::brand(cx).accent_tint;
    part(id)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(Chamfer::new(ChamferCut::CONTROL).ring(ChamferRing::outline(tint)))
}

/// A multi-line field well `id`, sized by its content, for the raw text and the GraphQL
/// query.
pub(in crate::shell::request) fn text_well(id: String, cx: &App) -> Stateful<Div> {
    part(id).relative().py(Spacing::SM).child(field_shape(cx))
}

/// The shape of a field: the ground fill with a line on its straight edges.
fn field_shape(cx: &App) -> Chamfer {
    let colors = bolt::colors(cx);
    Chamfer::new(ChamferCut::CONTROL)
        .fill(colors.background)
        .border(colors.line)
}
