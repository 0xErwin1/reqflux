//! `cookie_list`, `cookie_delete` and `cookie_clear` (v1-features S5): the open workspace's
//! cookie jar, and what an opened engine takes from the user: the `[network]` settings of
//! the config file and the jar kept in the app state.
//!
//! A cookie value is a credential an agent has no reason to hold, so `cookie_list` returns
//! every cookie with its value masked, as `request_execute` masks a `Set-Cookie` value.

use reqflux_engine::config;
use reqflux_engine::{Engine, StoredCookie};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::policy::Scope;
use super::{EngineState, encode};
use crate::tools::ToolError;

/// Arguments of `cookie_delete`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CookieArgs {
    /// The domain the cookie is stored under, as `cookie_list` returns it.
    pub domain: String,
    /// The cookie path, as `cookie_list` returns it.
    pub path: String,
    /// The cookie name.
    pub name: String,
}

impl EngineState {
    /// Applies the user's `[network]` settings to `engine` and keeps its cookie jar in the
    /// app state this state reads history from, when it names one. A setting or a jar that
    /// cannot be applied is logged; the workspace still opens.
    pub(super) fn configure(&self, engine: &Engine) {
        match config::load(&self.paths.config) {
            Ok(user) => {
                for warning in engine.apply_network(&user.network) {
                    log::warn!(target: "reqflux_mcp", "{warning}");
                }
            }
            Err(error) => {
                log::warn!(target: "reqflux_mcp", "network settings not applied: {error}");
            }
        }
        if let Some(path) = &self.state_path
            && let Err(error) = engine.persist_cookies(path)
        {
            log::warn!(target: "reqflux_mcp", "the cookie jar is kept in memory only: {error}");
        }
    }
}

fn cookie_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::Engine(format!("the cookie jar failed: {error}"))
}

fn describe(cookie: &StoredCookie) -> Value {
    let cookie = cookie.masked();
    json!({
        "domain": cookie.domain,
        "host_only": cookie.host_only,
        "path": cookie.path,
        "name": cookie.name,
        "value": cookie.value,
        "expires": cookie.expires,
        "secure": cookie.secure,
        "http_only": cookie.http_only,
        "same_site": cookie.same_site,
    })
}

/// `cookie_list`: the cookies of the open workspace's jar, values masked, by domain, path
/// and name.
///
/// # Errors
///
/// No workspace is open, or the policy refuses reading it.
pub fn cookie_list(state: &mut EngineState) -> Result<String, ToolError> {
    state.authorize("cookie_list", Scope::Open, &[])?;
    let cookies: Vec<Value> = state.engine()?.cookies().iter().map(describe).collect();
    encode(&json!({ "cookies": cookies }))
}

/// `cookie_delete`: deletes one cookie of the open workspace's jar; `deleted` says whether
/// there was one.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, or the app state failed.
pub fn cookie_delete(state: &mut EngineState, args: CookieArgs) -> Result<String, ToolError> {
    state.authorize("cookie_delete", Scope::Open, &[args.domain.as_str()])?;
    let deleted = state
        .engine()?
        .delete_cookie(&args.domain, &args.path, &args.name)
        .map_err(cookie_error)?;
    encode(&json!({ "deleted": deleted }))
}

/// `cookie_clear`: deletes every cookie of the open workspace's jar; `cleared` is how many
/// there were.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, or the app state failed.
pub fn cookie_clear(state: &mut EngineState) -> Result<String, ToolError> {
    state.authorize("cookie_clear", Scope::Open, &[])?;
    let cleared = state.engine()?.clear_cookies().map_err(cookie_error)?;
    encode(&json!({ "cleared": cleared }))
}
