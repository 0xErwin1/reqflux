//! The response body: numbered mono lines in a virtual list, so only the visible lines are
//! built, with JSON tokens colored in the Pretty view and the search matches highlighted;
//! or the size of a binary body.

use std::sync::Arc;

use bolt::tokens::{Borders, EditorMetrics, GridMetrics, Spacing};
use gpui::{
    AnyElement, App, Hsla, ParentElement, Styled, UniformListScrollHandle, div, prelude::*,
    uniform_list,
};

use super::model::{BodyView, Token, tokens as line_tokens};
use super::search::{Hit, Mark, mark};
use crate::shell::request::{Received, size_text};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{region, text_region};
use crate::tokens::{self, SyntaxColors};

/// The search matches the body shows, top to bottom, and which one is current.
#[derive(Clone, Default)]
pub(super) struct Found {
    pub(super) hits: Arc<[Hit]>,
    pub(super) current: usize,
}

/// The body of `received` as `view`: `response-body` with rows `response-body-line-<n>`, or
/// `response-body-binary`. Each match of `found` is a piece `response-body-hit-<m>`, from 1,
/// the current one filled in the warning color and the others washed in it; `scroll`
/// scrolls the lines.
pub(super) fn render(
    received: &Received,
    view: BodyView,
    found: Found,
    scroll: &UniformListScrollHandle,
    cx: &App,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let Some((lines, json)) = received.text.lines(view) else {
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
                        Some(line_row(index, line, json, &found, cx))
                    })
                    .collect()
            })
            .track_scroll(scroll)
            .size_full(),
        )
        .into_any_element()
}

/// The line at `index`, element id `response-body-line-<index + 1>`, colored as JSON when
/// `json`, with the matches of `found` on it highlighted.
fn line_row(index: usize, line: &str, json: bool, found: &Found, cx: &App) -> AnyElement {
    let n = index + 1;
    let colors = bolt::colors(cx);
    let syntax = SyntaxColors::current(cx);
    let pieces = match json {
        true => line_tokens(line),
        false => vec![(Token::Plain, line)],
    };
    // The matches on this line: `hits` is in line order.
    let first = found.hits.partition_point(|hit| hit.line < index);
    let last = found.hits.partition_point(|hit| hit.line <= index);
    let on_line = found.hits.get(first..last).unwrap_or_default();
    let ranges: Vec<(usize, usize)> = on_line.iter().map(|hit| (hit.start, hit.end)).collect();
    let current = found
        .hits
        .get(found.current)
        .filter(|hit| hit.line == index)
        .map(|hit| (hit.start, hit.end));
    let mut offset = 0;
    let text = div()
        .flex()
        .whitespace_nowrap()
        .text_color(colors.foreground)
        .children(
            mark(&pieces, &ranges, current)
                .into_iter()
                .map(|(token, marked, piece)| {
                    let at = offset;
                    offset += piece.len();
                    let color = token_color(token, &syntax, colors.foreground);
                    let element = match marked {
                        Mark::None => {
                            return div()
                                .text_color(color)
                                .child(piece.to_owned())
                                .into_any_element();
                        }
                        Mark::Current => div().bg(colors.warning).text_color(colors.background),
                        Mark::Hit => div()
                            .bg(tokens::search_wash(colors.warning))
                            .border_b(Borders::THIN)
                            .border_color(colors.warning)
                            .text_color(colors.strong),
                    };
                    // The first piece of a match carries its id; a match a token boundary cuts
                    // continues in the next piece.
                    let starts = on_line.iter().position(|hit| hit.start == at);
                    match starts {
                        Some(k) => part(format!("response-body-hit-{}", first + k + 1))
                            .child(element.child(piece.to_owned()))
                            .into_any_element(),
                        None => element.child(piece.to_owned()).into_any_element(),
                    }
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

fn token_color(token: Token, syntax: &SyntaxColors, plain: Hsla) -> Hsla {
    match token {
        Token::Key => syntax.key,
        Token::String => syntax.string,
        Token::Literal => syntax.number,
        Token::Punct => syntax.punct,
        Token::Plain => plain,
    }
}
