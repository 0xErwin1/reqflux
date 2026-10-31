//! Transport errors.

use std::io;
use std::path::PathBuf;

/// Why a request could not produce a response.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    /// The URL does not parse.
    #[error("invalid URL {url}: {reason}")]
    InvalidUrl {
        /// The URL as given.
        url: String,
        /// The parse error.
        reason: String,
    },
    /// The method is not a valid HTTP token.
    #[error("invalid method {0}")]
    InvalidMethod(String),
    /// A header name or value is not valid HTTP.
    #[error("invalid header {name}: {reason}")]
    InvalidHeader {
        /// The header name as given.
        name: String,
        /// Why the name or value was rejected.
        reason: String,
    },
    /// The request did not finish within its timeout.
    #[error("request timed out after {0} ms")]
    Timeout(u128),
    /// The connection to the host failed.
    #[error("could not connect to {host}: {reason}")]
    Connect {
        /// The host of the URL.
        host: String,
        /// The underlying error.
        reason: String,
    },
    /// The TLS handshake or certificate verification failed.
    #[error("TLS error: {0}")]
    Tls(String),
    /// A body or form file could not be read.
    #[error("{}: {source}", path.display())]
    File {
        /// The file path.
        path: PathBuf,
        /// The read error.
        source: io::Error,
    },
    /// The redirect check refused a redirect target, which was not sent anything.
    #[error("a redirect to {url} was refused: {reason}")]
    RedirectRefused {
        /// The refused target.
        url: String,
        /// The check's refusal.
        reason: String,
    },
    /// The caller cancelled the request.
    #[error("request cancelled")]
    Cancelled,
    /// The HTTP client could not be built.
    #[error("could not build HTTP client: {0}")]
    Client(String),
    /// Any other transport failure, such as too many redirects.
    #[error("{0}")]
    Request(String),
}
