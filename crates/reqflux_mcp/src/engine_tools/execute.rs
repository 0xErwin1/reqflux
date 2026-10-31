//! `request_execute` (S4): sends a request through the engine, scripts included, and
//! returns the response with this send's secrets masked (S7). It reads no current values,
//! records no history and persists no script writes.
//!
//! The policy runs in steps (user-config-mcp-policy S6, S7): the class and the environment
//! before the workspace is read and again after it is reloaded, then the host of the
//! prepared request before it is sent. Every `pm.sendRequest` of the request's scripts
//! passes the same hosts check through the engine's host guard: a refused one sends nothing
//! and its callback receives the refusal. One audit line records the decision, with the
//! method and the masked URL once the request is prepared; a pre-request script's refused
//! send refuses the call, audited with that request's method and masked URL; a test
//! script's refused send, which runs after the allowed line, adds one refused line of its
//! own. A redirect of any of these requests passes the same check before it is followed: a
//! refused one sends nothing to its target, fails the request it belongs to and adds one
//! refused line naming the redirect. A line that cannot be written refuses the send.
//!
//! An execute class of `ask` is asked in the window once the environment passed, with the
//! method and masked URL the request prepares to without running its scripts; nothing runs
//! before the window allows it (mcp-approvals S2).

use reqflux_engine::audit::Decision;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use reqflux_engine::config::Rule;
use reqflux_engine::policy::{Policy, Refusal as PolicyRefusal};
use reqflux_engine::{Engine, ExecuteOptions, Executed, HostGuard, Masker};
use serde_json::{Value, json};

use super::policy::{self, Scope};
use super::{EngineState, ExecuteArgs, cut, encode, parse_id};
use crate::tools::ToolError;

const TOOL: &str = "request_execute";

/// What a refusal of a script's request starts with, in the tool error and the audit line.
const SCRIPT_REFUSED: &str = "a script's pm.sendRequest was refused: ";

/// What the host guard refused during one call: the method, the URL, the refusal and
/// whether the URL was a redirect target, in order. The guard runs on the script thread and
/// on the transport, so the list is shared behind a mutex.
#[derive(Debug, Clone, Default)]
struct Refused(Arc<Mutex<Vec<Refusal>>>);

#[derive(Debug)]
struct Refusal {
    method: String,
    url: String,
    reason: String,
    redirect: bool,
}

impl Refusal {
    /// The refusal text for the tool error and the audit line, the URL masked.
    fn text(&self, masker: &Masker) -> String {
        if self.redirect {
            masker.mask(&format!(
                "a redirect to {} was refused: {}",
                self.url, self.reason
            ))
        } else {
            masker.mask(&format!("{SCRIPT_REFUSED}{}", self.reason))
        }
    }
}

impl Refused {
    /// A guard checking each request and redirect target against the policy's hosts and
    /// recording a refusal. The main request is checked by the caller, with its own audit
    /// line; its redirects are checked here.
    fn guard(&self, policy: Policy) -> HostGuard {
        let refused = self.clone();
        HostGuard::with_redirects(move |method, url, redirect| {
            policy.check_host(TOOL, url).map_err(|refusal| {
                let reason = refusal.to_string();
                refused.lock().push(Refusal {
                    method: method.to_owned(),
                    url: url.to_owned(),
                    reason: reason.clone(),
                    redirect,
                });
                reason
            })
        })
    }

    /// The refusals recorded so far, leaving none.
    fn take(&self) -> Vec<Refusal> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Refusal>> {
        // A poisoned list still holds every refusal pushed before the panic.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `request_execute`: sends the request `id` with `environment` and `vars`.
///
/// # Errors
///
/// A bad or unknown id or environment, no open workspace, or the transport failed; a
/// transport error is masked with the send's masker.
pub fn request_execute(state: &mut EngineState, args: ExecuteArgs) -> Result<String, ToolError> {
    let call = state.call(TOOL, Scope::Open, &[&args.id]);
    let paths = state.paths.clone();
    // The class and the environment; a class whose rule is `ask` comes back as the refusal
    // the window is asked to lift, once the environment passed too.
    let checked = |policy: &Policy| -> Result<Option<PolicyRefusal>, PolicyRefusal> {
        let ask = match policy.check_class(TOOL, call.class) {
            Ok(()) => None,
            Err(refusal) if policy.rule(call.class) == Rule::Ask => Some(refusal),
            Err(refusal) => return Err(refusal),
        };
        policy.check_environment(TOOL, args.environment.as_deref())?;
        Ok(ask)
    };
    if let Err(refusal) = checked(&policy::evaluate(&paths, state.engine.as_ref())) {
        return Err(call.refuse(&paths, &refusal.to_string(), None));
    }
    let id = parse_id(&args.id);
    let reloaded = id.and_then(|id| state.reloaded().map(|_| id));
    let id = match reloaded {
        Ok(id) => id,
        Err(error) => {
            call.audit(&paths, Decision::Allowed, None, None)?;
            return Err(error);
        }
    };
    // The workspace was reloaded: check again with its manifest as it is now, before the
    // pre-request script runs, then guard every request the execution sends with its hosts.
    let policy = policy::evaluate(&paths, state.engine.as_ref());
    let ask = match checked(&policy) {
        Ok(ask) => ask,
        Err(refusal) => return Err(call.refuse(&paths, &refusal.to_string(), None)),
    };
    // `ask`: the window sees the method and masked URL the request prepares to, without its
    // scripts, so nothing is sent before it answers (mcp-approvals S2).
    let approval = match ask {
        None => None,
        Some(refusal) => {
            let preview = state
                .engine
                .as_ref()
                .filter(|_| state.approvals.enabled())
                .and_then(|engine| preview(engine, &id.to_string(), &args));
            let sent = preview
                .as_ref()
                .map(|(method, url)| (method.as_str(), url.as_str()));
            match state.approve(&call, &refusal, sent, args.environment.as_deref()) {
                Ok(reason) => Some(reason),
                Err(text) => return Err(call.refuse(&paths, &text, sent)),
            }
        }
    };
    let engine = state.engine()?;
    let refused = Refused::default();
    let options = ExecuteOptions {
        environment: args.environment.clone(),
        vars: args.vars.clone().unwrap_or_default().into_iter().collect(),
        host_guard: Some(refused.guard(policy.clone())),
        // The server never reads current values: they belong to the window's app state.
        ..ExecuteOptions::default()
    };
    let pending = match engine.prepare_execution(&id.to_string(), &options) {
        Ok(pending) => pending,
        Err(error) => {
            call.audit(&paths, Decision::Allowed, None, None)?;
            return Err(ToolError::Engine(error.to_string()));
        }
    };
    let masker = pending.masker();
    // A pre-request script sent to a refused host, or was redirected to one: nothing was
    // sent there, and the call is refused with that request in the audit line.
    if let Some(refusal) = refused.take().into_iter().next() {
        let url = masker.mask(&refusal.url);
        let reason = refusal.text(&masker);
        return Err(call.refuse(
            &paths,
            &reason,
            Some((refusal.method.as_str(), url.as_str())),
        ));
    }
    let method = pending.request().method.clone();
    let url = masker.mask(&pending.request().url);
    let sent = Some((method.as_str(), url.as_str()));
    if let Err(refusal) = policy.check_host(TOOL, &pending.request().url) {
        return Err(call.refuse(&paths, &masker.mask(&refusal.to_string()), sent));
    }
    call.audit(&paths, Decision::Allowed, approval, sent)?;
    let execution = engine.start(pending);
    let executed = engine
        .wait(execution)
        .map_err(|error| ToolError::Engine(masker.mask(&error.to_string())));
    // A test script's send and a redirect of the main request run after the allowed line
    // was written: each refused one gets its own refused line. A refused test-script send
    // still returns the response, the refusal in its console; a refused redirect fails it.
    for refusal in refused.take() {
        let url = masker.mask(&refusal.url);
        let reason = refusal.text(&masker);
        call.audit(
            &paths,
            Decision::Refused,
            Some(&reason),
            Some((refusal.method.as_str(), url.as_str())),
        )?;
    }
    let executed = executed?;
    encode(&output(&executed, &masker))
}

/// The method and masked URL the request `id` prepares to with `args`, its scripts not
/// run; `None` when it does not prepare.
fn preview(engine: &Engine, id: &str, args: &ExecuteArgs) -> Option<(String, String)> {
    let options = ExecuteOptions {
        environment: args.environment.clone(),
        vars: args.vars.clone().unwrap_or_default().into_iter().collect(),
        ..ExecuteOptions::default()
    };
    let prepared = engine.prepare(id, &options).ok()?;
    let url = prepared.masker().mask(&prepared.request.url);
    Some((prepared.request.method.clone(), url))
}

fn output(executed: &Executed, masker: &Masker) -> Value {
    let response = &executed.response;
    let headers: Vec<(String, String)> = response
        .headers
        .iter()
        .map(|(name, value)| masker.header_for_agent(name, value))
        .collect();
    let (body, body_bytes, truncated) = match std::str::from_utf8(&response.body) {
        Ok(text) => {
            // Mask before cutting, so a cut never splits a secret into an unmasked prefix.
            let (text, truncated) = cut(masker.mask(text));
            (Some(text), None, truncated)
        }
        Err(_) => (None, Some(response.size()), false),
    };
    let tests: Vec<Value> = executed
        .scripts
        .tests
        .iter()
        .map(|test| {
            json!({
                "name": masker.mask(&test.name),
                "passed": test.passed,
                "message": test.message.as_deref().map(|message| masker.mask(message)),
            })
        })
        .collect();
    json!({
        "method": executed.sent.method,
        "url": executed.sent.url,
        "status": response.status,
        "reason": response.reason,
        "headers": headers,
        "elapsed_ms": u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        "size": response.size(),
        "body": body,
        "body_bytes": body_bytes,
        "truncated": truncated,
        "unresolved": executed.unresolved,
        "secret_problem": executed.secret_problem,
        "tests": tests,
    })
}
