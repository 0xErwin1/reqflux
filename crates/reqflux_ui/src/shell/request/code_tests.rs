//! The code of the Body and Scripts panes draws JSON as design/syntax/01 does even when a
//! reference stands where a value goes, and marks references as the URL field does.

use gpui::TestAppContext;
use reqflux_engine::Span;

use super::*;
use crate::theme::PaletteMode;
use crate::tokens::tests::rgb_of;

/// The line `  "amount": {{amount}},` and the bytes of its reference.
const UNQUOTED: (&str, Range<usize>) = (r#"  "amount": {{amount}},"#, 12..22);

/// `line` with the reference at `range` masked.
fn mask(line: &str, range: Range<usize>) -> String {
    mask_unquoted_references(line, std::slice::from_ref(&range))
}

#[test]
fn an_unquoted_reference_is_masked_as_a_string_of_its_length() {
    let (line, range) = UNQUOTED;
    let masked = mask(line, range);
    assert_eq!(masked, r#"  "amount": "________","#);
    assert_eq!(masked.len(), line.len(), "the runs keep their offsets");
}

#[test]
fn a_quoted_reference_is_left_alone() {
    let line = r#"  "id": "{{orderId}}","#;
    assert_eq!(mask(line, 10..21), line);
    let escaped = r#"  "a": "x\"{{y}}","#;
    assert_eq!(mask(escaped, 12..17), escaped);
}

#[test]
fn a_multibyte_reference_keeps_the_byte_length() {
    let line = "[{{ñ}}]";
    let masked = mask(line, 1..7);
    assert_eq!(masked.len(), line.len());
    assert_eq!(masked, r#"["____"]"#);
}

#[test]
fn text_without_references_is_unchanged() {
    assert_eq!(mask_unquoted_references(r#"{"a": 1}"#, &[]), r#"{"a": 1}"#);
}

#[gpui::test]
fn keys_after_an_unquoted_reference_keep_the_property_color(cx: &mut TestAppContext) {
    cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
    let lines = [
        "{",
        r#"  "amount": {{amount}},"#,
        r#"  "wallet": "x","#,
        r#"  "endConsumer": true"#,
        "}",
    ];
    let references: Vec<Vec<Range<usize>>> = vec![vec![], vec![12..22], vec![], vec![], vec![]];
    let runs = cx.update(|cx| syntax_runs(&lines, &references, "json", cx));
    let color_at = |line: usize, at: usize| {
        runs.get(line)
            .and_then(|runs| runs.iter().find(|(range, _)| range.contains(&at)))
            .and_then(|(_, style)| style.color)
            .map(rgb_of)
    };
    assert_eq!(color_at(1, 3), Some(0xA9A4B2), "the key before it");
    assert_eq!(color_at(2, 3), Some(0xA9A4B2), "the key after it: property");
    assert_eq!(color_at(3, 3), Some(0xA9A4B2), "the last key: property");
    assert_eq!(color_at(2, 12), Some(0x7BE0A0), "its string value");
    assert_eq!(color_at(3, 18), Some(0xB79CFF), "its boolean value");
    assert_eq!(
        color_at(4, 0),
        Some(0xC6C3CC),
        "the closing brace: punctuation"
    );
}

#[gpui::test]
fn a_reference_is_marked_as_the_url_field_marks_it(cx: &mut TestAppContext) {
    cx.update(|cx| crate::theme::install(PaletteMode::Dark, cx));
    let spans = [
        Span::Literal("a ".to_owned()),
        Span::Reference {
            raw: "{{x}}".to_owned(),
            name: "x".to_owned(),
            resolves: true,
            secret: false,
        },
        Span::Reference {
            raw: "{{y}}".to_owned(),
            name: "y".to_owned(),
            resolves: false,
            secret: false,
        },
    ];
    let runs = cx.update(|cx| mark_runs(&spans, cx));
    let (resolved, unresolved) = cx.update(|cx| {
        (
            super::super::spans::mark_style(true, cx),
            super::super::spans::mark_style(false, cx),
        )
    });
    assert_eq!(runs, vec![(2..7, resolved), (7..12, unresolved)]);
}
