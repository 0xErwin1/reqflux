//! The response view without GPUI: the selected sub-tab and body view, the body split into
//! lines once per response, JSON indenting and the tokens a JSON line colors.

use std::sync::Arc;

/// A response sub-tab, in row order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ResponseTab {
    #[default]
    Body,
    Headers,
    Cookies,
    Timeline,
    Tests,
    Console,
}

/// How the body shows: indented and colored, the bytes as text, or previewed as an image
/// or the readable text of an HTML page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BodyView {
    #[default]
    Pretty,
    Raw,
    Preview,
}

/// The body of a response split into lines, computed once when the response arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BodyText {
    /// The body is not UTF-8.
    Binary,
    Text {
        raw: Arc<[String]>,
        /// The JSON indented, or the raw lines when the body is not JSON.
        pretty: Arc<[String]>,
        /// Whether the body parsed as JSON, so Pretty colors its tokens.
        json: bool,
    },
}

impl BodyText {
    /// Splits `body`, indenting it when it parses as JSON and either `content_type` names
    /// JSON or the body starts like an object or array.
    pub(crate) fn new(body: &[u8], content_type: Option<&str>) -> Self {
        let Ok(text) = std::str::from_utf8(body) else {
            return Self::Binary;
        };
        let raw: Arc<[String]> = text.lines().map(str::to_owned).collect();
        let looks_json =
            content_type.is_some_and(is_json_type) || text.trim_start().starts_with(['{', '[']);
        match looks_json.then(|| pretty_json(text)).flatten() {
            Some(pretty) => Self::Text {
                raw,
                pretty: pretty.lines().map(str::to_owned).collect(),
                json: true,
            },
            None => Self::Text {
                pretty: Arc::clone(&raw),
                raw,
                json: false,
            },
        }
    }

    /// The lines `view` shows and whether they are colored as JSON; `None` for a binary body.
    pub(crate) fn lines(&self, view: BodyView) -> Option<(Arc<[String]>, bool)> {
        match (self, view) {
            (Self::Binary, _) => None,
            (Self::Text { pretty, json, .. }, BodyView::Pretty | BodyView::Preview) => {
                Some((Arc::clone(pretty), *json))
            }
            (Self::Text { raw, .. }, BodyView::Raw) => Some((Arc::clone(raw), false)),
        }
    }
}

/// `application/json`, `application/problem+json`, `text/json` and the like.
fn is_json_type(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence.ends_with("/json") || essence.ends_with("+json")
}

/// `text` indented by two spaces when it is valid JSON. Keys keep their order and numbers
/// their spelling: the text is re-indented, not re-serialized.
pub(crate) fn pretty_json(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text).ok()?;
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0_usize;
    let mut chars = text.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n("  ", depth));
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                let mut escaped = false;
                for c in chars.by_ref() {
                    out.push(c);
                    match c {
                        _ if escaped => escaped = false,
                        '\\' => escaped = true,
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '{' | '[' => {
                out.push(c);
                while chars.next_if(|next| next.is_whitespace()).is_some() {}
                let close = if c == '{' { '}' } else { ']' };
                if chars.next_if_eq(&close).is_some() {
                    out.push(close);
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, depth);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Some(out)
}

/// The kind of a piece of a JSON line, which picks its syntax color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token {
    Key,
    String,
    /// Numbers, booleans and null.
    Literal,
    Punct,
    /// Whitespace and anything else.
    Plain,
}

/// `line` of indented JSON cut into tokens, left to right; the pieces join back to `line`.
pub(crate) fn tokens(line: &str) -> Vec<(Token, &str)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(first) = rest.chars().next() {
        let (token, len) = match first {
            '"' => {
                let len = string_len(rest);
                let after = rest.get(len..).unwrap_or_default().trim_start();
                let token = if after.starts_with(':') {
                    Token::Key
                } else {
                    Token::String
                };
                (token, len)
            }
            '{' | '}' | '[' | ']' | ',' | ':' => (Token::Punct, 1),
            c if c.is_whitespace() => (Token::Plain, run_len(rest, char::is_whitespace)),
            c if c.is_ascii_alphanumeric() || c == '-' => (
                Token::Literal,
                run_len(rest, |c| c.is_ascii_alphanumeric() || "-+.".contains(c)),
            ),
            c => (Token::Plain, c.len_utf8()),
        };
        let (piece, tail) = rest.split_at(len);
        out.push((token, piece));
        rest = tail;
    }
    out
}

/// The byte length of the string literal `text` starts with, quotes included; the rest of
/// the line when it does not close.
fn string_len(text: &str) -> usize {
    let mut escaped = false;
    for (index, c) in text.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return index + 1,
            _ => {}
        }
    }
    text.len()
}

/// The byte length of the run of `text` that `keep` accepts.
fn run_len(text: &str, keep: impl Fn(char) -> bool) -> usize {
    text.find(|c| !keep(c)).unwrap_or(text.len())
}

/// `Sent with unresolved variables: a, b`, or `None` when every name resolved.
pub(crate) fn unresolved_text(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| format!("Sent with unresolved variables: {}", names.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(body: &BodyText, view: BodyView) -> Vec<String> {
        body.lines(view)
            .map(|(lines, _)| lines.to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn json_is_reindented_keeping_key_order_and_numbers() {
        let pretty = pretty_json(r#"{"z":1.50,"a":[1, {}],"s":"x,{\"y\":[]}","e":[ ]}"#);
        assert_eq!(
            pretty.as_deref(),
            Some(
                "{\n  \"z\": 1.50,\n  \"a\": [\n    1,\n    {}\n  ],\n  \"s\": \"x,{\\\"y\\\":[]}\",\n  \"e\": []\n}"
            )
        );
        assert_eq!(pretty_json("{\"a\":"), None);
        assert_eq!(pretty_json("plain"), None);
    }

    #[test]
    fn a_json_line_splits_into_colored_tokens() {
        assert_eq!(
            tokens(r#"  "id": "a\"b", "n": -1.5e3, "ok": true, [null]"#),
            vec![
                (Token::Plain, "  "),
                (Token::Key, r#""id""#),
                (Token::Punct, ":"),
                (Token::Plain, " "),
                (Token::String, r#""a\"b""#),
                (Token::Punct, ","),
                (Token::Plain, " "),
                (Token::Key, r#""n""#),
                (Token::Punct, ":"),
                (Token::Plain, " "),
                (Token::Literal, "-1.5e3"),
                (Token::Punct, ","),
                (Token::Plain, " "),
                (Token::Key, r#""ok""#),
                (Token::Punct, ":"),
                (Token::Plain, " "),
                (Token::Literal, "true"),
                (Token::Punct, ","),
                (Token::Plain, " "),
                (Token::Punct, "["),
                (Token::Literal, "null"),
                (Token::Punct, "]"),
            ]
        );
        assert_eq!(tokens(""), Vec::new());
        assert_eq!(tokens("\"open"), vec![(Token::String, "\"open")]);
    }

    #[test]
    fn a_json_body_has_pretty_and_raw_lines() {
        let body = BodyText::new(br#"{"id":7,"tags":["a"]}"#, Some("application/json"));
        assert_eq!(
            lines(&body, BodyView::Pretty),
            [
                "{",
                "  \"id\": 7,",
                "  \"tags\": [",
                "    \"a\"",
                "  ]",
                "}"
            ]
        );
        assert_eq!(lines(&body, BodyView::Raw), [r#"{"id":7,"tags":["a"]}"#]);
        assert_eq!(
            body.lines(BodyView::Pretty).map(|(_, json)| json),
            Some(true)
        );
        assert_eq!(body.lines(BodyView::Raw).map(|(_, json)| json), Some(false));
    }

    #[test]
    fn json_is_detected_by_content_type_or_shape() {
        let typed = BodyText::new(b"1", Some("application/problem+json; charset=utf-8"));
        assert_eq!(
            typed.lines(BodyView::Pretty).map(|(_, json)| json),
            Some(true)
        );
        let shaped = BodyText::new(b" [1,2]", Some("text/plain"));
        assert_eq!(lines(&shaped, BodyView::Pretty), ["[", "  1,", "  2", "]"]);
        let text = BodyText::new(b"one\ntwo", Some("text/plain"));
        assert_eq!(lines(&text, BodyView::Pretty), ["one", "two"]);
        assert_eq!(
            text.lines(BodyView::Pretty).map(|(_, json)| json),
            Some(false)
        );
        let broken = BodyText::new(b"{\"a\":", Some("application/json"));
        assert_eq!(lines(&broken, BodyView::Pretty), ["{\"a\":"]);
    }

    #[test]
    fn a_body_that_is_not_utf8_is_binary() {
        let body = BodyText::new(&[0xFF, 0xFE, 0x00], None);
        assert_eq!(body, BodyText::Binary);
        assert_eq!(body.lines(BodyView::Raw), None);
        assert_eq!(
            lines(&BodyText::new(b"", None), BodyView::Raw),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_unresolved_line_lists_the_names() {
        assert_eq!(unresolved_text(&[]), None);
        assert_eq!(
            unresolved_text(&["a".to_owned(), "b".to_owned()]).as_deref(),
            Some("Sent with unresolved variables: a, b")
        );
    }
}
