//! The controls of the Body and Auth panes, as artboards 04 and 05 draw them: a segmented
//! control (a sunken track holding one segment per choice, the selected one raised), a select
//! (a raised box with a chevron) and a field well (a sunken box with an edge).

use bolt::chamfer::Chamfer;
use bolt::tokens::{border, chrome, cut, family, field, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{App, Div, ParentElement, Role, SharedString, Stateful, Styled, div, prelude::*};

use crate::shell::rows::part;
use crate::tokens;

/// The track around `segments`, element id `id`.
pub(in crate::shell::request) fn track(
    id: &str,
    segments: impl IntoIterator<Item = Stateful<Div>>,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id.to_owned())
        .role(Role::TabList)
        .relative()
        .flex_none()
        .flex()
        .p(border::THICK)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
        .children(segments)
}

/// A segment `label` with its icon and an optional muted count, element id `id`.
pub(in crate::shell::request) fn segment(
    id: String,
    label: impl Into<SharedString>,
    icon_name: IconName,
    count: Option<String>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let label = label.into();
    let color = if selected {
        colors.strong
    } else {
        colors.muted
    };
    crate::shell::text::role(part(id), text::BODY_SMALL, cx)
        .role(Role::Tab)
        .aria_label(label.clone())
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::HALF)
        .h(chrome::STATUS_CHIP)
        .pl(field::PADDING_X)
        .pr(field::PADDING_X)
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(color)
        .when(selected, |segment| {
            segment.child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
        })
        .child(Icon::new(icon_name).size(icon::CHIP).color(color))
        .child(label)
        .children(count.map(|count| {
            div()
                .font_family(family::MONO)
                .text_size(tokens::SMALL_MONO_TEXT)
                .text_color(colors.muted)
                .child(count)
        }))
}

/// A select `id` showing `value`, with an optional leading icon and a trailing chevron: a
/// button that opens its menu.
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
        .gap(field::GAP)
        .h(chrome::CONTROL)
        .pl(field::PADDING_X)
        .pr(field::PADDING_X)
        .text_size(field::FONT)
        .text_color(colors.strong)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.raised)
                .edge(colors.line, border::THIN),
        )
        .children(leading.map(|name| Icon::new(name).size(icon::MD).color(colors.muted)))
        .child(div().flex_1().min_w_0().overflow_hidden().child(value))
        .child(
            Icon::new(IconName::ChevronDown)
                .size(icon::SM)
                .color(colors.muted),
        )
}

/// A field well `id`: a `chrome::CONTROL` high sunken box with an edge, in mono, around the
/// value its caller adds.
pub(in crate::shell::request) fn well(id: String, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id)
        .relative()
        .flex()
        .items_center()
        .gap(field::GAP)
        .h(chrome::CONTROL)
        .pl(field::PADDING_X)
        .pr(field::PADDING_X)
        .font_family(family::MONO)
        .text_size(field::FONT)
        .text_color(colors.strong)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
}

/// The focus ring `id` of an editor box: the accent edge over the box's plain one, drawn only
/// while the editor has focus.
pub(in crate::shell::request) fn focus_ring(id: String, cx: &App) -> Stateful<Div> {
    let accent = bolt::brand(cx).accent_tint;
    part(id)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(Chamfer::new(cut::CONTROL).edge(accent, border::FOCUS_RING))
}

/// A multi-line field well `id`, sized by its content, for the raw text and the GraphQL
/// query.
pub(in crate::shell::request) fn text_well(id: String, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id).relative().py(space::SM).child(
        Chamfer::new(cut::CONTROL)
            .fill(colors.background)
            .edge(colors.line, border::THIN),
    )
}
