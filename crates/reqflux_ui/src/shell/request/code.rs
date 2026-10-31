//! Read-only text blocks shared by the Body, Auth, Scripts and Settings panes: a code block
//! with right-aligned line numbers, a section label and an empty state.

use bolt::tokens::{family, space};
use gpui::{AnyElement, App, Div, ParentElement, Styled, div, prelude::*};
use reqflux_engine::Inspection;

use super::pane_model::numbered_lines;
use super::spans;
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// `text` in mono under element `id`, one row `<line_prefix>-<n>` per line with its
/// references styled.
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
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .children(numbered_lines(text).into_iter().map(|(n, line)| {
            labeled_row(format!("{line_prefix}-{n}"), line.to_owned())
                .flex()
                .items_center()
                .h(tokens::CODE_LINE)
                .child(
                    div()
                        .w(tokens::GRID_NUMBER_COLUMN)
                        .flex_none()
                        .pr(space::MD)
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

/// A small muted heading above a block.
pub(super) fn label(text: &'static str, cx: &App) -> Div {
    div()
        .pl(space::MD)
        .pt(space::MD)
        .pb(space::SM)
        .text_size(tokens::SMALL_MONO_TEXT)
        .text_color(bolt::colors(cx).muted)
        .child(text)
}

/// A muted line `text` under element `id`.
pub(super) fn empty(id: String, text: &'static str, cx: &App) -> AnyElement {
    labeled_row(id, text.to_owned())
        .pl(space::MD)
        .pt(space::MD)
        .text_size(tokens::GRID_TEXT)
        .text_color(bolt::colors(cx).muted)
        .child(text)
        .into_any_element()
}
