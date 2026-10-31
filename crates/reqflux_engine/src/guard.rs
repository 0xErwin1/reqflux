//! The outbound host guard of an execution (user-config-mcp-policy S3, S6): a check every
//! request an execution sends passes first, the main request and every `pm.sendRequest` of
//! its pre-request and test scripts. The window and `reqflux run` pass none; the MCP server
//! passes its host policy. The guard also checks every redirect target of those requests
//! before it is followed.

use std::fmt;
use std::sync::Arc;

use reqflux_http::RedirectCheck;

/// The check behind a [`HostGuard`]: the method and the URL text of a request about to be
/// sent, whether it is a redirect target, and the refusal text when it may not be.
type Check = dyn Fn(&str, &str, bool) -> Result<(), String> + Send + Sync;

/// Decides whether a request may be sent, before anything reaches the network.
///
/// A refused `pm.sendRequest` reaches the script as its callback's `error`, with the refusal
/// as the text; a refused main request is [`ExecuteError::HostRefused`](crate::ExecuteError).
#[derive(Clone)]
pub struct HostGuard(Arc<Check>);

impl HostGuard {
    /// A guard running `check` with the method and the URL text of each request.
    pub fn new(check: impl Fn(&str, &str) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(move |method, url, _redirect| check(method, url)))
    }

    /// A guard running `check` with the method, the URL text and whether the URL is a
    /// redirect target, so a caller can tell a refused redirect from a refused request.
    pub fn with_redirects(
        check: impl Fn(&str, &str, bool) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(check))
    }

    /// Runs the check on a request to `url` with `method`.
    ///
    /// # Errors
    ///
    /// The refusal text.
    pub fn check(&self, method: &str, url: &str) -> Result<(), String> {
        (self.0)(method, url, false)
    }

    /// The transport hook checking each redirect target of a request sent with `method`.
    pub(crate) fn redirect_check(&self, method: &str) -> RedirectCheck {
        let check = Arc::clone(&self.0);
        let method = method.to_owned();
        Arc::new(move |url| check(&method, url.as_str(), true))
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
