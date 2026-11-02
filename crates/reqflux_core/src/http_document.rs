//! The `.http`-style document format behind the Document layout (v1-features S7):
//! requests separated by `### <name>` lines, a `<METHOD> <url>` line, `Key: value`
//! headers, a blank line and a verbatim body — the shape of
//! `design/layouts/02-document-layout.html`. The response block drawn under a section in
//! that artboard is UI-only and not part of the document text.
//!
//! # Separator rule
//!
//! A line opens a new section when it starts with `###` at column zero and either the
//! body has not begun yet or the previous line is blank. Everything else is content:
//!
//! - an indented `###` line inside a body is body text, never a separator;
//! - a `###` line that directly follows body text is body text, never a separator.
//!
//! `write` always emits a blank line between sections, so canonical documents are
//! unambiguous. The cost is one documented limitation: a body cannot contain a blank
//! line followed by a `###` line at column zero, because that pair always opens a new
//! section.
//!
//! # Other rules
//!
//! - `#` and `//` comment lines are ignored in the head of a section (before the first
//!   blank line). They are not stored and `write` never emits them.
//! - The head's first content line is the method line: `<METHOD> <url>`, or a URL alone,
//!   which means GET. A lone token counts as a URL unless it holds a `Key:value` header
//!   written without the space after the colon (a colon that is not part of `://`).
//! - A section with no method/URL line produces a warning and is skipped; so does a head
//!   line that is not a `Key: value` header. Parsing never fails.
//! - The body is kept verbatim, except that blank lines right before the next separator
//!   belong to the separation, not the body.
//! - Header keys must not contain whitespace: a key with a space would be read back as a
//!   method line.
//! - The first section may start without a `###` line; it is unnamed.
//!
//! [`parse`] and [`write`] are inverse on canonical documents:
//! `parse(write(sections)) == sections`, and a document already in canonical form
//! round-trips byte for byte.

use crate::model::{Body, Id, KeyValue, Method, RawLanguage, Request};

/// One request section of a `.http` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRequest {
    /// The name from the `### <name>` line; empty for the unnamed first section.
    pub name: String,
    /// The HTTP method, uppercase.
    pub method: Method,
    /// The URL, kept verbatim including its query string.
    pub url: String,
    /// The headers, in document order.
    pub headers: Vec<KeyValue>,
    /// The body text after the first blank line, verbatim; empty when absent.
    pub body: String,
}

impl DocumentRequest {
    /// Turns the section into a model [`Request`] the layout can open: the URL's query
    /// string moves into params (with `{{variables}}` kept verbatim) and a non-empty body
    /// becomes [`Body::Raw`], JSON when a `Content-Type` header mentions JSON, plain text
    /// otherwise. The caller assigns the `id`.
    #[must_use]
    pub fn to_request(&self, id: Id) -> Request {
        let (url, params) = split_query(&self.url);
        let mut request = Request::new(id, self.name.clone(), self.method.clone(), url);
        request.params = params;
        request.headers = self.headers.clone();
        if !self.body.is_empty() {
            request.body = Body::Raw {
                language: self.body_language(),
                text: self.body.clone(),
            };
        }
        request
    }

    /// JSON when a `Content-Type` header mentions JSON, plain text otherwise.
    fn body_language(&self) -> RawLanguage {
        let json = self.headers.iter().any(|header| {
            header.key.eq_ignore_ascii_case("content-type")
                && header.value.to_ascii_lowercase().contains("json")
        });
        if json {
            RawLanguage::Json
        } else {
            RawLanguage::Text
        }
    }
}

/// Splits the query string off a URL, ignoring a `?` inside a `{{template}}`.
fn split_query(url: &str) -> (String, Vec<KeyValue>) {
    let mut base = String::new();
    let mut query = String::new();
    let mut in_query = false;
    let mut braces = 0usize;
    for character in url.chars() {
        match character {
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            // Only the first unbraced `?` starts the query; later ones are data.
            '?' if braces == 0 && !in_query => {
                in_query = true;
                continue;
            }
            _ => {}
        }
        if in_query {
            query.push(character);
        } else {
            base.push(character);
        }
    }
    let params = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => KeyValue::new(key, value),
            None => KeyValue::new(pair, ""),
        })
        .collect();
    (base, params)
}

/// A section being built: the head (name, method line, headers) and, once a blank line
/// has passed, the verbatim body lines.
struct Section {
    name: String,
    start_line: usize,
    request: Option<(Method, String)>,
    headers: Vec<KeyValue>,
    body: Vec<String>,
    skip: bool,
}

impl Section {
    fn new(name: String, start_line: usize) -> Self {
        Self {
            name,
            start_line,
            request: None,
            headers: Vec::new(),
            body: Vec::new(),
            skip: false,
        }
    }

    /// `<unnamed>` for warning text when the section has no `###` name.
    fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "<unnamed>"
        } else {
            self.name.as_str()
        }
    }

    /// Classifies one head line: the method line, a header, or a warning.
    fn head_line(&mut self, line: &str, line_number: usize, warnings: &mut Vec<String>) {
        if self.skip {
            return;
        }
        let trimmed = line.trim();
        let mut tokens = trimmed.split_whitespace();
        let Some(first) = tokens.next() else {
            return;
        };
        let has_more = tokens.next().is_some();
        let header_shaped = if has_more {
            first.contains(':')
        } else {
            // A lone token is a URL (GET) unless it is a `Key:value` header written
            // without the space after the colon.
            first.contains(':') && !first.contains("://")
        };
        if self.request.is_none() {
            if header_shaped {
                warnings.push(format!(
                    "section `{}` (line {line_number}): no method or URL line; section skipped",
                    self.display_name()
                ));
                self.skip = true;
                return;
            }
            // `METHOD url` when more tokens follow; a lone token is a bare URL, GET.
            let (method, url) = if has_more {
                let method = Method::new(first).unwrap_or(Method::GET);
                let url = trimmed.strip_prefix(first).map(str::trim).unwrap_or("");
                (method, url.to_owned())
            } else {
                (Method::GET, trimmed.to_owned())
            };
            self.request = Some((method, url));
            return;
        }
        match trimmed.split_once(':') {
            Some((key, value)) => self.headers.push(KeyValue::new(key.trim(), value.trim())),
            None => warnings.push(format!(
                "section `{}` (line {line_number}): `{line}` is not a `Key: value` header; line ignored",
                self.display_name()
            )),
        }
    }
}

/// A line opens a section when it starts with `###` at column zero (see the module doc
/// for the blank-line rule the caller applies).
fn is_separator(line: &str) -> bool {
    line.starts_with("###")
}

/// A head comment: a trimmed `#` or `//` line. `###` at column zero never reaches here,
/// because it is checked as a separator first.
fn is_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// Closes a section: strips the blank lines that belong to the next separator when
/// `strip_body_blanks` is set (EOF keeps them verbatim), then either records the
/// warning-and-skip or pushes the finished request.
fn finish_section(
    mut section: Section,
    strip_body_blanks: bool,
    requests: &mut Vec<DocumentRequest>,
    warnings: &mut Vec<String>,
) {
    if strip_body_blanks {
        while section.body.last().is_some_and(String::is_empty) {
            section.body.pop();
        }
    }
    if section.skip {
        return;
    }
    let Some((method, url)) = section.request else {
        warnings.push(format!(
            "section `{}` (line {}): no method or URL line; section skipped",
            section.display_name(),
            section.start_line
        ));
        return;
    };
    requests.push(DocumentRequest {
        name: section.name,
        method,
        url,
        headers: section.headers,
        body: section.body.join("\n"),
    });
}

/// Parses a document into one [`DocumentRequest`] per section, plus warnings for
/// sections and head lines that do not follow the format. Never fails.
pub fn parse(text: &str) -> (Vec<DocumentRequest>, Vec<String>) {
    let mut requests = Vec::new();
    let mut warnings = Vec::new();
    let mut section: Option<Section> = None;
    let mut in_body = false;
    let mut previous_blank = false;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let blank = line.trim().is_empty();

        let opens = is_separator(line) && (!in_body || previous_blank);
        if opens {
            if let Some(done) = section.take() {
                finish_section(done, true, &mut requests, &mut warnings);
            }
            section = Some(Section::new(section_name(line), line_number));
            in_body = false;
            previous_blank = false;
            continue;
        }

        match section.as_mut() {
            // Content before the first `###` line starts the unnamed first section.
            None => {
                if blank || is_comment(line) {
                    continue;
                }
                let mut fresh = Section::new(String::new(), line_number);
                fresh.head_line(line, line_number, &mut warnings);
                section = Some(fresh);
            }
            Some(current) => {
                if in_body {
                    current.body.push(line.to_owned());
                } else if blank {
                    in_body = true;
                } else if is_comment(line) {
                    // Head comments are ignored.
                } else {
                    current.head_line(line, line_number, &mut warnings);
                }
            }
        }
        previous_blank = blank;
    }
    if let Some(done) = section.take() {
        finish_section(done, false, &mut requests, &mut warnings);
    }
    (requests, warnings)
}

/// The name after the `###` marker, trimmed; a bare `###` line names nothing.
fn section_name(line: &str) -> String {
    line.strip_prefix("###")
        .map(str::trim)
        .unwrap_or("")
        .to_owned()
}

/// Writes sections back to document text: per section the `### name` line, the method
/// line, the headers, a blank line and the body; one blank line between sections; one
/// final newline. Deterministic.
#[must_use]
pub fn write(requests: &[DocumentRequest]) -> String {
    let sections: Vec<String> = requests.iter().map(section_text).collect();
    let mut document = sections.join("\n\n");
    if !document.is_empty() {
        document.push('\n');
    }
    document
}

fn section_text(request: &DocumentRequest) -> String {
    let mut lines = vec![format!("### {}", request.name)];
    lines.push(format!("{} {}", request.method, request.url));
    for header in &request.headers {
        lines.push(format!("{}: {}", header.key, header.value));
    }
    if !request.body.is_empty() {
        lines.push(String::new());
        lines.extend(request.body.split('\n').map(str::to_owned));
    }
    lines.join("\n")
}
