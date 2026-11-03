//! The response body: numbered mono lines in a virtual list, so only the visible lines are
//! built, colored by the body's grammar (JSON, XML, HTML) with the search matches drawn
//! over the colors; or the size of a binary body.

use std::ops::Range;
use std::sync::Arc;

use bolt::tokens::{Borders, EditorMetrics, GridMetrics, Spacing};
use gpui::{
    AnyElement, App, HighlightStyle, ParentElement, Styled, UnderlineStyle, div, prelude::*,
    uniform_list,
};

use super::search::Hit;
use super::syntax::pieces;
use super::views::ViewState;
use crate::shell::request::{Received, size_text};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{region, text_region};
use crate::tokens;

/// The search matches the body shows, top to bottom, and which one is current.
#[derive(Clone, Default)]
pub(super) struct Found {
    pub(super) hits: Arc<[Hit]>,
    pub(super) current: usize,
}

/// The body of `received` as `lines` show it, colored by their language:
/// `response-body` with rows `response-body-line-<n>`, or, without lines,
/// `response-body-binary`. Each match of `found` is a piece `response-body-hit-<m>`, from 1,
/// the current one filled in the warning color and the others washed in it; `views` holds
/// the scroll of the lines and their syntax runs.
pub(super) fn render(
    received: &Received,
    lines: Option<(Arc<[String]>, Option<&'static str>)>,
    found: Found,
    views: &ViewState,
    cx: &App,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let Some((lines, language)) = lines else {
        let text = format!("Binary body, {}", size_text(received.size()));
        return text_region("response-body-binary")
            .aria_label(text.clone())
            .p(Spacing::MD)
            .border_t(Borders::THIN)
            .border_color(colors.line)
            .text_size(bolt::density::font_base(cx))
            .text_color(colors.muted)
            .child(text)
            .into_any_element();
    };
    let count = lines.len();
    let syntax = views.syntax.runs(&lines, language, cx);
    region("response-body")
        .flex_1()
        .min_h_0()
        .pt(Spacing::XXS)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT)
        .child(
            uniform_list("response-body-lines", count, move |range, _window, cx| {
                range
                    .filter_map(|index| {
                        let line = lines.get(index)?;
                        let runs = syntax
                            .as_ref()
                            .and_then(|runs| runs.get(index))
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        Some(line_row(index, line, runs, &found, cx))
                    })
                    .collect()
            })
            .track_scroll(&views.scroll)
            .size_full(),
        )
        .into_any_element()
}

/// The line at `index`, element id `response-body-line-<index + 1>`, in its `syntax` runs,
/// with the matches of `found` on it drawn over them.
fn line_row(
    index: usize,
    line: &str,
    syntax: &[(Range<usize>, HighlightStyle)],
    found: &Found,
    cx: &App,
) -> AnyElement {
    let n = index + 1;
    let colors = bolt::colors(cx);
    // The matches on this line: `hits` is in line order.
    let first = found.hits.partition_point(|hit| hit.line < index);
    let last = found.hits.partition_point(|hit| hit.line <= index);
    let on_line = found.hits.get(first..last).unwrap_or_default();
    let current = found
        .hits
        .get(found.current)
        .filter(|hit| hit.line == index);
    let hit = HighlightStyle {
        color: Some(colors.strong),
        background_color: Some(tokens::search_wash(colors.warning)),
        underline: Some(UnderlineStyle {
            thickness: Borders::THIN,
            color: Some(colors.warning),
            wavy: false,
        }),
        ..Default::default()
    };
    let current_hit = HighlightStyle {
        color: Some(colors.background),
        background_color: Some(colors.warning),
        ..Default::default()
    };
    let matches: Vec<_> = on_line
        .iter()
        .map(|found| {
            let style = match current == Some(found) {
                true => current_hit,
                false => hit,
            };
            (found.start..found.end, style)
        })
        .collect();
    let text = div()
        .flex()
        .whitespace_nowrap()
        .text_color(colors.foreground)
        .children(
            pieces(line.len(), syntax, &matches)
                .into_iter()
                .filter_map(|piece| {
                    let text = line.get(piece.range.clone())?.to_owned();
                    let style = piece.style.unwrap_or_default();
                    let element = div()
                        .when_some(style.color, |div, color| div.text_color(color))
                        .when_some(style.font_weight, |div, weight| div.font_weight(weight))
                        .when_some(style.background_color, |div, color| div.bg(color))
                        .when_some(style.underline, |div, underline| {
                            div.border_b(underline.thickness)
                                .when_some(underline.color, |div, color| div.border_color(color))
                        })
                        .child(text);
                    // The first piece of a match carries its id; a match a syntax run cuts
                    // continues in the next piece.
                    Some(match piece.hit {
                        Some(k) => part(format!("response-body-hit-{}", first + k + 1))
                            .child(element)
                            .into_any_element(),
                        None => element.into_any_element(),
                    })
                }),
        );
    labeled_row(format!("response-body-line-{n}"), line.to_owned())
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
        .child(text)
        .into_any_element()
}
