//! JSONPath filtering of a JSON response body (RFC 9535), a view over the raw bytes.
//!
//! The expression parser and evaluator are `serde_json_path`'s, which implements RFC 9535
//! and runs the JSONPath compliance test suite, so filters behave the same as in other
//! RFC 9535 tools instead of one of the older, mutually incompatible dialects.

use serde_json::Value;
use serde_json_path::JsonPath;

/// Why a JSONPath filter produced no result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonPathError {
    /// The expression is not valid RFC 9535 JSONPath.
    #[error("invalid JSONPath expression: {0}")]
    Expression(String),
    /// The body is not JSON.
    #[error("the body is not JSON: {0}")]
    Body(String),
}

/// Applies the JSONPath `expression` to the JSON `body` and returns the matched values,
/// in document order, as a pretty-printed JSON array (`[]` when nothing matches).
///
/// # Errors
///
/// The expression is not valid JSONPath, or the body is not JSON.
pub fn filter(body: &str, expression: &str) -> Result<String, JsonPathError> {
    let path = JsonPath::parse(expression.trim())
        .map_err(|error| JsonPathError::Expression(error.to_string()))?;
    let document: Value =
        serde_json::from_str(body).map_err(|error| JsonPathError::Body(error.to_string()))?;
    let matched: Vec<&Value> = path.query(&document).all();
    serde_json::to_string_pretty(&matched).map_err(|error| JsonPathError::Body(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const BODY: &str = r#"{"store": {"book": [
        {"title": "A", "price": 8},
        {"title": "B", "price": 12}
    ], "open": true}}"#;

    #[test]
    fn matched_values_come_back_pretty_printed_in_document_order() {
        assert_eq!(
            filter(BODY, "$.store.book[*].title"),
            Ok("[\n  \"A\",\n  \"B\"\n]".to_owned())
        );
    }

    #[test]
    fn a_filter_selector_selects_by_value() {
        assert_eq!(
            filter(BODY, "$.store.book[?@.price > 10]"),
            Ok("[\n  {\n    \"title\": \"B\",\n    \"price\": 12\n  }\n]".to_owned())
        );
    }

    #[test]
    fn no_match_is_an_empty_array() {
        assert_eq!(filter(BODY, "$.missing"), Ok("[]".to_owned()));
    }

    #[test]
    fn a_bad_expression_is_an_error_message() {
        let error = filter(BODY, "$.store[").unwrap_err();
        assert!(matches!(error, JsonPathError::Expression(_)), "{error:?}");
        assert!(
            error
                .to_string()
                .starts_with("invalid JSONPath expression: "),
            "{error}"
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_an_error_message() {
        let error = filter("<html/>", "$").unwrap_err();
        assert!(matches!(error, JsonPathError::Body(_)), "{error:?}");
        assert!(
            error.to_string().starts_with("the body is not JSON: "),
            "{error}"
        );
    }
}
