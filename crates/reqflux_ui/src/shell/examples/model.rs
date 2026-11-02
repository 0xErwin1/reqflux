//! What the example views say and keep, without GPUI: the default name of a new example, the
//! response a send or a saved example stands for, how many secrets the save masked, the
//! lines of a saved body with the masked ones marked, and the texts of the toasts.

use std::path::Path;

use reqflux_core::model::{ExampleBody, KeyValue};
use reqflux_engine::{MASK, Response, SavedExample, Timings};

use crate::shell::request::{Received, size_text};
use crate::shell::toasts;

/// The name a new example starts with: the reason phrase of its status, else the code.
pub(crate) fn default_name(status: u16, reason: Option<&str>) -> String {
    match reason.map(str::trim) {
        Some(reason) if !reason.is_empty() => reason.to_owned(),
        Some(_) | None => status.to_string(),
    }
}

/// `<code> <reason>`, or the code alone without a reason.
pub(crate) fn status_line(status: u16, reason: Option<&str>) -> String {
    match reason {
        Some(reason) if !reason.is_empty() => format!("{status} {reason}"),
        Some(_) | None => status.to_string(),
    }
}

/// The response a received send stands for, as the engine saves it.
pub(crate) fn received_response(received: &Received) -> Response {
    Response {
        status: received.status,
        reason: received.reason.clone(),
        version: String::new(),
        headers: received.headers.clone(),
        body: received.body.clone(),
        final_url: String::new(),
        elapsed: received.elapsed,
        timings: Timings::default(),
    }
}

/// The response a saved example keeps, for writing its body to a file.
pub(crate) fn saved_response(saved: &SavedExample) -> Response {
    let response = &saved.example.response;
    Response {
        status: response.status,
        reason: response.reason.clone(),
        version: String::new(),
        headers: response
            .headers
            .iter()
            .map(|header| (header.key.clone(), header.value.clone()))
            .collect(),
        body: saved.body.clone(),
        final_url: String::new(),
        elapsed: std::time::Duration::ZERO,
        timings: Timings::default(),
    }
}

/// How many secrets `mask` (the send's masker) hides in the parts of `response` an example
/// keeps as text: the reason, the header values and a UTF-8 body.
pub(crate) fn masked_count(response: &Response, mask: impl Fn(&str) -> String) -> usize {
    let hidden = |text: &str| {
        mask(text)
            .matches(MASK)
            .count()
            .saturating_sub(text.matches(MASK).count())
    };
    let body = std::str::from_utf8(&response.body).map_or(0, hidden);
    let reason = response.reason.as_deref().map_or(0, hidden);
    let headers: usize = response
        .headers
        .iter()
        .map(|(_, value)| hidden(value))
        .sum();
    body + reason + headers
}

/// The title of the toast after a save.
pub(crate) fn saved_title(name: &str) -> String {
    format!("Saved example {name}")
}

/// The line under it: the request, and the secrets masked when there were any.
pub(crate) fn saved_subtitle(request: &str, masked: usize) -> String {
    match masked {
        0 => request.to_owned(),
        masked => format!(
            "{request} \u{b7} {} masked in the response",
            toasts::counted(masked, "secret")
        ),
    }
}

/// The line under the toast after a body was written: the path and the size.
pub(crate) fn body_saved_subtitle(path: &Path, bytes: usize) -> String {
    format!(
        "{} \u{b7} {}",
        path.display(),
        size_text(u64::try_from(bytes).unwrap_or(u64::MAX))
    )
}

/// The value of the `content-type` header, without parameters.
pub(crate) fn content_type<'a>(headers: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    headers
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .and_then(|(_, value)| value.split(';').next())
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// The file name the Save dialog suggests for a body: `stem` and the extension of the
/// content type, `.bin` when it says nothing known.
pub(crate) fn suggested_file_name(stem: &str, content_type: &str) -> String {
    let extension = match content_type {
        "application/json" => "json",
        "text/html" => "html",
        "application/xml" | "text/xml" => "xml",
        "text/csv" => "csv",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        kind if kind.starts_with("text/") => "txt",
        kind if kind.ends_with("+json") => "json",
        _ => "bin",
    };
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "response" } else { stem };
    format!("{stem}.{extension}")
}

/// The body file `body` beside the example file `example`, as the workspace names it:
/// `invoice.examples/ok.body`.
pub(crate) fn body_file(example: &str, body: &str) -> String {
    match example.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{body}"),
        None => body.to_owned(),
    }
}

/// Whether `text` holds a value the save masked.
pub(crate) fn is_masked(text: &str) -> bool {
    text.contains(MASK)
}

/// The body of a saved example as the view shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShownBody {
    /// No body was saved.
    Empty,
    /// The text lines, each with whether it holds a masked value.
    Text(Vec<(String, bool)>),
    /// A body that is not UTF-8, kept as received in `file`: its type and size.
    Binary {
        file: String,
        content_type: String,
        bytes: usize,
    },
}

/// What the view shows of the body of `saved`.
pub(crate) fn shown_body(saved: &SavedExample) -> ShownBody {
    match &saved.example.response.body {
        ExampleBody::None => ShownBody::Empty,
        ExampleBody::Text(text) => ShownBody::Text(
            text.lines()
                .map(|line| (line.to_owned(), is_masked(line)))
                .collect(),
        ),
        ExampleBody::File(file) => ShownBody::Binary {
            file: file.clone(),
            content_type: content_type(header_pairs(&saved.example.response.headers)),
            bytes: saved.body.len(),
        },
    }
}

/// `headers` as name and value pairs.
pub(crate) fn header_pairs(headers: &[KeyValue]) -> impl Iterator<Item = (&str, &str)> {
    headers
        .iter()
        .map(|header| (header.key.as_str(), header.value.as_str()))
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
