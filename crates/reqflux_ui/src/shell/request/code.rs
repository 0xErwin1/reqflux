//! The code of the Body and Scripts panes: numbered lines in the data face with their
//! references styled, a caption over a block, and the empty state of a pane.

use bolt::composites::EmptyState;
use bolt::primitives::TextVariant;
use bolt::tokens::{EditorMetrics, GridMetrics, Spacing};
use bolt_legacy::IconName;
use gpui::{AnyElement, App, Div, ParentElement, Styled, div, prelude::*};
use reqflux_engine::Inspection;

use super::pane_model::numbered_lines;
use super::{spans, ui};
use crate::shell::rows::{labeled_row, part};

/// The lines of `text`, element id `id`, each `<line_prefix>-<n>` with its number.
pub(super) fn block(
    inspection: &Inspection,
    id: String,
    line_prefix: &str,
    text: &str,
    cx: &App,
) -> AnyElement {
    let colors = bolt::colors(cx);
    part(id)
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT)
        .children(numbered_lines(text).into_iter().map(|(n, line)| {
            labeled_row(format!("{line_prefix}-{n}"), line.to_owned())
                .flex()
                .items_center()
                .h(EditorMetrics::CODE_LINE_HEIGHT)
                .child(
                    div()
                        .w(GridMetrics::ROW_NUMBER_WIDTH)
                        .flex_none()
                        .pr(Spacing::MD)
                        .text_right()
                        .text_color(colors.line_2)
                        .child(n.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .whitespace_nowrap()
                        .text_color(colors.foreground)
                        .children(spans::render(inspection, line, cx)),
                )
        }))
        .into_any_element()
}

/// A muted caption `text` over a block.
pub(super) fn label(text: &'static str, cx: &App) -> Div {
    ui::role(div(), TextVariant::Caption, cx)
        .pl(Spacing::MD)
        .pt(Spacing::MD)
        .pb(Spacing::SM)
        .child(text)
}

/// The empty state `id` of a pane: `icon` over `text`, which also names it.
pub(super) fn empty(id: String, icon: IconName, text: impl Into<String>) -> AnyElement {
    let text = text.into();
    labeled_row(id, text.clone())
        .py(Spacing::XL)
        .child(EmptyState::new(ui::icon(icon), text))
        .into_any_element()
}
