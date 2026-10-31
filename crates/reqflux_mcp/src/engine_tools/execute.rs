//! `request_execute` (S4): sends a request through the engine, scripts included, and
//! returns the response with this send's secrets masked (S7). It reads no current values,
//! records no history and persists no script writes.
//!
//! The policy runs in two steps (user-config-mcp-policy S6, S7): the class and the
//! environment before the workspace is read, then the host of the prepared request before it
//! is sent. One audit line records the decision, with the method and the masked URL once the
//! request is prepared; a line that cannot be written refuses the send.

use reqflux_engine::audit::Decision;
use reqflux_engine::{ExecuteOptions, Executed, Masker};
use serde_json::{Value, json};

use super::policy::{self, Scope};
use super::{EngineState, ExecuteArgs, cut, encode, parse_id};
use crate::tools::ToolError;

const TOOL: &str = "request_execute";

/// `request_execute`: sends the request `id` with `environment` and `vars`.
///
/// # Errors
///
/// A bad or unknown id or environment, no open workspace, or the transport failed; a
/// transport error is masked with the send's masker.
pub fn request_execute(state: &mut EngineState, args: ExecuteArgs) -> Result<String, ToolError> {
    let call = state.call(TOOL, Scope::Open, &[&args.id]);
    let paths = state.paths.clone();
    let checked = |policy: &reqflux_engine::policy::Policy| {
        policy
            .check_class(TOOL, call.class)
            .and_then(|()| policy.check_environment(TOOL, args.environment.as_deref()))
    };
    if let Err(refusal) = checked(&policy::evaluate(&paths, state.engine.as_ref())) {
        return Err(call.refuse(&paths, &refusal.to_string(), None));
    }
    let prepared = parse_id(&args.id).and_then(|id| {
        let engine = state.reloaded()?;
        let options = ExecuteOptions {
            environment: args.environment.clone(),
            vars: args.vars.clone().unwrap_or_default().into_iter().collect(),
            // The server never reads current values: they belong to the window's app state.
            ..ExecuteOptions::default()
        };
        engine
            .prepare_execution(&id.to_string(), &options)
            .map_err(|error| ToolError::Engine(error.to_string()))
    });
    let pending = match prepared {
        Ok(pending) => pending,
        Err(error) => {
            call.audit(&paths, Decision::Allowed, None, None)?;
            return Err(error);
        }
    };
    let masker = pending.masker();
    let method = pending.request().method.clone();
    let url = masker.mask(&pending.request().url);
    let sent = Some((method.as_str(), url.as_str()));
    // The workspace was reloaded: check again with its manifest as it is now, then the host.
    let engine = state.engine()?;
    let policy = policy::evaluate(&paths, Some(engine));
    if let Err(refusal) =
        checked(&policy).and_then(|()| policy.check_host(TOOL, &pending.request().url))
    {
        return Err(call.refuse(&paths, &masker.mask(&refusal.to_string()), sent));
    }
    call.audit(&paths, Decision::Allowed, None, sent)?;
    let execution = engine.start(pending);
    let executed = engine
        .wait(execution)
        .map_err(|error| ToolError::Engine(masker.mask(&error.to_string())))?;
    encode(&output(&executed, &masker))
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
