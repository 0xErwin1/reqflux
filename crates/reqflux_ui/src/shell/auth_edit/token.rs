//! The OAuth 2.0 token of a request in the Auth pane: its state, read from the engine (never
//! the token), Get new token, which runs the grant on the engine without a send, Cancel
//! while the browser is awaited, and Clear.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Context, Task, Window};
use reqflux_core::model::{OAuth2Grant, TypedAuth};
use reqflux_engine::{
    CancellationToken, ExecuteOptions, HttpError, OAuthBrowser, OAuthFetch, OAuthTokenState,
};

use crate::shell::Shell;
use crate::workspace::OpenedWorkspace;

/// How long a token state read from the engine is shown before it is read again: a send can
/// store or drop a token meanwhile.
const STATE_TTL: Duration = Duration::from_secs(2);

/// How often the pane is redrawn while a fetch runs, so the loopback address shows once the
/// browser opened.
const TICK: Duration = Duration::from_millis(250);

/// The token states read so far, Get new token in flight and the last failure, by request id.
#[derive(Default)]
pub(super) struct Tokens {
    /// Read while rendering, which only has `&Shell`.
    states: RefCell<HashMap<String, (OAuthTokenState, Instant)>>,
    fetches: HashMap<String, Fetch>,
    failures: HashMap<String, String>,
}

/// A Get new token in flight. Dropping it cancels the fetch.
struct Fetch {
    grant: OAuth2Grant,
    /// The engine's fetch, once prepared: what cancels it and the browser it opens.
    started: Option<(CancellationToken, OAuthBrowser)>,
    _wait: Task<()>,
    _tick: Task<()>,
}

impl Drop for Fetch {
    fn drop(&mut self) {
        if let Some((cancel, _)) = &self.started {
            cancel.cancel();
        }
    }
}

/// Cancels the engine's fetch when it is dropped before the shell took it: Cancel came
/// while it was being prepared.
struct Unclaimed(Option<OAuthFetch>);

impl Drop for Unclaimed {
    fn drop(&mut self) {
        if let Some(fetch) = &self.0 {
            fetch.cancel.cancel();
        }
    }
}

/// What the Token row shows while Get new token runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenFetching {
    pub(crate) grant: OAuth2Grant,
    /// The loopback redirect, once the authorization code grant opened the browser.
    pub(crate) redirect_uri: Option<String>,
}

impl Shell {
    /// The draft of the request `id`, which the token belongs to in place of the file.
    fn token_draft(&self, id: &str) -> Option<reqflux_core::model::Request> {
        self.drafts.get(id).map(|draft| draft.request.clone())
    }

    /// The state of the OAuth 2.0 token of the request `id`, read from the engine at most
    /// every [`STATE_TTL`]; `None` without a workspace or when the engine cannot tell.
    pub(in crate::shell) fn oauth_token_state(&self, id: &str) -> Option<OAuthTokenState> {
        let tokens = &self.auth_pane.tokens;
        if let Some((state, read)) = tokens.states.borrow().get(id)
            && read.elapsed() < STATE_TTL
        {
            return Some(*state);
        }
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return None;
        };
        let draft = self.token_draft(id);
        match engine.oauth_token_state(id, draft.as_ref()) {
            Ok(state) => {
                tokens
                    .states
                    .borrow_mut()
                    .insert(id.to_owned(), (state, Instant::now()));
                Some(state)
            }
            Err(error) => {
                log::warn!("the OAuth 2.0 token state of {id} was not read: {error}");
                None
            }
        }
    }

    /// Get new token in flight for the request `id`.
    pub(in crate::shell) fn token_fetching(&self, id: &str) -> Option<TokenFetching> {
        self.auth_pane
            .tokens
            .fetches
            .get(id)
            .map(|fetch| TokenFetching {
                grant: fetch.grant,
                redirect_uri: fetch
                    .started
                    .as_ref()
                    .and_then(|(_, browser)| browser.redirect_uri()),
            })
    }

    /// Why the last Get new token or Clear of the request `id` failed.
    pub(in crate::shell) fn token_failure(&self, id: &str) -> Option<&str> {
        self.auth_pane.tokens.failures.get(id).map(String::as_str)
    }

    fn forget_token_state(&mut self, id: &str) {
        self.auth_pane.tokens.states.borrow_mut().remove(id);
    }

    /// Get new token for the request `id`: prepares its auth off the UI thread with the
    /// active environment and runs the grant on the engine, without a send. An open field
    /// commits first; one fetch at a time per request.
    pub(in crate::shell) fn get_new_token(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self.auth_pane.tokens.fetches.contains_key(id) {
            return;
        }
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let engine = Arc::clone(engine);
        let grant = match self.displayed_auth(id).and_then(|auth| auth.typed()) {
            Some(Ok(TypedAuth::OAuth2(oauth))) => oauth.grant_type,
            _ => return,
        };
        let options = ExecuteOptions {
            environment: self.active_environment.clone(),
            current: self.current.clone(),
            ..ExecuteOptions::default()
        };
        let draft = self.token_draft(id);
        let selector = id.to_owned();
        let prepare = cx.background_executor().spawn(async move {
            engine
                .fetch_oauth_token(&selector, draft.as_ref(), &options)
                .map(|fetch| Unclaimed(Some(fetch)))
        });
        let key = id.to_owned();
        let wait = cx.spawn(async move |this, cx| {
            let outcome = match prepare.await {
                Ok(mut unclaimed) => {
                    let Some(OAuthFetch {
                        cancel,
                        handle,
                        browser,
                    }) = unclaimed.0.take()
                    else {
                        return;
                    };
                    // From here the shell's fetch cancels the engine's when it is dropped.
                    let claimed = this.update(cx, |shell, _| {
                        match shell.auth_pane.tokens.fetches.get_mut(&key) {
                            Some(fetch) => {
                                fetch.started = Some((cancel.clone(), browser));
                                true
                            }
                            None => false,
                        }
                    });
                    if !matches!(claimed, Ok(true)) {
                        cancel.cancel();
                        return;
                    }
                    match handle.await {
                        Ok(Err(HttpError::Cancelled)) => Err(None),
                        Ok(outcome) => outcome.map_err(|error| Some(error.to_string())),
                        Err(error) => Err(Some(format!("the token task failed: {error}"))),
                    }
                }
                Err(error) => Err(Some(error.to_string())),
            };
            let finished = this.update(cx, |shell, cx| shell.finish_token_fetch(&key, outcome, cx));
            if let Err(error) = finished {
                log::debug!("Get new token of {key} ended after its shell: {error}");
            }
        });
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        self.auth_pane.tokens.failures.remove(id);
        self.auth_pane.tokens.fetches.insert(
            id.to_owned(),
            Fetch {
                grant,
                started: None,
                _wait: wait,
                _tick: tick,
            },
        );
        cx.notify();
    }

    /// Ends the fetch of the request `id` with `outcome`, the new token's state or why it
    /// failed; a cancelled one (`Err(None)`) shows nothing.
    fn finish_token_fetch(
        &mut self,
        id: &str,
        outcome: Result<OAuthTokenState, Option<String>>,
        cx: &mut Context<Self>,
    ) {
        // Dropping the fetch here drops this very task's handle: it only cancels a fetch
        // that has already ended.
        self.auth_pane.tokens.fetches.remove(id);
        self.forget_token_state(id);
        match outcome {
            Ok(state) => {
                self.auth_pane
                    .tokens
                    .states
                    .borrow_mut()
                    .insert(id.to_owned(), (state, Instant::now()));
            }
            Err(None) => {}
            Err(Some(error)) => {
                self.auth_pane.tokens.failures.insert(id.to_owned(), error);
            }
        }
        cx.notify();
    }

    /// Cancel: stops Get new token of the request `id`; the token kept before stays.
    pub(in crate::shell) fn cancel_token_fetch(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.auth_pane.tokens.fetches.remove(id).is_some() {
            cx.notify();
        }
    }

    /// Open browser again: opens the authorization URL of the fetch of the request `id`.
    pub(in crate::shell) fn reopen_token_browser(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(browser) = self
            .auth_pane
            .tokens
            .fetches
            .get(id)
            .and_then(|fetch| fetch.started.as_ref())
            .map(|(_, browser)| browser.clone())
        else {
            return;
        };
        if let Err(error) = browser.reopen() {
            self.auth_pane.tokens.failures.insert(id.to_owned(), error);
            cx.notify();
        }
    }

    /// Clear: deletes the token kept for the request `id`.
    pub(in crate::shell) fn clear_token(&mut self, id: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let draft = self.token_draft(id);
        let cleared = engine.forget_oauth_token_of(id, draft.as_ref());
        self.forget_token_state(id);
        match cleared {
            Ok(()) => {
                self.auth_pane.tokens.failures.remove(id);
            }
            Err(error) => {
                self.auth_pane
                    .tokens
                    .failures
                    .insert(id.to_owned(), error.to_string());
            }
        }
        cx.notify();
    }
}
