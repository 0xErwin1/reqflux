//! The request as sent, redacted for history (history S3).
//!
//! Preparing a request collects the values that must never be stored in clear: every value
//! of every secret variable in the scopes the request resolves with, and the basic password,
//! bearer token and API key value whether they came from a variable or were typed. The
//! redacted copy replaces each of them by [`MASK`] wherever it appears in the URL, the
//! header values and the body, as text, in the forms a request carries it: as written,
//! percent-encoded, form-encoded, as the `url` crate serializes it into a query or a path,
//! and JSON-string-escaped. A header name that contains a secret value of two or more
//! characters is replaced by [`MASK`] whole; shorter secrets never touch names, so a
//! one-character secret cannot mangle `Content-Type`. The values of `Authorization`,
//! `Proxy-Authorization` and `Cookie` are always masked whole, keeping an auth scheme word,
//! since a script can transform a secret (base64, for example) beyond what text matching
//! finds. A response's `Set-Cookie` is the user's own data and is recorded as received, with
//! only secret values masked; [`Masker::header_for_agent`] also masks its cookie value,
//! keeping the name and attributes, for output an agent reads. A [`Masker`] applies the same replacement to
//! any other text, such as the error of a failed send. Text replacement masks a secret also where something
//! else produced the same characters, so a very short secret value can mask more than it
//! produced; that is the safe side and there is no length floor. Only an empty value is
//! skipped, since it occurs everywhere and masks nothing.

use std::fmt;
use std::sync::Arc;

use reqflux_http::{PreparedBody, PreparedRequest, SecretMask};

/// What a secret is replaced with in a recorded request.
pub const MASK: &str = "••••••••";

/// The request a send put on the wire, with every secret replaced by [`MASK`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentRequest {
    /// The HTTP method.
    pub method: String,
    /// The URL, query string included.
    pub url: String,
    /// The headers in sent order, auth included.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: SentBody,
}

/// The body of a [`SentRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentBody {
    /// No body.
    None,
    /// A text body: raw and GraphQL bodies as sent, URL-encoded forms in their wire form.
    Text(String),
    /// A body that is not kept as text, described: a multipart form, a file, or bytes
    /// that are not UTF-8.
    Omitted(String),
}

/// The values a send must never record in clear. Its `Debug` output shows only how many
/// there are, so it never prints one.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Secrets(Vec<String>);

impl fmt::Debug for Secrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Secrets({} values)", self.0.len())
    }
}

impl Secrets {
    /// Adds `value`, unless it is empty or already known.
    pub(crate) fn add(&mut self, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() && !self.0.contains(&value) {
            self.0.push(value);
        }
    }

    /// The [`Masker`] of these values.
    pub(crate) fn masker(&self) -> Masker {
        Masker {
            patterns: self.patterns().into(),
        }
    }

    /// The values to replace, each as written, percent-encoded the way the query string
    /// and a URL-encoded form carry it, and escaped the way a JSON string carries it,
    /// longest first so a secret that contains another is masked whole.
    fn patterns(&self) -> Vec<String> {
        let mut patterns: Vec<String> = Vec::new();
        for value in &self.0 {
            let forms = [
                value.clone(),
                crate::prepare::percent_encode(value),
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect(),
                json_escaped(value),
                url_query_form(value),
                url_path_form(value),
            ];
            for form in forms {
                // A form can be empty (`#frag` has no query or path): an empty pattern would
                // match between every character.
                if !form.is_empty() && !patterns.contains(&form) {
                    patterns.push(form);
                }
            }
        }
        patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
        patterns
    }
}

/// Replaces the secret values of one send by [`MASK`] in any text, in every form
/// [`SentRequest`] masks them. A front end masks the error of a failed send with it before
/// recording or showing that error, since a transport error can echo the URL. Cheap to
/// clone; its `Debug` output shows only how many patterns it has.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Masker {
    patterns: Arc<[String]>,
}

impl fmt::Debug for Masker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Masker({} patterns)", self.patterns.len())
    }
}

impl Masker {
    /// The transport hook that tells which headers a redirect to another origin drops.
    pub(crate) fn secret_mask(&self) -> SecretMask {
        let masker = self.clone();
        Arc::new(move |text| masker.mask(text))
    }

    /// Whether `text` contains a secret value form of two or more characters.
    fn contains_long(&self, text: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.chars().count() >= 2 && text.contains(pattern.as_str()))
    }

    /// `text` with every secret value replaced by [`MASK`].
    #[must_use]
    pub fn mask(&self, text: &str) -> String {
        self.patterns.iter().fold(text.to_owned(), |text, pattern| {
            text.replace(pattern.as_str(), MASK)
        })
    }

    /// The header `name: value` as a recorded request or response shows it: a credential
    /// header's value masked whole (keeping the auth scheme word), any other masked by text,
    /// and a name that carries a secret replaced by [`MASK`].
    #[must_use]
    pub fn header(&self, name: &str, value: &str) -> (String, String) {
        (header_name(name, self), header_value(name, value, self))
    }

    /// [`Masker::header`] for output an agent reads: a `Set-Cookie` value also has its cookie
    /// value masked, keeping the cookie name and attributes, since a session cookie is a
    /// credential the agent has no reason to hold.
    #[must_use]
    pub fn header_for_agent(&self, name: &str, value: &str) -> (String, String) {
        if name.eq_ignore_ascii_case("set-cookie") {
            (header_name(name, self), self.set_cookie(value))
        } else {
            self.header(name, value)
        }
    }

    /// `value`, a `Set-Cookie` value, with the cookie value replaced by [`MASK`] and the name
    /// and attributes masked by text.
    fn set_cookie(&self, value: &str) -> String {
        let (pair, attributes) = match value.split_once(';') {
            Some((pair, attributes)) => (pair, Some(attributes)),
            None => (value, None),
        };
        let pair = match pair.split_once('=') {
            Some((cookie, _)) => format!("{}={MASK}", self.mask(cookie)),
            None => MASK.to_owned(),
        };
        match attributes {
            Some(attributes) => format!("{pair};{}", self.mask(attributes)),
            None => pair,
        }
    }
}

/// Whether the header `name` carries a credential as its whole value: `Authorization`,
/// `Proxy-Authorization` or `Cookie`, in any case.
#[must_use]
pub fn is_credential_header(name: &str) -> bool {
    CREDENTIAL_HEADERS
        .iter()
        .any(|credential| name.eq_ignore_ascii_case(credential))
}

/// `value` as the `url` crate serializes it in a query string, which leaves characters such
/// as `/` and `:` that [`crate::prepare::percent_encode`] encodes. A transport error echoes
/// the URL in this form.
fn url_query_form(value: &str) -> String {
    url::Url::parse(&format!("http://h/?{value}"))
        .ok()
        .and_then(|url| url.query().map(str::to_owned))
        .unwrap_or_else(|| value.to_owned())
}

/// `value` as the `url` crate serializes it in a path segment.
fn url_path_form(value: &str) -> String {
    url::Url::parse(&format!("http://h/{value}"))
        .ok()
        .and_then(|url| url.path().strip_prefix('/').map(str::to_owned))
        .unwrap_or_else(|| value.to_owned())
}

/// `value` as a JSON string carries it, without the quotes.
fn json_escaped(value: &str) -> String {
    let quoted = serde_json::Value::from(value).to_string();
    quoted
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map_or(quoted.clone(), str::to_owned)
}

/// Headers whose whole value is a credential.
const CREDENTIAL_HEADERS: [&str; 3] = ["authorization", "proxy-authorization", "cookie"];

/// The recorded value of the header `name`: a credential header masked whole, keeping the
/// auth scheme word of `Authorization` and `Proxy-Authorization`; any other masked by text.
fn header_value(name: &str, value: &str, masker: &Masker) -> String {
    if !is_credential_header(name) {
        return masker.mask(value);
    }
    let cookie = name.eq_ignore_ascii_case("cookie");
    let scheme = (!cookie)
        .then(|| value.trim_start().split_once(' '))
        .flatten()
        .map(|(scheme, _)| scheme)
        .filter(|scheme| !scheme.is_empty() && !masker.mask(scheme).contains(MASK));
    match scheme {
        Some(scheme) => format!("{scheme} {MASK}"),
        None => MASK.to_owned(),
    }
}

/// The recorded name of a header: [`MASK`] when it carries a secret of two or more
/// characters, else the name as sent.
fn header_name(name: &str, masker: &Masker) -> String {
    if masker.contains_long(name) {
        MASK.to_owned()
    } else {
        name.to_owned()
    }
}

/// The redacted copy of `request`.
pub(crate) fn redact(request: &PreparedRequest, masker: &Masker) -> SentRequest {
    let mask = |text: &str| masker.mask(text);
    SentRequest {
        method: request.method.clone(),
        url: mask(&request.url),
        headers: request
            .headers
            .iter()
            .map(|(key, value)| (header_name(key, masker), header_value(key, value, masker)))
            .collect(),
        body: match &request.body {
            PreparedBody::None => SentBody::None,
            PreparedBody::Bytes { data, .. } => match std::str::from_utf8(data) {
                Ok(text) => SentBody::Text(mask(text)),
                Err(_) => SentBody::Omitted(format!("{} bytes of binary data", data.len())),
            },
            PreparedBody::Form(fields) => SentBody::Text(mask(
                &url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(fields)
                    .finish(),
            )),
            PreparedBody::Multipart(parts) => {
                SentBody::Omitted(format!("multipart form data, {} parts", parts.len()))
            }
            PreparedBody::File(path) => SentBody::Omitted(format!("file {}", path.display())),
        },
    }
}
