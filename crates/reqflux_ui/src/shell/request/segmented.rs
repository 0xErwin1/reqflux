//! The segmented controls of the request and response panes: the body mode, the auth source
//! and location, the settings' Default / On / Off, the response views and the body format.
//! They are drawn as `bolt::primitives::SegmentedControl` draws its track and segments.
// bolt gap: SegmentedControl names its segments `segmented-<group>-<id>`, which drops the
// element ids tests and the automation tree use, and has no counts after a label; this
// module goes once it takes caller ids and counts.

use bolt::primitives::Chamfer;
use bolt::tokens::{ChamferCut, Fields, FontSizes, TabMetrics};
use bolt_legacy::IconName;
use gpui::{App, Div, Hsla, ParentElement, Role, SharedString, Stateful, Styled, div, prelude::*};

use super::ui;
use crate::shell::rows::part;

/// The track `id` around `segments`: a tab list, or a radio group for a setting.
pub(crate) fn track(
    id: impl Into<String>,
    role: Role,
    segments: impl IntoIterator<Item = Stateful<Div>>,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id.into())
        .role(role)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .p(Fields::SEGMENT_TRACK_PADDING)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(FontSizes::XS)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
        .children(segments)
}

/// A segment `id` reading `label`, with an optional icon before it and counts after it, the
/// raised thumb when `selected`; `role` is the tab or radio button its track holds.
pub(crate) fn segment(
    id: String,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    counts: Vec<(String, Hsla)>,
    selected: bool,
    role: Role,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let label = label.into();
    let color = if selected {
        colors.strong
    } else {
        colors.muted
    };
    let thumb = if selected {
        Chamfer::new(ChamferCut::KEYCAP).fill(colors.raised)
    } else {
        Chamfer::new(ChamferCut::KEYCAP)
            .fill_hover(colors.hover)
            .interactive(SharedString::from(format!("{id}-thumb")))
    };
    part(id)
        .role(role)
        .aria_label(label.clone())
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap(Fields::SEGMENT_GAP)
        .h(Fields::SEGMENT_HEIGHT)
        .pl(Fields::SEGMENT_PADDING_X)
        .pr(Fields::SEGMENT_PADDING_X)
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(color)
        .child(thumb)
        .children(icon.map(|name| {
            bolt::primitives::Icon::new(ui::icon(name))
                .size(Fields::SEGMENT_ICON)
                .color(color)
        }))
        .child(label)
        .children(counts.into_iter().map(|(count, count_color)| {
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TabMetrics::RESULT_META_SIZE)
                .text_color(count_color)
                .child(count)
        }))
}
