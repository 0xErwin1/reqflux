//! Auth signing that needs the request as sent: digest answers (RFC 7616) and AWS
//! Signature Version 4. Pure functions; the engine decides when to call them.

pub mod digest;
pub mod sigv4;

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail.
        if write!(text, "{byte:02x}").is_err() {
            return text;
        }
        text
    })
}
