//! The segmented control of the response: a sunken track holding one segment per choice, the
//! selected one raised. The HTTP views (Body, Headers, Cookies) and the body format (Pretty,
//! Raw, Preview) both use it.

use bolt::chamfer::Chamfer;
use bolt::tokens::{border, chrome, cut, family, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{App, Div, Hsla, ParentElement, Role, Stateful, Styled, div, prelude::*};

use crate::shell::rows::part;
use crate::tokens;

/// The track around `segments`, element id `id`.
pub(super) fn track(
    id: &'static str,
    segments: impl IntoIterator<Item = Stateful<Div>>,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    crate::shell::region(id)
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

/// A segment `label` with its icon and optional counts, element id `id`.
pub(super) fn segment(
    id: String,
    label: &'static str,
    icon_name: IconName,
    counts: Vec<(String, Hsla)>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let color = if selected {
        colors.strong
    } else {
        colors.muted
    };
    crate::shell::text::role(part(id), text::BODY_SMALL, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::HALF)
        .h(chrome::STATUS_CHIP)
        .pl(space::SM)
        .pr(space::SM)
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(color)
        .when(selected, |segment| {
            segment.child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
        })
        .child(Icon::new(icon_name).size(icon::SM).color(color))
        .child(label)
        .children(counts.into_iter().map(|(count, count_color)| {
            div()
                .font_family(family::MONO)
                .text_size(tokens::SMALL_MONO_TEXT)
                .text_color(count_color)
                .child(count)
        }))
}
