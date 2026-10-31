//! The outbound host guard of an execution (user-config-mcp-policy S3, S6): a check every
//! request an execution sends passes first, the main request and every `pm.sendRequest` of
//! its pre-request and test scripts. The window and `reqflux run` pass none; the MCP server
//! passes its host policy.

use std::fmt;
use std::sync::Arc;

/// The check behind a [`HostGuard`]: the method and the URL text of a request about to be
/// sent, and the refusal text when it may not be.
type Check = dyn Fn(&str, &str) -> Result<(), String> + Send + Sync;

/// Decides whether a request may be sent, before anything reaches the network.
///
/// A refused `pm.sendRequest` reaches the script as its callback's `error`, with the refusal
/// as the text; a refused main request is [`ExecuteError::HostRefused`](crate::ExecuteError).
#[derive(Clone)]
pub struct HostGuard(Arc<Check>);

impl HostGuard {
    /// A guard running `check` with the method and the URL text of each request.
    pub fn new(check: impl Fn(&str, &str) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(check))
    }

    /// Runs the check on a request to `url` with `method`.
    ///
    /// # Errors
    ///
    /// The refusal text.
    pub fn check(&self, method: &str, url: &str) -> Result<(), String> {
        (self.0)(method, url)
    }
}

impl fmt::Debug for HostGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HostGuard")
    }
}

/// Two guards are equal when they share one check, so options compare without calling it.
impl PartialEq for HostGuard {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for HostGuard {}
