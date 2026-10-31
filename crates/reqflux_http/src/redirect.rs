//! One redirect hop: where it goes, and what the next request keeps.
//!
//! The client follows redirects itself rather than through `reqwest`'s policy, because a
//! policy can only follow or stop: it cannot drop a header that holds a secret before a hop
//! to another origin.

use reqwest::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue,
    LOCATION, PROXY_AUTHORIZATION, REFERER, TRANSFER_ENCODING, WWW_AUTHENTICATE,
};
use reqwest::{Method, StatusCode};
use url::Url;

use crate::client::SecretMask;
use crate::request::PreparedBody;

/// The target of a redirect response, resolved against `current`: none when the status is
/// not a redirect, or its `Location` is missing, invalid or not HTTP(S), in which case the
/// response is the final one, as with `reqwest`.
pub(crate) fn target(response: &reqwest::Response, current: &Url) -> Option<Url> {
    if !matches!(
        response.status(),
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    ) {
        return None;
    }
    let location = response.headers().get(LOCATION)?.to_str().ok()?;
    let next = current.join(location).ok()?;
    matches!(next.scheme(), "http" | "https").then_some(next)
}

/// Whether a redirect with `status` resends the method and the body (307 and 308).
pub(crate) fn keeps_body(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT
    )
}

/// A copy of `body` to resend on a 307 or 308: none for a multipart form or a file, which
/// are streamed once, so such a redirect is returned as the response, as with `reqwest`.
pub(crate) fn replayable(body: &PreparedBody) -> Option<PreparedBody> {
    match body {
        PreparedBody::None | PreparedBody::Bytes { .. } | PreparedBody::Form(_) => {
            Some(body.clone())
        }
        PreparedBody::Multipart(_) | PreparedBody::File(_) => None,
    }
}

/// Turns the request into the GET a 301, 302 or 303 asks for: no body and no header
/// describing one; HEAD stays HEAD.
pub(crate) fn to_get(method: &mut Method, headers: &mut HeaderMap) {
    for name in [
        TRANSFER_ENCODING,
        CONTENT_ENCODING,
        CONTENT_TYPE,
        CONTENT_LENGTH,
    ] {
        headers.remove(name);
    }
    if *method != Method::HEAD {
        *method = Method::GET;
    }
}

/// Sets the `Referer` of the hop from `previous` to `next`, as `reqwest` does: the previous
/// URL without credentials or fragment, never from HTTPS to HTTP.
pub(crate) fn set_referer(headers: &mut HeaderMap, previous: &Url, next: &Url) {
    if previous.scheme() == "https" && next.scheme() == "http" {
        return;
    }
    let mut referer = previous.clone();
    // Clearing them fails only on URLs that cannot have them, which then have none.
    if referer.set_username("").is_err() || referer.set_password(None).is_err() {
        return;
    }
    referer.set_fragment(None);
    if let Ok(value) = HeaderValue::from_str(referer.as_str()) {
        headers.insert(REFERER, value);
    }
}

/// Drops, before a hop to another origin, the standard credential headers and every header
/// whose name or value holds a secret of `mask`.
pub(crate) fn strip_cross_origin(headers: &mut HeaderMap, mask: Option<&SecretMask>) {
    for name in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, WWW_AUTHENTICATE] {
        headers.remove(name);
    }
    headers.remove("cookie2");
    let Some(mask) = mask else {
        return;
    };
    let secret: Vec<_> = headers
        .iter()
        .filter(|(name, value)| {
            holds_secret(mask, name.as_str())
                || holds_secret(mask, &String::from_utf8_lossy(value.as_bytes()))
        })
        .map(|(name, _)| name.clone())
        .collect();
    for name in secret {
        headers.remove(name);
    }
}

/// Whether `body` holds a secret of `mask`.
pub(crate) fn body_holds_secret(body: &PreparedBody, mask: &SecretMask) -> bool {
    match body {
        PreparedBody::Bytes { data, .. } => holds_secret(mask, &String::from_utf8_lossy(data)),
        PreparedBody::Form(fields) => fields
            .iter()
            .any(|(name, value)| holds_secret(mask, name) || holds_secret(mask, value)),
        PreparedBody::None | PreparedBody::Multipart(_) | PreparedBody::File(_) => false,
    }
}

fn holds_secret(mask: &SecretMask, text: &str) -> bool {
    mask(text) != text
}
