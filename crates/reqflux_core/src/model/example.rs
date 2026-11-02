//! Saved examples (v1-features S4): a request and the response it got, kept beside the
//! request as `<request-slug>.examples/<example-slug>.example.toml`.

use serde::{Deserialize, Serialize};

use super::auth::Auth;
use super::body::Body;
use super::extensions;
use super::id::Id;
use super::method::Method;
use super::variable::KeyValue;

/// `<request-slug>.examples/<example-slug>.example.toml`: one saved example.
///
/// Field order is the on-disk key order. The request is kept as written, `{{variables}}`
/// unresolved, so the file never holds a resolved secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    /// The example id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// The request as written when the example was saved.
    pub request: ExampleRequest,
    /// The response it got.
    pub response: ExampleResponse,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

/// The `[request]` table of an [`Example`]: the parts of a request document that describe
/// what is sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExampleRequest {
    /// The HTTP method.
    pub method: Method,
    /// The URL without its query string; the query lives in `params`.
    #[serde(default)]
    pub url: String,
    /// Query parameters, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    /// Headers, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// The request auth.
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// The request body.
    #[serde(default, skip_serializing_if = "Body::is_none")]
    pub body: Body,
}

/// The `[response]` table of an [`Example`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleResponse {
    /// The status code.
    pub status: u16,
    /// The reason phrase, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The headers in received order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// The body.
    #[serde(default, skip_serializing_if = "ExampleBody::is_none")]
    pub body: ExampleBody,
}

/// The body of an [`ExampleResponse`]: `[response.body]` with `text`, or with `file`, a
/// file name in the same `.examples` directory holding the bytes of a body that is not
/// UTF-8 text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "BodyTable", into = "BodyTable")]
pub enum ExampleBody {
    /// No body.
    #[default]
    None,
    /// A text body.
    Text(String),
    /// A body kept in a sidecar file, named relative to the `.examples` directory.
    File(String),
}

impl ExampleBody {
    /// Whether there is no body.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct BodyTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<String>,
}

impl From<BodyTable> for ExampleBody {
    fn from(table: BodyTable) -> Self {
        match (table.text, table.file) {
            (Some(text), _) => Self::Text(text),
            (None, Some(file)) => Self::File(file),
            (None, None) => Self::None,
        }
    }
}

impl From<ExampleBody> for BodyTable {
    fn from(body: ExampleBody) -> Self {
        match body {
            ExampleBody::None => Self::default(),
            ExampleBody::Text(text) => Self {
                text: Some(text),
                file: None,
            },
            ExampleBody::File(file) => Self {
                text: None,
                file: Some(file),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const FILE: &str = r#"id = "01JB7X3K9Q0000000000000005"
name = "OK"

[request]
method = "GET"
url = "{{base_url}}/v1/invoices"

[[request.params]]
key = "expand"
value = "lines"

[[request.headers]]
key = "Authorization"
value = "Bearer {{token}}"

[response]
status = 200
reason = "OK"

[[response.headers]]
key = "content-type"
value = "application/json"

[response.body]
text = '{"items": []}'
"#;

    #[test]
    fn an_example_round_trips_to_the_same_bytes() {
        let example: Example = toml::from_str(FILE).unwrap();

        assert_eq!(example.name, "OK");
        assert_eq!(example.request.url, "{{base_url}}/v1/invoices");
        assert_eq!(example.response.status, 200);
        assert_eq!(
            example.response.body,
            ExampleBody::Text("{\"items\": []}".to_owned())
        );
        assert_eq!(toml::to_string(&example).unwrap(), FILE);
    }

    #[test]
    fn a_file_body_and_no_body_serialize_as_documented() {
        let mut example: Example = toml::from_str(FILE).unwrap();
        example.response.body = ExampleBody::File("ok.body".to_owned());
        let text = toml::to_string(&example).unwrap();
        assert!(
            text.ends_with("[response.body]\nfile = \"ok.body\"\n"),
            "{text}"
        );
        assert_eq!(toml::from_str::<Example>(&text).unwrap(), example);

        example.response.body = ExampleBody::None;
        let text = toml::to_string(&example).unwrap();
        assert!(!text.contains("[response.body]"), "{text}");
        assert_eq!(toml::from_str::<Example>(&text).unwrap(), example);
    }
}
