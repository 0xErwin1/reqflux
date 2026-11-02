//! Host functions behind the `pm` shim and the context JSON the shim reads.
//!
//! Every host function locks the shared [`State`], touches it briefly and returns; none of
//! them runs JavaScript while holding the lock, so the script can call back into the host.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rquickjs::function::Func;
use rquickjs::{Ctx, String as JsString};

use crate::sandbox::State;
use crate::{ConsoleLevel, NextRequest, ScriptContext, ScriptRequest, SendRequest, VariableScope};

type CtxResult<T> = rquickjs::Result<T>;
type SharedState = Arc<Mutex<State>>;

/// Stops the run from inside a host call: the interrupt handler cannot bound a script whose
/// wall time sits in host work, so each call checks the deadline and the collected size.
fn abort(ctx: &Ctx<'_>, kind: crate::ScriptErrorKind) -> rquickjs::Error {
    let text = match kind {
        crate::ScriptErrorKind::Timeout => "script stopped by its time limit",
        crate::ScriptErrorKind::MemoryLimit => "script stopped by its memory limit",
        _ => "script stopped",
    };
    match JsString::from_str(ctx.clone(), text) {
        Ok(value) => ctx.throw(value.into_value()),
        Err(error) => error,
    }
}

pub(super) fn install(
    ctx: &Ctx<'_>,
    state: &SharedState,
    send_request: Option<SendRequest>,
) -> CtxResult<()> {
    let globals = ctx.globals();
    globals.set(
        "__reqfluxModule",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>, name: String| -> CtxResult<Option<String>> {
                let source = crate::modules::source(&name).map(str::to_owned);
                if let Some(source) = &source
                    && let Some(kind) = lock(&state).charge(source.len())
                {
                    return Err(abort(&ctx, kind));
                }
                Ok(source)
            }
        }),
    )?;
    globals.set(
        "__reqfluxReach",
        Func::from({
            let state = Arc::clone(state);
            move || {
                lock(&state).mark_reached();
            }
        }),
    )?;
    globals.set(
        "__reqfluxVarGet",
        Func::from({
            let state = Arc::clone(state);
            move |scope: String, key: String| -> CtxResult<Option<String>> {
                Ok(lock(&state).var_get(parse_scope(&scope), &key))
            }
        }),
    )?;
    globals.set(
        "__reqfluxVarSet",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>, scope: String, key: String, value: String| -> CtxResult<()> {
                let charged = key.len().saturating_add(value.len());
                if let Some(kind) = lock(&state).charge(charged) {
                    return Err(abort(&ctx, kind));
                }
                lock(&state).var_set(parse_scope(&scope), key, value);
                Ok(())
            }
        }),
    )?;
    globals.set(
        "__reqfluxVarUnset",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>, scope: String, key: String| -> CtxResult<()> {
                if let Some(kind) = lock(&state).charge(key.len()) {
                    return Err(abort(&ctx, kind));
                }
                lock(&state).var_unset(parse_scope(&scope), key);
                Ok(())
            }
        }),
    )?;
    globals.set(
        "__reqfluxTest",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>,
                  name: String,
                  passed: bool,
                  message: Option<String>|
                  -> CtxResult<()> {
                let charged = name
                    .len()
                    .saturating_add(message.as_deref().map_or(0, str::len));
                if let Some(kind) = lock(&state).charge(charged) {
                    return Err(abort(&ctx, kind));
                }
                lock(&state).record_test(name, passed, message);
                Ok(())
            }
        }),
    )?;
    globals.set(
        "__reqfluxConsole",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>, level: String, message: String| -> CtxResult<()> {
                if let Some(kind) = lock(&state).charge(message.len()) {
                    return Err(abort(&ctx, kind));
                }
                lock(&state).record_console(parse_level(&level), message);
                Ok(())
            }
        }),
    )?;
    globals.set(
        "__reqfluxNext",
        Func::from({
            let state = Arc::clone(state);
            // The shim has already refused anything but a string or null.
            move |ctx: Ctx<'_>, target: Option<String>| -> CtxResult<()> {
                let charged = target.as_deref().map_or(0, str::len);
                if let Some(kind) = lock(&state).charge(charged) {
                    return Err(abort(&ctx, kind));
                }
                lock(&state)
                    .set_next_request(target.map_or(NextRequest::Stop, NextRequest::Request));
                Ok(())
            }
        }),
    )?;
    globals.set(
        "__reqfluxSend",
        Func::from({
            let state = Arc::clone(state);
            move |ctx: Ctx<'_>, spec: String| -> CtxResult<String> {
                send_request_call(&ctx, &state, send_request.as_ref(), &spec)
            }
        }),
    )?;
    Ok(())
}

/// Handles one `__reqfluxSend` call (S13): parses the canonical request spec the shim built,
/// hands it to the host handler without holding the state lock, and answers with a JSON
/// document the shim turns into the callback's `(error, response)`. Every step charges the
/// collected state and checks the deadline, so a loop of sends is bounded like any other
/// host call and a host slower than the deadline ends the run as a Timeout.
fn send_request_call(
    ctx: &Ctx<'_>,
    state: &SharedState,
    send_request: Option<&SendRequest>,
    spec: &str,
) -> CtxResult<String> {
    let request = match parse_spec(spec) {
        Ok(request) => request,
        Err(message) => return Ok(send_failure(&message)),
    };
    if let Some(kind) = lock(state).charge(spec.len()) {
        return Err(abort(ctx, kind));
    }
    let Some(send_request) = send_request else {
        return Ok(send_failure("pm.sendRequest has no host handler"));
    };
    let answer = (send_request.0)(request);
    let answer = match answer {
        Ok(response) => serde_json::json!({
            "ok": true,
            "response": {
                "code": response.code,
                "status": response.status,
                "headers": response.headers,
                "text": response.body,
            },
        }),
        Err(message) => serde_json::json!({ "ok": false, "error": message }),
    };
    let answer = serde_json::to_string(&answer)
        .unwrap_or_else(|_| send_failure("the host answer was not serializable"));
    if let Some(kind) = lock(state).charge(answer.len()) {
        return Err(abort(ctx, kind));
    }
    Ok(answer)
}

/// A failed send, which the shim delivers as the callback's `error`.
fn send_failure(message: &str) -> String {
    serde_json::json!({ "ok": false, "error": message }).to_string()
}

/// Parses the canonical request spec the shim builds: `{ method, url, headers, body }`.
/// The host stays defensive: a spec without a URL fails the send instead of the run.
fn parse_spec(spec: &str) -> Result<ScriptRequest, String> {
    let value: serde_json::Value =
        serde_json::from_str(spec).map_err(|error| format!("pm.sendRequest spec: {error}"))?;
    let url = value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "pm.sendRequest needs a url".to_owned())?;
    let method = value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .filter(|method| !method.is_empty())
        .unwrap_or("GET");
    let mut headers = Vec::new();
    if let Some(rows) = value.get("headers").and_then(serde_json::Value::as_array) {
        for row in rows {
            let Some(pair) = row.as_array() else {
                continue;
            };
            if let (Some(key), Some(value)) = (
                pair.first().and_then(serde_json::Value::as_str),
                pair.get(1).and_then(serde_json::Value::as_str),
            ) {
                headers.push((key.to_owned(), value.to_owned()));
            }
        }
    }
    let body = value
        .get("body")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(ScriptRequest {
        method: method.to_owned(),
        url: url.to_owned(),
        headers,
        body,
    })
}

fn lock(state: &SharedState) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn parse_scope(scope: &str) -> VariableScope {
    match scope {
        "environment" => VariableScope::Environment,
        "collectionVariables" => VariableScope::CollectionVariables,
        "globals" => VariableScope::Globals,
        _ => VariableScope::Variables,
    }
}

fn parse_level(level: &str) -> ConsoleLevel {
    match level {
        "info" => ConsoleLevel::Info,
        "warn" => ConsoleLevel::Warn,
        "error" => ConsoleLevel::Error,
        _ => ConsoleLevel::Log,
    }
}

/// Builds the `globalThis.__reqfluxInit = JSON.parse("...")` source line the shim reads.
pub(super) fn init_source(context: &ScriptContext) -> String {
    let response = context.response.as_ref().map(|response| {
        let mut value = serde_json::json!({
            "code": response.code,
            "status": response.status,
            "responseTime": response.response_time_ms,
            "responseSize": response.response_size,
            "headers": response.headers,
            "_text": response.body_text,
        });
        if let Some(parsed) = &response.parsed_json
            && let Some(object) = value.as_object_mut()
        {
            object.insert("_parsed".into(), parsed.clone());
        }
        value
    });
    let init = serde_json::json!({
        "stage": context.stage.as_str(),
        "request": {
            "method": context.request.method,
            "url": context.request.url,
            "headers": context.request.headers,
            "body": context.request.body,
        },
        "response": response,
    });
    let json = serde_json::to_string(&init).unwrap_or_else(|_| "{}".into());
    format!(
        "globalThis.__reqfluxInit = JSON.parse(\"{}\");",
        escape_js(&json)
    )
}

/// Escapes a JSON document so it is a valid double-quoted JavaScript string literal. The
/// backslash goes first so the JSON escape sequences survive as literal text for `JSON.parse`.
fn escape_js(json: &str) -> String {
    json.replace('\\', "\\\\").replace('"', "\\\"")
}
