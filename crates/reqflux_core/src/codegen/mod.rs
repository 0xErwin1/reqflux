//! A request as code to send it elsewhere (v1-features S2).
//!
//! The code is the request as written: `{{variables}}` stay as `{{name}}` and nothing is
//! resolved, so a secret never reaches the output. Disabled params, headers, fields and parts
//! are left out. The `Content-Type` the engine would add for a raw or GraphQL body is
//! written when no header sets one.

mod script;
mod shell;

use std::fmt;
use std::str::FromStr;

use crate::model::{ApiKeyLocation, Auth, Body, KeyValue, RawLanguage, Request};
use crate::query;

/// A target of [`generate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// A curl command for a POSIX shell.
    Curl,
    /// An HTTPie command for a POSIX shell.
    Httpie,
    /// Python with `requests`.
    Python,
    /// JavaScript `fetch`, as an ES module for Node 20 or later.
    Fetch,
}

impl Language {
    /// Every language, in menu order.
    pub const ALL: [Self; 4] = [Self::Curl, Self::Httpie, Self::Python, Self::Fetch];

    /// The name `--lang` takes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Curl => "curl",
            Self::Httpie => "httpie",
            Self::Python => "python",
            Self::Fetch => "fetch",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A language name that [`Language`] does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown language `{0}`: expected curl, httpie, python or fetch")]
pub struct UnknownLanguage(pub String);

impl FromStr for Language {
    type Err = UnknownLanguage;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|language| language.as_str() == name)
            .ok_or_else(|| UnknownLanguage(name.to_owned()))
    }
}

/// The code that sends `request` in `language`. An auth type the language has no form for
/// becomes a comment line on top; `Inherit` sends no auth, so callers pass the effective one.
#[must_use]
pub fn generate(request: &Request, language: Language) -> String {
    let parts = Parts::of(request);
    match language {
        Language::Curl => shell::curl(request, &parts),
        Language::Httpie => shell::httpie(request, &parts),
        Language::Python => script::python(request, &parts),
        Language::Fetch => script::fetch(request, &parts),
    }
}

/// What every language writes the same way.
pub(crate) struct Parts {
    /// The URL with the sent params and an API key in the query.
    url: String,
    /// The enabled headers, then the default `Content-Type`.
    headers: Vec<(String, String)>,
    /// The auth header of an API key or a bearer token, for languages without a flag.
    auth_header: Option<(String, String)>,
    /// The text of a raw or GraphQL body.
    text: Option<String>,
    /// Comment lines for what is not generated.
    notes: Vec<String>,
}

impl Parts {
    fn of(request: &Request) -> Self {
        let mut url = query::compose(&request.url, &request.params);
        let mut auth_header = None;
        let mut notes = Vec::new();
        match &request.auth {
            Auth::ApiKey {
                key,
                value,
                location: ApiKeyLocation::Query,
            } => url = query::append(url, &[(key.clone(), value.clone())]),
            Auth::ApiKey { key, value, .. } => auth_header = Some((key.clone(), value.clone())),
            Auth::Bearer { token } => {
                auth_header = Some(("Authorization".to_owned(), format!("Bearer {token}")));
            }
            Auth::Other { kind, .. } => notes.push(format!("auth {kind} is not generated")),
            Auth::Inherit | Auth::None | Auth::Basic { .. } => {}
        }
        let mut headers: Vec<(String, String)> = request
            .headers
            .iter()
            .filter(|header| header.enabled && !header.key.is_empty())
            .map(|header| (header.key.clone(), header.value.clone()))
            .collect();
        let has_type = headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("Content-Type"));
        let (default_type, text) = match &request.body {
            Body::Raw { language, text } => (Some(content_type(*language)), Some(text.clone())),
            Body::Graphql { query, variables } => {
                (Some("application/json"), Some(graphql(query, variables)))
            }
            _ => (None, None),
        };
        if let Some(default_type) = default_type.filter(|_| !has_type) {
            headers.push(("Content-Type".to_owned(), default_type.to_owned()));
        }
        Self {
            url,
            headers,
            auth_header,
            text,
            notes,
        }
    }
}

/// The enabled entries of `rows` as pairs.
pub(crate) fn sent(rows: &[KeyValue]) -> impl Iterator<Item = (&str, &str)> {
    rows.iter()
        .filter(|row| row.enabled)
        .map(|row| (row.key.as_str(), row.value.as_str()))
}

fn content_type(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Json => "application/json",
        RawLanguage::Xml => "application/xml",
        RawLanguage::Text => "text/plain",
        RawLanguage::Html => "text/html",
        RawLanguage::Javascript => "application/javascript",
    }
}

/// The JSON the engine sends for a GraphQL body; blank variables are `null`, others are
/// written as typed.
fn graphql(query: &str, variables: &str) -> String {
    let variables = if variables.trim().is_empty() {
        "null"
    } else {
        variables
    };
    format!(
        "{{\"query\":{},\"variables\":{variables}}}",
        json_string(query)
    )
}

/// `text` as a double-quoted JSON string literal, which is also a valid JavaScript and
/// Python string literal.
pub(crate) fn json_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{2028}' => quoted.push_str("\\u2028"),
            '\u{2029}' => quoted.push_str("\\u2029"),
            c if c.is_control() => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    quoted.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// Milliseconds as seconds, without trailing zeros: `2500` is `2.5`.
pub(crate) fn seconds(ms: u64) -> String {
    let fraction = ms % 1000;
    if fraction == 0 {
        return (ms / 1000).to_string();
    }
    let fraction = format!("{fraction:03}");
    format!("{}.{}", ms / 1000, fraction.trim_end_matches('0'))
}
