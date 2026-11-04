//! The code of the Body and Scripts panes: numbered lines in the data face, colored by their
//! language's grammar (design/syntax/01) with their references marked over the colors, and
//! the empty state of a pane.

use bolt::composites::EmptyState;
use bolt::tokens::{EditorMetrics, GridMetrics, Spacing};
use std::ops::Range;

use gpui::{AnyElement, App, HighlightStyle, ParentElement, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_engine::{Inspection, Span};

use super::pane_model::numbered_lines;
use super::spans;
use super::ui;
use crate::shell::rows::{labeled_row, part};
use crate::shell::syntax::{LineRuns, highlight_lines, pieces, styled};

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
    let references: Vec<Vec<Range<usize>>> = texts
        .iter()
        .map(|line| {
            references(&inspection.spans(line))
                .into_iter()
                .map(|(range, _)| range)
                .collect()
        })
        .collect();
    let syntax = syntax_runs(&texts, &references, language, cx);
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

/// The syntax runs of `lines` in `language`, given the byte ranges of each line's references.
fn syntax_runs(
    lines: &[&str],
    references: &[Vec<Range<usize>>],
    language: &str,
    cx: &App,
) -> Vec<LineRuns> {
    if language != "json" {
        return highlight_lines(lines, language, cx);
    }
    let masked: Vec<String> = lines
        .iter()
        .zip(references.iter().chain(std::iter::repeat(&Vec::new())))
        .map(|(line, references)| mask_unquoted_references(line, references))
        .collect();
    highlight_lines(&masked, language, cx)
}

/// `line` cut into pieces: its `syntax` runs with its references marked over them.
pub(super) fn line_pieces(
    inspection: &Inspection,
    line: &str,
    syntax: &[(Range<usize>, HighlightStyle)],
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
) -> Vec<(Range<usize>, HighlightStyle)> {
    mark_runs(&inspection.spans(line), cx)
}

/// The references among `spans`: their bytes in the text the spans cut, and whether each
/// resolves.
fn references(spans: &[Span]) -> Vec<(Range<usize>, bool)> {
    let mut at = 0;
    let mut found = Vec::new();
    for span in spans {
        match span {
            Span::Literal(literal) => at += literal.len(),
            Span::Reference { raw, resolves, .. } => {
                found.push((at..at + raw.len(), *resolves));
                at += raw.len();
            }
        }
    }
    found
}

/// The marks of the references among `spans`, drawn as the URL field draws them.
fn mark_runs(spans: &[Span], cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
    references(spans)
        .into_iter()
        .map(|(range, resolves)| (range, spans::mark_style(resolves, cx)))
        .collect()
}

/// `line` with each of its `references` outside a string replaced by a string of the same
/// length, so the line parses as JSON where a value stands and the runs keep their offsets.
fn mask_unquoted_references(line: &str, references: &[Range<usize>]) -> String {
    let mut masked = String::with_capacity(line.len());
    let (mut at, mut in_string, mut escaped) = (0, false, false);
    for reference in references {
        let Some(before) = line.get(at..reference.start) else {
            return line.to_owned();
        };
        for character in before.chars() {
            match (escaped, character) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = in_string,
                (false, '"') => in_string = !in_string,
                _ => {}
            }
        }
        masked.push_str(before);
        let Some(text) = line.get(reference.clone()) else {
            return line.to_owned();
        };
        if in_string {
            masked.push_str(text);
        } else {
            masked.push('"');
            masked.extend(std::iter::repeat_n('_', text.len().saturating_sub(2)));
            masked.push('"');
        }
        at = reference.end;
    }
    masked.push_str(line.get(at..).unwrap_or_default());
    masked
}

/// The empty state `id` of a pane: `icon` over `text`, which also names it.
pub(super) fn empty(id: String, icon: IconName, text: impl Into<String>) -> AnyElement {
    let text = text.into();
    labeled_row(id, text.clone())
        .py(Spacing::XL)
        .child(EmptyState::new(ui::icon(icon), text))
        .into_any_element()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
