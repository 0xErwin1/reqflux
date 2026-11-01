//! The query string (S6).

/// Appends `query` to `base`, percent-encoded, after `&` when `base` already has a `?`.
pub(super) fn with_query(base: String, query: &[(String, String)]) -> String {
    let encoded: Vec<(String, String)> = query
        .iter()
        .map(|(key, value)| (encode(key), encode(value)))
        .collect();
    append(base, &encoded)
}

/// Appends `query` to `base` as written, without encoding, after the same separator as
/// [`with_query`] and before a `#fragment`; front ends show requests this way.
pub(super) fn append(base: String, query: &[(String, String)]) -> String {
    reqflux_core::query::append(base, query)
}

/// Percent-encodes every byte except the RFC 3986 unreserved characters.
pub(super) fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
