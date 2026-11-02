//! What a front end shows and does with the OAuth 2.0 token of a request (v1-features S3):
//! the state of the kept token, never the token, and Get new token, which runs the
//! configured grant without a send and can be cancelled while it waits for the browser.

use std::sync::{Arc, Mutex, PoisonError};

use reqflux_core::model::{Request, TypedAuth};
use reqflux_http::HttpError;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::oauth::{self, OAuthToken};
use super::{AuthContext, Browser, PendingAuth};
use crate::Engine;
use crate::error::ExecuteError;
use crate::prepare::ExecuteOptions;

/// The state of the OAuth 2.0 token kept for a request's effective auth. It carries no
/// token, only whether one is kept and when it expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthTokenState {
    /// No token is kept: the next send, or Get new token, obtains one.
    None,
    /// A token the next send uses.
    Valid {
        /// When it expires, in seconds since the Unix epoch; `None` when the server did
        /// not say.
        expires_at: Option<u64>,
    },
    /// The kept token has expired, or a `401` refused it.
    Expired {
        /// When it expired, in seconds since the Unix epoch; `None` when that is not
        /// known, as for a token a `401` refused.
        expires_at: Option<u64>,
        /// A refresh token is kept, which the next send or Get new token tries first.
        refreshable: bool,
    },
}

impl OAuthTokenState {
    fn of(token: Option<&OAuthToken>, now: u64) -> Self {
        let Some(token) = token else {
            return Self::None;
        };
        if token.access_token.is_empty() || token.expired(now) {
            Self::Expired {
                expires_at: token.expires_at.filter(|at| *at > 0),
                refreshable: token.refresh_token.is_some(),
            }
        } else {
            Self::Valid {
                expires_at: token.expires_at,
            }
        }
    }
}

/// A Get new token in flight: its task, a token that cancels it and, for the authorization
/// code grant, the browser it opened.
#[derive(Debug)]
pub struct OAuthFetch {
    /// Cancels the fetch; the task then ends with [`HttpError::Cancelled`] and the loopback
    /// redirect stops listening.
    pub cancel: CancellationToken,
    /// The fetch: the state of the new token once it is stored. A failure keeps the token
    /// that was stored before.
    pub handle: JoinHandle<Result<OAuthTokenState, HttpError>>,
    /// The browser the authorization code grant opens.
    pub browser: OAuthBrowser,
}

/// The browser of a Get new token: what the authorization code grant opened, without the
/// authorization URL itself.
#[derive(Debug, Clone)]
pub struct OAuthBrowser(Arc<RecordingBrowser>);

impl OAuthBrowser {
    /// The loopback redirect the authorization code grant listens on, once it opened the
    /// browser; `None` before that and for the other grants.
    #[must_use]
    pub fn redirect_uri(&self) -> Option<String> {
        let url = url::Url::parse(&self.0.opened()?).ok()?;
        url.query_pairs()
            .find(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned())
    }

    /// Opens the authorization URL in the browser again.
    ///
    /// # Errors
    ///
    /// The browser was not opened yet, or did not open; the text says why without the URL.
    pub fn reopen(&self) -> Result<(), String> {
        let url = self
            .0
            .opened()
            .ok_or_else(|| "the browser was not opened yet".to_owned())?;
        self.0.inner.open(&url)
    }
}

/// Opens the authorization URL with `inner` and keeps it, so it can be opened again.
#[derive(Debug)]
struct RecordingBrowser {
    inner: Arc<dyn Browser>,
    url: Mutex<Option<String>>,
}

impl RecordingBrowser {
    fn opened(&self) -> Option<String> {
        self.url
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Browser for RecordingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        *self.url.lock().unwrap_or_else(PoisonError::into_inner) = Some(url.to_owned());
        self.inner.open(url)
    }
}

fn not_oauth() -> ExecuteError {
    ExecuteError::Http(HttpError::Auth(
        "the request's auth is not OAuth 2.0".to_owned(),
    ))
}

impl Engine {
    /// The state of the OAuth 2.0 token kept for the effective auth of the request
    /// `selector`, or of `draft` in its place. An effective auth other than OAuth 2.0 has
    /// no token. A secret store that cannot be read reads as no token, and is logged.
    ///
    /// # Errors
    ///
    /// No request matches `selector`.
    pub fn oauth_token_state(
        &self,
        selector: &str,
        draft: Option<&Request>,
    ) -> Result<OAuthTokenState, ExecuteError> {
        let target = crate::select::find(self.workspace(), selector)?;
        let request = draft.unwrap_or(&target.request.request);
        let (owner, auth) = crate::prepare::effective(request, &target);
        if !matches!(auth.typed(), Some(Ok(TypedAuth::OAuth2(_)))) {
            return Ok(OAuthTokenState::None);
        }
        let key = owner.token_key(
            self.workspace().manifest.id,
            target.collection.collection.id,
        );
        let token = oauth::read(&self.auth_context(Arc::clone(&self.browser)), &key);
        Ok(OAuthTokenState::of(token.as_ref(), oauth::unix_now()))
    }

    /// Get new token: obtains a new OAuth 2.0 token for the effective auth of the request
    /// `selector`, or of `draft` in its place, with its fields resolved under `options`,
    /// and stores it, without sending the request. A kept refresh token is tried first,
    /// then the configured grant. An `Authorization` request header does not stop it.
    ///
    /// # Errors
    ///
    /// The errors of [`prepare`](crate::prepare), and an effective auth that is not OAuth
    /// 2.0.
    pub fn fetch_oauth_token(
        &self,
        selector: &str,
        draft: Option<&Request>,
        options: &ExecuteOptions,
    ) -> Result<OAuthFetch, ExecuteError> {
        let target = crate::select::find(self.workspace(), selector)?;
        let mut request = draft.unwrap_or(&target.request.request).clone();
        // An enabled Authorization header keeps the OAuth 2.0 auth out of the prepared
        // request; the token is still what this asks for.
        for header in &mut request.headers {
            if header.key.eq_ignore_ascii_case("authorization") {
                header.enabled = false;
            }
        }
        let prepared = self.prepare_stored(&target, &request, options)?;
        let Some(PendingAuth::OAuth2 { config, token: key }) = prepared.auth else {
            return Err(not_oauth());
        };
        let request_options = prepared.request.options;
        let browser = Arc::new(RecordingBrowser {
            inner: Arc::clone(&self.browser),
            url: Mutex::default(),
        });
        let context = self.auth_context(Arc::clone(&browser) as Arc<dyn Browser>);
        let cancel = CancellationToken::new();
        let cancelled = cancel.clone();
        let handle = self.runtime.spawn(async move {
            let stored = oauth::read(&context, &key);
            let token = tokio::select! {
                biased;
                () = cancelled.cancelled() => return Err(HttpError::Cancelled),
                token = oauth::acquire(&context, &config, stored, request_options) => token?,
            };
            let state = OAuthTokenState::of(Some(&token), oauth::unix_now());
            oauth::store(&context, &key, token);
            Ok(state)
        });
        Ok(OAuthFetch {
            cancel,
            handle,
            browser: OAuthBrowser(browser),
        })
    }

    /// Clear: [`Engine::forget_oauth_token`] for the effective auth of `draft` in the place
    /// of the request `selector`, so a draft that gave the request its own OAuth 2.0 auth
    /// clears its own token, not its parent's.
    ///
    /// # Errors
    ///
    /// No request matches `selector`, or the secret store failed.
    pub fn forget_oauth_token_of(
        &self,
        selector: &str,
        draft: Option<&Request>,
    ) -> Result<(), ExecuteError> {
        let target = crate::select::find(self.workspace(), selector)?;
        let request = draft.unwrap_or(&target.request.request);
        let (owner, _) = crate::prepare::effective(request, &target);
        let key = owner.token_key(
            self.workspace().manifest.id,
            target.collection.collection.id,
        );
        Ok(self.secrets.delete(&key)?)
    }

    fn auth_context(&self, browser: Arc<dyn Browser>) -> AuthContext {
        AuthContext {
            client: self.client.clone(),
            secrets: Arc::clone(&self.secrets),
            browser,
        }
    }
}
