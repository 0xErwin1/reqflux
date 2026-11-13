//! The request a hop puts on the wire (sent-headers S2, S5).
//!
//! `reqwest` builds a request and then adds more before the bytes leave: the client's default
//! headers, the `Accept-Encoding` of the decompression layer, and, in `hyper`, `Host`,
//! `Content-Length` and `Transfer-Encoding`. [`WireRequest::of`] applies the same rules, in the
//! same order, to the built request, so the recorded headers equal the request head of an
//! HTTP/1.1 connection (the raw-capture tests in `tests/sent_hops.rs` pin that).
//!
//! Known gaps: a `Proxy-Authorization` that `reqwest` adds for an HTTP proxy is not recorded;
//! over HTTP/2 the wire has `:authority` instead of `Host`, and `host` is recorded anyway; a
//! `Transfer-Encoding` the request sets itself makes `hyper` drop the `Content-Length`, which
//! is not reproduced here.

use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, HOST, HeaderMap, HeaderName, HeaderValue,
    TRANSFER_ENCODING, USER_AGENT,
};

/// The `Accept` default of `reqwest::Client`.
const DEFAULT_ACCEPT: &str = "*/*";

/// The `Accept-Encoding` the decompression layer adds with the gzip, deflate, brotli and zstd
/// features of `reqwest` all enabled (`tower-http`'s `AcceptEncoding::to_header_value`).
const DEFAULT_ACCEPT_ENCODING: &str = "zstd,gzip,deflate,br";

/// The request one hop sent: its method, URL and the headers on the wire, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WireRequest {
    /// The HTTP method.
    pub method: String,
    /// The URL of the hop.
    pub url: String,
    /// The headers sent, in order, names lowercase like `Response.headers`.
    pub headers: Vec<(String, String)>,
}

impl WireRequest {
    /// What `request` puts on the wire when sent by a client whose default `User-Agent` is
    /// `user_agent`.
    pub(crate) fn of(request: &reqwest::Request, user_agent: &'static str) -> Self {
        let mut headers = request.headers().clone();
        // `reqwest::Client::execute`: the client's default headers, where the request has none.
        set_default(
            &mut headers,
            ACCEPT,
            HeaderValue::from_static(DEFAULT_ACCEPT),
        );
        set_default(
            &mut headers,
            USER_AGENT,
            HeaderValue::from_static(user_agent),
        );
        // The decompression layer.
        set_default(
            &mut headers,
            ACCEPT_ENCODING,
            HeaderValue::from_static(DEFAULT_ACCEPT_ENCODING),
        );
        // `hyper-util`, HTTP/1: `Host` without the default port.
        let url = request.url();
        if let Some(host) = url.host_str() {
            let value = match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            };
            if let Ok(value) = HeaderValue::from_str(&value) {
                set_default(&mut headers, HOST, value);
            }
        }
        // `hyper`'s `set_length`: a known, non-empty body gets its `Content-Length`, a body of
        // unknown size goes chunked, unless the request says either already.
        if let Some(body) = request.body()
            && !headers.contains_key(CONTENT_LENGTH)
            && !headers.contains_key(TRANSFER_ENCODING)
        {
            match body.as_bytes() {
                Some(bytes) if !bytes.is_empty() => {
                    headers.insert(CONTENT_LENGTH, HeaderValue::from(bytes.len()));
                }
                Some(_) => {}
                None => {
                    headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
                }
            }
        }
        Self {
            method: request.method().as_str().to_owned(),
            url: url.to_string(),
            headers: headers
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect(),
        }
    }
}

fn set_default(headers: &mut HeaderMap, name: HeaderName, value: HeaderValue) {
    if let reqwest::header::Entry::Vacant(entry) = headers.entry(name) {
        entry.insert(value);
    }
}
