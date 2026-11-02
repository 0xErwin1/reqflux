//! The response the transport returns.

use std::time::Duration;

use crate::timing::Timings;

/// A received response with its body fully read and decompressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The canonical reason phrase of the status code, when it has one.
    pub reason: Option<String>,
    /// The HTTP version, such as `1.1` or `2`.
    pub version: String,
    /// Response headers in received order, names lowercase; a name may repeat.
    pub headers: Vec<(String, String)>,
    /// The decompressed body bytes.
    pub body: Vec<u8>,
    /// The URL of the final response, after redirects.
    pub final_url: String,
    /// Time from sending the request to reading the whole body.
    pub elapsed: Duration,
    /// The phases of every request of the send, redirects first.
    pub timings: Timings,
}

impl Response {
    /// The size of the decompressed body in bytes.
    pub fn size(&self) -> usize {
        self.body.len()
    }
}
