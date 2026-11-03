//! The syntax runs of the response body (design/syntax/01): `bolt`'s Tree-sitter runs of
//! the lines a view shows, computed once per lines and palette and split by line.

use std::cell::RefCell;
use std::sync::Arc;

use gpui::App;

use crate::shell::syntax::{LineRuns, highlight_lines};
use crate::tokens::Palette;

/// The most body text the response highlights: a larger body is drawn plain, so one
/// response never parses megabytes on the UI thread.
pub(crate) const HIGHLIGHT_LIMIT: usize = 1024 * 1024;

/// The runs last computed and what they were computed from: the lines, by identity, their
/// language and the palette.
#[derive(Default)]
pub(crate) struct SyntaxCache(RefCell<Option<Cached>>);

struct Cached {
    lines: Arc<[String]>,
    language: &'static str,
    palette: Palette,
    runs: Arc<[LineRuns]>,
}

impl SyntaxCache {
    /// The syntax runs of each of `lines` in `language`; `None` when there is no language or
    /// the lines are over [`HIGHLIGHT_LIMIT`].
    pub(crate) fn runs(
        &self,
        lines: &Arc<[String]>,
        language: Option<&'static str>,
        cx: &App,
    ) -> Option<Arc<[LineRuns]>> {
        let language = language?;
        let palette = Palette::current(cx);
        let mut cache = self.0.borrow_mut();
        if let Some(cached) = cache.as_ref()
            && Arc::ptr_eq(&cached.lines, lines)
            && cached.language == language
            && cached.palette == palette
        {
            return Some(Arc::clone(&cached.runs));
        }
        let size: usize = lines.iter().map(|line| line.len() + 1).sum();
        if size > HIGHLIGHT_LIMIT {
            return None;
        }
        let runs: Arc<[LineRuns]> = highlight_lines(lines, language, cx).into();
        *cache = Some(Cached {
            lines: Arc::clone(lines),
            language,
            palette,
            runs: Arc::clone(&runs),
        });
        Some(runs)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{HighlightStyle, TestAppContext};

    use super::*;
    use crate::shell::response::BodyText;
    use crate::shell::response::BodyView;
    use crate::theme::PaletteMode;
    use crate::tokens::tests::rgb_of;

    fn style_of(line: &str, runs: &LineRuns, token: &str) -> Option<HighlightStyle> {
        let start = line.find(token)?;
        runs.iter()
            .find(|(range, _)| range.start <= start && start + token.len() <= range.end)
            .map(|(_, style)| *style)
    }

    fn color_of(line: &str, runs: &LineRuns, token: &str) -> Option<u32> {
        style_of(line, runs, token)
            .and_then(|style| style.color)
            .map(rgb_of)
    }

    #[gpui::test]
    fn a_json_body_takes_bolt_syntax_roles(cx: &mut TestAppContext) {
        cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
        let body = BodyText::new(
            br#"{"id":"in_1","n":4900,"ok":false}"#,
            Some("application/json"),
        );
        let Some((lines, language)) = body.lines(BodyView::Pretty) else {
            panic!("no lines");
        };
        assert_eq!(language, Some("json"));
        let cache = SyntaxCache::default();
        let runs = cx.update(|cx| cache.runs(&lines, language, cx));
        let Some(runs) = runs else {
            panic!("no syntax runs");
        };
        let (Some(id), Some(id_runs)) = (lines.get(1), runs.get(1)) else {
            panic!("no line 2");
        };
        assert_eq!(id, r#"  "id": "in_1","#);
        assert_eq!(
            color_of(id, id_runs, "\"id\""),
            Some(0xA9A4B2),
            "key: property"
        );
        assert_eq!(color_of(id, id_runs, "\"in_1\""), Some(0x7BE0A0), "string");
        assert_eq!(
            color_of(id, id_runs, ":"),
            Some(0xC6C3CC),
            "punctuation: operator"
        );
        let (Some(n), Some(n_runs)) = (lines.get(2), runs.get(2)) else {
            panic!("no line 3");
        };
        assert_eq!(color_of(n, n_runs, "4900"), Some(0xB79CFF), "number");
    }

    #[gpui::test]
    fn xml_and_html_bodies_are_highlighted_and_text_stays_plain(cx: &mut TestAppContext) {
        cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
        let xml = BodyText::new(b"<a id=\"1\">x</a>", Some("application/xml"));
        let html = BodyText::new(b"<p class=\"x\">y</p>", Some("text/html; charset=utf-8"));
        let text = BodyText::new(b"<a>", Some("text/plain"));
        assert_eq!(
            xml.lines(BodyView::Pretty).map(|(_, l)| l),
            Some(Some("xml"))
        );
        assert_eq!(xml.lines(BodyView::Raw).map(|(_, l)| l), Some(Some("xml")));
        assert_eq!(
            html.lines(BodyView::Pretty).map(|(_, l)| l),
            Some(Some("html"))
        );
        assert_eq!(text.lines(BodyView::Pretty).map(|(_, l)| l), Some(None));

        let Some((lines, language)) = xml.lines(BodyView::Pretty) else {
            panic!("no lines");
        };
        let cache = SyntaxCache::default();
        let Some(runs) = cx.update(|cx| cache.runs(&lines, language, cx)) else {
            panic!("no syntax runs");
        };
        let (Some(line), Some(line_runs)) = (lines.first(), runs.first()) else {
            panic!("no line 1");
        };
        assert_eq!(
            color_of(line, line_runs, "a"),
            Some(0x6EA8FF),
            "tag: type_name"
        );
        assert_eq!(
            color_of(line, line_runs, "id"),
            Some(0xA9A4B2),
            "attribute: property"
        );
    }

    #[gpui::test]
    fn a_body_over_the_limit_stays_plain(cx: &mut TestAppContext) {
        cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
        let big: Arc<[String]> = vec!["1".repeat(HIGHLIGHT_LIMIT)].into();
        let cache = SyntaxCache::default();
        assert!(cx.update(|cx| cache.runs(&big, Some("json"), cx)).is_none());
        let small: Arc<[String]> = vec!["1".to_owned()].into();
        assert!(cx.update(|cx| cache.runs(&small, None, cx)).is_none());
    }
}
