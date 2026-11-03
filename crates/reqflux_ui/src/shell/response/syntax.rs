//! The syntax colors of the response body (design/syntax/01): `bolt`'s Tree-sitter runs of
//! the lines a view shows, computed once per lines and palette and split by line, and the
//! search matches drawn over them.

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

use gpui::{App, HighlightStyle};

use crate::tokens::Palette;

/// The most body text the response highlights: a larger body is drawn plain, so one
/// response never parses megabytes on the UI thread.
pub(crate) const HIGHLIGHT_LIMIT: usize = 1024 * 1024;

/// The syntax runs of one line, byte ranges into that line.
pub(crate) type LineRuns = Vec<(Range<usize>, HighlightStyle)>;

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
        let text = lines.join("\n");
        let runs: Arc<[LineRuns]> = by_line(lines, bolt::syntax::highlight(language, &text, cx));
        *cache = Some(Cached {
            lines: Arc::clone(lines),
            language,
            palette,
            runs: Arc::clone(&runs),
        });
        Some(runs)
    }
}

/// `runs` over `lines` joined by `\n`, cut at the line ends and moved to each line's
/// offsets.
fn by_line(lines: &[String], runs: Vec<(Range<usize>, HighlightStyle)>) -> Arc<[LineRuns]> {
    let mut out: Vec<LineRuns> = vec![Vec::new(); lines.len()];
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len() + 1;
    }
    for (range, style) in runs {
        let first = starts.partition_point(|start| *start <= range.start);
        for index in first.saturating_sub(1)..lines.len() {
            let (Some(start), Some(line), Some(runs)) =
                (starts.get(index), lines.get(index), out.get_mut(index))
            else {
                break;
            };
            if *start >= range.end {
                break;
            }
            let from = range.start.max(*start) - start;
            let to = range.end.min(start + line.len()) - start;
            if from < to {
                runs.push((from..to, style));
            }
        }
    }
    out.into()
}

/// One stretch of a line as the body draws it: its bytes, its style (`None` draws the
/// plain text color) and, when a search match starts there, the match's index on the line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub(crate) range: Range<usize>,
    pub(crate) style: Option<HighlightStyle>,
    pub(crate) hit: Option<usize>,
}

/// The pieces of a line `len` bytes long: the `matches` (in line order) drawn over the
/// `syntax` runs with `bolt::syntax::overlay`, cut where a match starts so each match
/// begins a piece.
pub(crate) fn pieces(
    len: usize,
    syntax: &[(Range<usize>, HighlightStyle)],
    matches: &[(Range<usize>, HighlightStyle)],
) -> Vec<Piece> {
    let runs = bolt::syntax::overlay(syntax.iter().cloned(), matches.iter().cloned());
    let mut cuts: Vec<usize> = runs
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .chain(matches.iter().map(|(range, _)| range.start))
        .filter(|at| *at < len)
        .chain([0, len])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|cut| match cut {
            [from, to] if from < to => Some(*from..*to),
            _ => None,
        })
        .map(|range| Piece {
            style: runs
                .iter()
                .find(|(run, _)| run.start <= range.start && range.end <= run.end)
                .map(|(_, style)| *style),
            hit: matches.iter().position(|(hit, _)| hit.start == range.start),
            range,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, hsla};

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

    #[test]
    fn runs_split_at_line_ends() {
        let lines = ["ab".to_owned(), "cde".to_owned()];
        let style = HighlightStyle::default();
        let runs = by_line(&lines, vec![(1..5, style)]);
        assert_eq!(&*runs, &[vec![(1..2, style)], vec![(0..2, style)]]);
    }

    #[test]
    fn a_search_match_draws_over_the_syntax_and_starts_a_piece() {
        let key = HighlightStyle {
            color: Some(hsla(0.1, 0.1, 0.1, 1.0)),
            ..Default::default()
        };
        let hit = HighlightStyle {
            color: Some(hsla(0.5, 0.5, 0.5, 1.0)),
            background_color: Some(hsla(0.2, 0.2, 0.2, 1.0)),
            ..Default::default()
        };
        // `  "id": 1` with the key styled and the matches `i` and `d` next to each other.
        let drawn = pieces(9, &[(2..6, key)], &[(3..4, hit), (4..5, hit)]);
        assert_eq!(
            drawn,
            vec![
                Piece {
                    range: 0..2,
                    style: None,
                    hit: None
                },
                Piece {
                    range: 2..3,
                    style: Some(key),
                    hit: None
                },
                Piece {
                    range: 3..4,
                    style: Some(hit),
                    hit: Some(0)
                },
                Piece {
                    range: 4..5,
                    style: Some(hit),
                    hit: Some(1)
                },
                Piece {
                    range: 5..6,
                    style: Some(key),
                    hit: None
                },
                Piece {
                    range: 6..9,
                    style: None,
                    hit: None
                },
            ]
        );
        assert_eq!(
            pieces(0, &[], &[]),
            Vec::<Piece>::new(),
            "an empty line has no pieces"
        );
    }
}
