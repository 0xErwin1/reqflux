//! Syntax colors on text a view draws itself (design/syntax/01): `bolt`'s Tree-sitter runs
//! of the text, split by line, and the marks a view lays over them (search matches,
//! `{{name}}` references) with `bolt::syntax::overlay`, cut into the pieces a line draws.

use std::ops::Range;

use gpui::{App, Div, HighlightStyle, ParentElement, Styled, div, prelude::*};

/// The syntax runs of one line, byte ranges into that line.
pub(crate) type LineRuns = Vec<(Range<usize>, HighlightStyle)>;

/// The runs of `lines` joined by `\n` in `language`, one list per line.
pub(crate) fn highlight_lines<S: AsRef<str>>(
    lines: &[S],
    language: &str,
    cx: &App,
) -> Vec<LineRuns> {
    let text = lines
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join("\n");
    by_line(lines, bolt::syntax::highlight(language, &text, cx))
}

/// `runs` over `lines` joined by `\n`, cut at the line ends and moved to each line's
/// offsets.
fn by_line<S: AsRef<str>>(lines: &[S], runs: Vec<(Range<usize>, HighlightStyle)>) -> Vec<LineRuns> {
    let mut out: Vec<LineRuns> = vec![Vec::new(); lines.len()];
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.as_ref().len() + 1;
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
            let to = range.end.min(start + line.as_ref().len()) - start;
            if from < to {
                runs.push((from..to, style));
            }
        }
    }
    out
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

/// `text` drawn in `style`: its color, weight and wash, and its underline as a bottom
/// border.
pub(crate) fn styled(text: String, style: HighlightStyle) -> Div {
    div()
        .when_some(style.color, |div, color| div.text_color(color))
        .when_some(style.font_weight, |div, weight| div.font_weight(weight))
        .when_some(style.background_color, |div, color| div.bg(color))
        .when_some(style.underline, |div, underline| {
            div.border_b(underline.thickness)
                .when_some(underline.color, |div, color| div.border_color(color))
        })
        .child(text)
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, hsla};

    use super::*;
    use crate::shell::request::spans;
    use crate::theme::PaletteMode;
    use crate::tokens::tests::rgb_of;

    #[gpui::test]
    fn lines_highlight_as_one_text(cx: &mut TestAppContext) {
        cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
        let lines = ["{", r#"  "a": "{{b}}""#, "}"];
        let runs = cx.update(|cx| highlight_lines(&lines, "json", cx));
        assert_eq!(runs.len(), 3);
        let color_at = |line: usize, at: usize| {
            runs.get(line)
                .and_then(|runs| runs.iter().find(|(range, _)| range.contains(&at)))
                .and_then(|(_, style)| style.color)
                .map(rgb_of)
        };
        assert_eq!(
            color_at(1, 2),
            Some(0xA9A4B2),
            "the key on line 2: property"
        );
        assert_eq!(color_at(1, 8), Some(0x7BE0A0), "its value: string");
        assert_eq!(
            color_at(2, 0),
            Some(0xC6C3CC),
            "the closing brace: operator"
        );

        let resolved = cx.update(|cx| spans::mark_style(true, cx));
        let unresolved = cx.update(|cx| spans::mark_style(false, cx));
        let drawn = pieces(
            14,
            runs.get(1).map(Vec::as_slice).unwrap_or_default(),
            &[(8..13, unresolved)],
        );
        assert!(
            drawn
                .iter()
                .any(|piece| piece.range == (8..13) && piece.style == Some(unresolved)),
            "the reference draws over the string: {drawn:?}"
        );
        assert_ne!(resolved.color, unresolved.color);
        assert!(unresolved.underline.is_some() && resolved.underline.is_none());
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
