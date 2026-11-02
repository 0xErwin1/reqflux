//! Sending a request without GPUI: the send state of an open request, the outcome of its
//! last send and the texts the response shows for them.

use std::time::{Duration, Instant};

use std::sync::Arc;

use reqflux_engine::{Response, ScriptOutcome};

use crate::shell::response::{BodyText, PreviewKind, preview_kind};

/// The outcome of a finished send.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Completed {
    /// The server answered.
    Response(Received),
    /// The request could not be prepared or sent.
    Failure(Failure),
}

/// A response the server sent back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Received {
    pub(crate) status: u16,
    pub(crate) reason: Option<String>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) elapsed: Duration,
    /// The timing breakdown, one entry per hop; empty when none was recorded.
    pub(crate) timings: reqflux_engine::Timings,
    /// Variable names that did not resolve, in first-occurrence order.
    pub(crate) unresolved: Vec<String>,
    /// What the effective scripts produced: test results, console entries and variable
    /// writes, pre-request entries first.
    pub(crate) scripts: ScriptOutcome,
    /// The body split into lines for the view, computed once.
    pub(crate) text: Arc<BodyText>,
    /// What the body previews as; `None` offers no Preview.
    pub(crate) preview: Option<PreviewKind>,
}

/// A send that ended without a response.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub(crate) error: String,
    pub(crate) elapsed: Duration,
    /// Variable names that did not resolve, in first-occurrence order.
    pub(crate) unresolved: Vec<String>,
    /// What the effective pre-request script produced before the failure: test results,
    /// console entries and variable writes. Empty when the request could not be prepared.
    pub(crate) scripts: ScriptOutcome,
}

impl Received {
    /// Splits the body into lines, so build it off the UI thread for a large body.
    pub(crate) fn new(response: Response, unresolved: Vec<String>, scripts: ScriptOutcome) -> Self {
        let content_type = content_type(&response.headers);
        let text = Arc::new(BodyText::new(&response.body, content_type));
        let preview = preview_kind(content_type, &response.body);
        Self {
            status: response.status,
            reason: response.reason,
            headers: response.headers,
            body: response.body,
            elapsed: response.elapsed,
            timings: response.timings,
            unresolved,
            scripts,
            text,
            preview,
        }
    }

    /// The body size in bytes.
    pub(crate) fn size(&self) -> u64 {
        u64::try_from(self.body.len()).unwrap_or(u64::MAX)
    }

    /// The value of the first `Content-Type` header, matched case-insensitively.
    pub(crate) fn content_type(&self) -> Option<&str> {
        content_type(&self.headers)
    }

    /// The status chip text: `<code> <reason>`, or the code alone without a reason.
    pub(crate) fn status_text(&self) -> String {
        status_text(self.status, self.reason.as_deref())
    }
}

fn content_type(headers: &[(String, String)]) -> Option<&str> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.as_str())
}

/// Where the send of one open request is.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum SendState {
    /// Never sent since the tab opened.
    #[default]
    Idle,
    /// In flight since `started`; `previous` is the outcome of the send before it.
    Sending {
        started: Instant,
        previous: Option<Completed>,
    },
    /// The last send finished.
    Done(Completed),
}

impl SendState {
    /// The outcome of the last finished send.
    pub(crate) fn last(&self) -> Option<&Completed> {
        match self {
            Self::Idle => None,
            Self::Sending { previous, .. } => previous.as_ref(),
            Self::Done(completed) => Some(completed),
        }
    }

    /// A new send started at `started`, keeping the last outcome as the previous one.
    pub(crate) fn start(&mut self, started: Instant) {
        let previous = self.last().cloned();
        *self = Self::Sending { started, previous };
    }

    /// The send finished with `completed`.
    pub(crate) fn finish(&mut self, completed: Completed) {
        *self = Self::Done(completed);
    }

    pub(crate) fn is_sending(&self) -> bool {
        matches!(self, Self::Sending { .. })
    }

    /// The state as one line of text, for tests.
    #[cfg(test)]
    pub(crate) fn text(&self) -> Option<String> {
        match self {
            Self::Idle => None,
            Self::Sending { .. } => Some("Sending".to_owned()),
            Self::Done(Completed::Response(received)) => Some(received.status_text()),
            Self::Done(Completed::Failure(failure)) => Some(format!("Failed: {}", failure.error)),
        }
    }
}

/// The class of a status code, which colors its chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusClass {
    Success,
    Info,
    Warning,
    Danger,
}

impl StatusClass {
    /// 2xx success, 1xx and 3xx info, 4xx warning, anything else danger.
    pub(crate) fn of(status: u16) -> Self {
        match status {
            200..=299 => Self::Success,
            100..=199 | 300..=399 => Self::Info,
            400..=499 => Self::Warning,
            _ => Self::Danger,
        }
    }
}

/// `<code> <reason>`, or the code alone without a reason.
pub(crate) fn status_text(status: u16, reason: Option<&str>) -> String {
    match reason {
        Some(reason) if !reason.is_empty() => format!("{status} {reason}"),
        Some(_) | None => status.to_string(),
    }
}

/// `184 ms` under a second, else seconds with two decimals: `1.84 s`.
pub(crate) fn elapsed_text(elapsed: Duration) -> String {
    if elapsed < Duration::from_secs(1) {
        format!("{} ms", elapsed.as_millis())
    } else {
        format!("{:.2} s", elapsed.as_secs_f64())
    }
}

/// `512 B` under a kilobyte, else one decimal of `KB`, `MB` or `GB`; 1 KB is 1024 B.
pub(crate) fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[expect(clippy::cast_precision_loss, reason = "a size shown with one decimal")]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = "KB";
    for next in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn received(status: u16, reason: Option<&str>) -> Received {
        Received {
            status,
            reason: reason.map(str::to_owned),
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: b"{}".to_vec(),
            elapsed: Duration::from_millis(5),
            timings: Default::default(),
            unresolved: Vec::new(),
            scripts: ScriptOutcome::default(),
            text: Arc::new(BodyText::new(b"{}", None)),
            preview: None,
        }
    }

    #[test]
    fn elapsed_is_milliseconds_under_a_second_else_seconds() {
        assert_eq!(elapsed_text(Duration::ZERO), "0 ms");
        assert_eq!(elapsed_text(Duration::from_millis(184)), "184 ms");
        assert_eq!(elapsed_text(Duration::from_millis(999)), "999 ms");
        assert_eq!(elapsed_text(Duration::from_millis(1000)), "1.00 s");
        assert_eq!(elapsed_text(Duration::from_millis(1840)), "1.84 s");
    }

    #[test]
    fn size_uses_binary_units_with_one_decimal() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(1023), "1023 B");
        assert_eq!(size_text(1024), "1.0 KB");
        assert_eq!(size_text(1229), "1.2 KB");
        assert_eq!(size_text(3_565_158), "3.4 MB");
        assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn status_classes_follow_the_first_digit() {
        assert_eq!(StatusClass::of(101), StatusClass::Info);
        assert_eq!(StatusClass::of(200), StatusClass::Success);
        assert_eq!(StatusClass::of(299), StatusClass::Success);
        assert_eq!(StatusClass::of(304), StatusClass::Info);
        assert_eq!(StatusClass::of(404), StatusClass::Warning);
        assert_eq!(StatusClass::of(503), StatusClass::Danger);
        assert_eq!(StatusClass::of(99), StatusClass::Danger);
        assert_eq!(StatusClass::of(600), StatusClass::Danger);
    }

    #[test]
    fn status_text_drops_a_missing_reason() {
        assert_eq!(status_text(200, Some("OK")), "200 OK");
        assert_eq!(status_text(299, None), "299");
        assert_eq!(status_text(299, Some("")), "299");
    }

    #[test]
    fn a_response_reports_its_size_and_content_type() {
        let mut response = received(200, Some("OK"));
        response
            .headers
            .insert(0, ("X-A".to_owned(), "1".to_owned()));
        assert_eq!(response.size(), 2);
        assert_eq!(response.content_type(), Some("application/json"));
        response.headers.clear();
        assert_eq!(response.content_type(), None);
    }

    #[test]
    fn a_send_keeps_the_previous_outcome_until_it_finishes() {
        let mut state = SendState::default();
        assert_eq!(state.text(), None);
        assert_eq!(state.last(), None);

        let now = Instant::now();
        state.start(now);
        assert!(state.is_sending());
        assert_eq!(state.last(), None);
        assert_eq!(state.text().as_deref(), Some("Sending"));

        let first = Completed::Response(received(200, Some("OK")));
        state.finish(first.clone());
        assert_eq!(state.text().as_deref(), Some("200 OK"));

        state.start(now);
        assert_eq!(
            state,
            SendState::Sending {
                started: now,
                previous: Some(first.clone()),
            }
        );
        assert_eq!(state.last(), Some(&first));

        state.finish(Completed::Failure(Failure {
            error: "request cancelled".to_owned(),
            elapsed: Duration::ZERO,
            unresolved: Vec::new(),
            scripts: ScriptOutcome::default(),
        }));
        assert!(!state.is_sending());
        assert_eq!(state.text().as_deref(), Some("Failed: request cancelled"));
    }
}
