//! The response body: numbered mono lines in a virtual list, so only the visible lines are
//! built, with JSON tokens colored in the Pretty view; or the size of a binary body.

use bolt_legacy::tokens::{border, family, space};
use gpui::{AnyElement, App, Hsla, ParentElement, Styled, div, prelude::*, uniform_list};

use super::model::{BodyView, Token, tokens as line_tokens};
use crate::shell::request::{Received, size_text};
use crate::shell::rows::labeled_row;
use crate::shell::{region, text_region};
use crate::tokens::{self, SyntaxColors};

/// The body of `received` as `view`: `response-body` with rows `response-body-line-<n>`, or
/// `response-body-binary`.
pub(super) fn render(received: &Received, view: BodyView, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    let Some((lines, json)) = received.text.lines(view) else {
        let text = format!("Binary body, {}", size_text(received.size()));
        return text_region("response-body-binary")
            .aria_label(text.clone())
            .p(space::MD)
            .border_t(border::THIN)
            .border_color(colors.line)
            .text_size(tokens::GRID_TEXT)
            .text_color(colors.muted)
            .child(text)
            .into_any_element();
    };
    let count = lines.len();
    region("response-body")
        .flex_1()
        .min_h_0()
        .pt(space::HALF)
        .border_t(border::THIN)
        .border_color(colors.line)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .child(
            uniform_list("response-body-lines", count, move |range, _window, cx| {
                range
                    .filter_map(|index| {
                        let line = lines.get(index)?;
                        Some(line_row(index + 1, line, json, cx))
                    })
                    .collect()
            })
            .size_full(),
        )
        .into_any_element()
}

/// Line `n`, element id `response-body-line-<n>`, colored as JSON when `json`.
fn line_row(n: usize, line: &str, json: bool, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    let syntax = SyntaxColors::for_palette(bolt_legacy::palette(cx));
    let text = div()
        .flex()
        .whitespace_nowrap()
        .text_color(colors.foreground);
    let text = if json {
        text.children(line_tokens(line).into_iter().map(|(token, piece)| {
            div()
                .text_color(token_color(token, &syntax, colors.foreground))
                .child(piece.to_owned())
        }))
    } else {
        text.child(line.to_owned())
    };
    labeled_row(format!("response-body-line-{n}"), line.to_owned())
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
