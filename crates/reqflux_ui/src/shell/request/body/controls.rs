//! The controls of the Body, Auth and Settings panes besides their segmented controls (see
//! `request::segmented`): a select that opens a pane menu, and the field wells a value shows
//! in until a click opens its editor, drawn as `bolt::controls::Input` draws its field.

use bolt::controls::SelectTrigger;
use bolt::primitives::{Chamfer, ChamferRing};
use bolt::tokens::{ChamferCut, Fields, Spacing};
use bolt_legacy::IconName;
use gpui::{App, Div, ElementId, ParentElement, SharedString, Stateful, Styled};

use crate::shell::request::ui;
use crate::shell::rows::part;

/// A select `id` showing `value`, with an optional leading icon: `bolt`'s select trigger, a
/// button that opens the shell's menu.
pub(in crate::shell::request) fn select(
    id: String,
    leading: Option<IconName>,
    value: impl Into<SharedString>,
) -> SelectTrigger {
    let trigger = SelectTrigger::new(ElementId::Name(id.into()), value);
    match leading {
        Some(name) => trigger.leading_icon(ui::icon(name)),
        None => trigger,
    }
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
