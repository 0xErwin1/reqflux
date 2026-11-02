//! The JSONPath filter of the response body without GPUI (response-views/02): the
//! engine's [`jsonpath::filter`] over the body, its matches as the lines the body view
//! shows and their count, or the engine's message for the error banner. The body itself
//! never changes.

use std::sync::Arc;

use reqflux_engine::jsonpath;

/// The placeholder of the JSONPath field.
pub(crate) const JSONPATH_PLACEHOLDER: &str = "Filter with JSONPath";

/// What a JSONPath expression made of the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Filtered {
    /// The matches as a pretty JSON array, one line each, in document order.
    Matches { lines: Arc<[String]>, count: usize },
    /// Why there is no result, the engine's message as is.
    Error(String),
}

impl Filtered {
    /// Applies `expression` to `body`; `None` for a blank expression, which filters
    /// nothing.
    pub(crate) fn new(body: &[u8], expression: &str) -> Option<Self> {
        if expression.trim().is_empty() {
            return None;
        }
        let text = String::from_utf8_lossy(body);
        Some(match jsonpath::filter(&text, expression) {
            Ok(array) => {
                let count = serde_json::from_str::<serde_json::Value>(&array)
                    .ok()
                    .and_then(|value| value.as_array().map(Vec::len))
                    .unwrap_or_default();
                Self::Matches {
                    lines: array.lines().map(str::to_owned).collect(),
                    count,
                }
            }
            Err(error) => Self::Error(error.to_string()),
        })
    }

    /// The count at the end of the field: `1 match`, `<n> matches` or `error`.
    pub(crate) fn count_label(&self) -> String {
        match self {
            Self::Matches { count: 1, .. } => "1 match".to_owned(),
            Self::Matches { count, .. } => format!("{count} matches"),
            Self::Error(_) => "error".to_owned(),
        }
    }

    /// The text Copy puts on the clipboard while the filter shows: the matches array.
    pub(crate) fn text(&self) -> Option<String> {
        match self {
            Self::Matches { lines, .. } => Some(lines.join("\n")),
            Self::Error(_) => None,
        }
    }
}

/// The caption of the body bar while the filter shows: `filtered view of a <size> body`.
pub(crate) fn filtered_caption(size: &str) -> String {
    format!("filtered view of a {size} body")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &[u8] =
        br#"{"data":[{"id":"a","status":"open"},{"id":"b","status":"paid"},{"id":"c","status":"open"}]}"#;

    fn lines(filtered: &Option<Filtered>) -> Vec<String> {
        match filtered {
            Some(Filtered::Matches { lines, .. }) => lines.to_vec(),
            other => panic!("matches expected, got {other:?}"),
        }
    }

    #[test]
    fn a_filter_shows_the_matches_as_a_pretty_array_with_their_count() {
        let filtered = Filtered::new(BODY, "$.data[?@.status == 'open'].id");
        assert_eq!(lines(&filtered), ["[", "  \"a\",", "  \"c\"", "]"]);
        assert_eq!(
            filtered.as_ref().map(Filtered::count_label).as_deref(),
            Some("2 matches")
        );
        assert_eq!(
            filtered.and_then(|filtered| filtered.text()).as_deref(),
            Some("[\n  \"a\",\n  \"c\"\n]")
        );
        let one = Filtered::new(BODY, "$.data[0].id");
        assert_eq!(
            one.as_ref().map(Filtered::count_label).as_deref(),
            Some("1 match")
        );
    }

    #[test]
    fn nothing_matching_is_an_empty_array_not_an_error() {
        let filtered = Filtered::new(BODY, "$.data[?@.status == 'void'].id");
        assert_eq!(lines(&filtered), ["[]"]);
        assert_eq!(
            filtered.as_ref().map(Filtered::count_label).as_deref(),
            Some("0 matches")
        );
    }

    #[test]
    fn a_blank_expression_filters_nothing() {
        assert_eq!(Filtered::new(BODY, ""), None);
        assert_eq!(Filtered::new(BODY, "   "), None);
    }

    #[test]
    fn an_invalid_expression_and_a_body_that_is_not_json_carry_the_engine_message() {
        let Some(Filtered::Error(message)) = Filtered::new(BODY, "$.data[?@.status ==") else {
            panic!("an error");
        };
        assert!(
            message.starts_with("invalid JSONPath expression: "),
            "{message}"
        );
        let Some(Filtered::Error(message)) = Filtered::new(b"<!doctype html>", "$.data") else {
            panic!("an error");
        };
        assert!(message.starts_with("the body is not JSON: "), "{message}");
        let error = Filtered::Error(message);
        assert_eq!(error.count_label(), "error");
        assert_eq!(error.text(), None);
    }

    #[test]
    fn the_caption_names_the_body_size() {
        assert_eq!(filtered_caption("1.2 KB"), "filtered view of a 1.2 KB body");
    }
}
