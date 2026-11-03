//! The code of the Body and Scripts panes: numbered lines in the data face, colored by their
//! language's grammar (design/syntax/01) with their references marked over the colors, and
//! the empty state of a pane.

use bolt::composites::EmptyState;
use bolt::tokens::{EditorMetrics, GridMetrics, Spacing};
use gpui::{AnyElement, App, HighlightStyle, ParentElement, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_engine::{Inspection, Span};

use super::pane_model::numbered_lines;
use super::ui;
use crate::shell::rows::{labeled_row, part};
use crate::shell::syntax::{highlight_lines, pieces, reference_style, styled};

/// The lines of `text`, element id `id`, each `<line_prefix>-<n>` with its number, colored
/// in `language`.
pub(super) fn block(
    inspection: &Inspection,
    id: String,
    line_prefix: &str,
    text: &str,
    language: &str,
    cx: &App,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let lines = numbered_lines(text);
    let texts: Vec<&str> = lines.iter().map(|(_, line)| *line).collect();
    let syntax = highlight_lines(&texts, language, cx);
    part(id)
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT)
        .children(lines.into_iter().zip(syntax).map(|((n, line), syntax)| {
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
                        .children(line_pieces(inspection, line, &syntax, cx)),
                )
        }))
        .into_any_element()
}

/// `line` cut into pieces: its `syntax` runs with its references marked over them.
pub(super) fn line_pieces(
    inspection: &Inspection,
    line: &str,
    syntax: &[(std::ops::Range<usize>, HighlightStyle)],
    cx: &App,
) -> Vec<AnyElement> {
    let references = reference_runs(inspection, line, cx);
    pieces(line.len(), syntax, &references)
        .into_iter()
        .filter_map(|piece| {
            let text = line.get(piece.range)?.to_owned();
            Some(styled(text, piece.style.unwrap_or_default()).into_any_element())
        })
        .collect()
}

/// The `{{name}}` references of `line` with their marks.
fn reference_runs(
    inspection: &Inspection,
    line: &str,
    cx: &App,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let mut at = 0;
    let mut runs = Vec::new();
    for span in inspection.spans(line) {
        match span {
            Span::Literal(literal) => at += literal.len(),
            Span::Reference { raw, resolves, .. } => {
                runs.push((at..at + raw.len(), reference_style(resolves, cx)));
                at += raw.len();
            }
        }
    }
    runs
}

/// The empty state `id` of a pane: `icon` over `text`, which also names it.
pub(super) fn empty(id: String, icon: IconName, text: impl Into<String>) -> AnyElement {
    let text = text.into();
    labeled_row(id, text.clone())
        .py(Spacing::XL)
        .child(EmptyState::new(ui::icon(icon), text))
        .into_any_element()
}
