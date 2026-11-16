//! The response body (S4 of feedback-nov16): a read-only code editor (`code_view`) with a
//! line-number gutter, colored by the body's grammar (JSON, XML, HTML) with the search
//! matches drawn over the colors, in which the text can be selected and copied; or the size
//! of a binary body.
//!
//! The editor is seeded by [`sync`] before a frame draws, only when the lines it shows, or
//! their language, change: the body, its view or the JSONPath filter. The matches and the
//! scroll to the current one follow the search.

use std::ops::Range;
use std::sync::Arc;

use bolt::tokens::{Borders, Spacing};
use gpui::{
    AnyElement, App, HighlightStyle, ParentElement, Styled, UnderlineStyle, Window, prelude::*,
};
use gpui_component::input::TextDecoration;

use super::model::BodyView;
use super::search::Hit;
use super::sub_tabs::{effective_tab, effective_view};
use super::views::{self, ViewState};
use crate::shell::code_view::CodeView;
use crate::shell::request::{Completed, Received, size_text};
use crate::shell::send::RequestSend;
use crate::shell::{region, text_region};
use crate::tokens;

use super::model::ResponseTab;

/// What the body editor of a request was last seeded with: the lines, by identity, and
/// their language.
pub(crate) struct BodyCode {
    view: CodeView,
    lines: Arc<[String]>,
    language: Option<&'static str>,
}

impl BodyCode {
    /// The editor of the body, for tests.
    #[cfg(test)]
    pub(crate) fn view(&self) -> &CodeView {
        &self.view
    }
}

/// The body of `received`: `response-body` around the editor `response-body-text`, or,
/// for a binary body, `response-body-binary`. `views` holds the editor.
pub(super) fn render(received: &Received, binary: bool, views: &ViewState, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    if binary {
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
    }
    region("response-body")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .pt(Spacing::XXS)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .children(
            views
                .code
                .as_ref()
                .map(|code| code.view.fill("response-body-text".to_owned(), cx)),
        )
        .into_any_element()
}

/// The response `send` shows: its outcome, or the one before while a send is in flight.
fn shown_response(send: &RequestSend) -> Option<&Received> {
    match send.state.last()? {
        Completed::Response(received) => Some(received),
        Completed::Failure(_) => None,
    }
}

/// Seeds the body editor of `send` before a frame draws it: nothing while the body is not
/// the tab shown or its view is Preview; for a binary body the editor goes; else the editor
/// shows the lines of the view, `marks` the search matches and the current one, with the
/// scroll brought to it after a step.
pub(crate) fn sync(send: &mut RequestSend, window: &mut Window, cx: &mut App) {
    let Some(received) = shown_response(send) else {
        return;
    };
    if effective_tab(send.tab, received) != ResponseTab::Body
        || effective_view(send.view, received) == BodyView::Preview
    {
        return;
    }
    let Some((lines, language)) = views::shown_lines(send, received, cx) else {
        send.views.code = None;
        return;
    };
    let hits = views::hits(send, cx);
    let current = send.views.current;
    let colors = bolt::colors(cx);
    let (marks, current_offset) = marks(&lines, &hits, current, &colors);

    let code = send.views.code.get_or_insert_with(|| BodyCode {
        view: CodeView::new(window, cx),
        lines: Arc::from(Vec::new()),
        language: None,
    });
    let reseed = !Arc::ptr_eq(&code.lines, &lines) || code.language != language;
    if reseed {
        code.view.show(&lines.join("\n"), language, window, cx);
        code.lines = lines;
        code.language = language;
    }
    code.view.mark(marks, cx);
    if std::mem::take(&mut send.views.reveal)
        && let Some(offset) = current_offset
    {
        code.view.reveal(offset, cx);
    }
}

/// The decorations of the matches `hits` in `lines`, the `current` one in the warning color
/// and the others washed in it, and the byte offset of the current one in the text the
/// lines make.
fn marks(
    lines: &[String],
    hits: &[Hit],
    current: usize,
    colors: &bolt::Colors,
) -> (Vec<TextDecoration>, Option<usize>) {
    if hits.is_empty() {
        return (Vec::new(), None);
    }
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for line in lines {
        starts.push(at);
        at += line.len() + 1;
    }
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
    let mut current_offset = None;
    let marks = hits
        .iter()
        .enumerate()
        .filter_map(|(index, found)| {
            let start = starts.get(found.line)?;
            let range: Range<usize> = start + found.start..start + found.end;
            let style = match index == current {
                true => {
                    current_offset = Some(range.start);
                    current_hit
                }
                false => hit,
            };
            Some(TextDecoration::new(range, style))
        })
        .collect();
    (marks, current_offset)
}
