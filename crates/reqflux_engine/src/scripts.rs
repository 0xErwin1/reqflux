//! The engine side of scripting (S7): the effective scripts, the views they read and the
//! variable writes they leave behind.
//!
//! The engine picks the most specific script that exists — the request's, else the
//! innermost folder's, else the collection's — runs it through [`reqflux_script`] and applies
//! its writes to the execution's scopes for this run only, never to disk.

use std::collections::{BTreeMap, BTreeSet};

use reqflux_core::model::{Body, Request, Scripts};
use reqflux_core::template::{ScopeEntry, ScopeKind, Scopes};
use reqflux_http::{
    HttpClient, PreparedBody, PreparedRequest, RedirectRules, RequestOptions, Response,
};
use reqflux_script::{
    ConsoleEntry, ConsoleLevel, RequestView, ResponseView, ScriptContext, ScriptOutcome,
    ScriptRequest, ScriptResponse, ScriptStage, SendRequest, VariableScope, VariableWrite,
};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::guard::HostGuard;
use crate::select::Target;
use crate::sent::Masker;

/// What a script's `pm.sendRequest` needs from the engine (S13): the same
/// [`HttpClient`] the request path uses and the request's own settings, so a send from a
/// script respects the same timeout, redirect and TLS choices as the real request, and
/// passes the execution's host guard first. A redirect to another origin drops the headers
/// holding a secret of `masker`.
#[derive(Debug, Clone)]
pub(crate) struct Transport {
    client: HttpClient,
    options: RequestOptions,
    guard: Option<HostGuard>,
    masker: Masker,
}

impl Transport {
    pub(crate) fn new(
        client: HttpClient,
        options: RequestOptions,
        guard: Option<HostGuard>,
        masker: Masker,
    ) -> Self {
        Self {
            client,
            options,
            guard,
            masker,
        }
    }

    /// Replaces the secrets a redirect to another origin must not carry.
    pub(crate) fn set_masker(&mut self, masker: Masker) {
        self.masker = masker;
    }

    /// The execution's host guard, if any.
    pub(crate) fn guard(&self) -> Option<&HostGuard> {
        self.guard.as_ref()
    }
}

/// The effective pre-request script of `target` whose own scripts are `own` (the saved
/// sidecars or a draft's): the request's, else the innermost folder's that has one, else
/// the collection's. An empty script of the request counts as absent, as saving it deletes
/// its sidecar.
pub(crate) fn effective_pre_request<'a>(target: &Target<'a>, own: &'a Scripts) -> Option<&'a str> {
    own.pre_request
        .as_deref()
        .filter(|script| !script.is_empty())
        .or_else(|| {
            target
                .folders
                .iter()
                .rev()
                .find_map(|folder| folder.scripts.pre_request.as_deref())
        })
        .or(target.collection.scripts.pre_request.as_deref())
}

/// The effective test script of `target`, resolved like the pre-request one.
pub(crate) fn effective_test<'a>(target: &Target<'a>, own: &'a Scripts) -> Option<&'a str> {
    own.test
        .as_deref()
        .filter(|script| !script.is_empty())
        .or_else(|| {
            target
                .folders
                .iter()
                .rev()
                .find_map(|folder| folder.scripts.test.as_deref())
        })
        .or(target.collection.scripts.test.as_deref())
}

/// The `pm.request` view of `request`: the stored document, before variable resolution.
pub(crate) fn request_view(request: &Request) -> RequestView {
    let headers = request
        .headers
        .iter()
        .filter(|row| row.enabled)
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect();
    let body = match &request.body {
        Body::Raw { text, .. } => Some(text.clone()),
        _ => None,
    };
    RequestView {
        method: request.method.as_str().to_owned(),
        url: request.url.clone(),
        headers,
        body,
    }
}

/// The `pm.response` view of the response. The body decodes lossily; the JSON parse is best
/// effort, because the shim falls back to parsing the text.
pub(crate) fn response_view(response: &Response) -> ResponseView {
    let headers = response
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    ResponseView {
        code: response.status,
        status: response.reason.clone().unwrap_or_default(),
        response_time_ms: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        response_size: u64::try_from(response.size()).unwrap_or(u64::MAX),
        headers,
        body_text: String::from_utf8_lossy(&response.body).into_owned(),
        parsed_json: serde_json::from_slice(&response.body).unwrap_or(None),
    }
}

/// The merged variable view scripts read: one entry per name at the value of its
/// highest-precedence scope.
pub(crate) fn merged(scopes: &Scopes) -> BTreeMap<String, String> {
    let mut names = BTreeSet::new();
    for kind in ScopeKind::ALL {
        for entry in &scopes.scope(kind).entries {
            names.insert(entry.key.clone());
        }
    }
    let mut merged = BTreeMap::new();
    for name in names {
        if let Some(value) = scopes.lookup(&name) {
            merged.insert(name, value.to_owned());
        }
    }
    merged
}

/// Applies the writes a script made to this run's scopes: a write replaces the entries the
/// scope has for its key, an unset removes them. Nothing reaches the workspace.
pub(crate) fn apply_writes(scopes: &mut Scopes, writes: &[VariableWrite]) {
    for write in writes {
        let scope = scopes.scope_mut(kind_of(write.scope));
        if let Some(value) = &write.value {
            scope
                .entries
                .push(ScopeEntry::new(write.key.clone(), value.clone(), true));
        } else {
            scope.entries.retain(|entry| entry.key != write.key);
        }
    }
}

/// The scope kind a script scope writes to; `pm.variables` is the local scope.
fn kind_of(scope: VariableScope) -> ScopeKind {
    match scope {
        VariableScope::Variables => ScopeKind::Local,
        VariableScope::Environment => ScopeKind::Environment,
        VariableScope::CollectionVariables => ScopeKind::Collection,
        VariableScope::Globals => ScopeKind::Globals,
    }
}

/// Runs `script` with `context`, reporting a failure as an error console entry in the
/// outcome it collected so far: a script error never fails the request.
pub(crate) fn run_bounded(script: &str, context: &ScriptContext) -> ScriptOutcome {
    match reqflux_script::run(script, context) {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = error.to_string();
            let mut outcome = error.outcome;
            outcome.console.push(ConsoleEntry {
                level: ConsoleLevel::Error,
                message,
            });
            outcome
        }
    }
}

/// Runs one script on a dedicated thread with its own current-thread Tokio runtime (S14),
/// so a script that waits for the network can block on it without blocking the caller's
/// thread — a GPU front end's executor — and without a `block_on` inside the engine's async
/// runtime. The runtime is created once per run and shared by every `pm.sendRequest` call
/// in it; the caller only joins the outcome. A thread or runtime failure is reported the
/// way every script failure is: an error console entry, never a failed request.
pub(crate) fn run_off_thread(
    script: &str,
    context: ScriptContext,
    transport: Transport,
) -> ScriptOutcome {
    let script = script.to_owned();
    let stage = context.stage;
    let spawned = std::thread::Builder::new()
        .name("reqflux-script".into())
        .spawn(move || run_on_thread(&script, context, transport));
    match spawned {
        Ok(handle) => match handle.join() {
            Ok(outcome) => outcome,
            Err(_) => failed(stage, "the script thread panicked".into()),
        },
        Err(error) => failed(stage, format!("the script thread could not start: {error}")),
    }
}

/// Runs one script like [`run_off_thread`], but delivers the outcome through a channel
/// instead of joining the thread (S14): awaiting the returned receiver never blocks the
/// awaiting executor, however long the script runs. A thread that panics or never starts
/// ends the channel, which the awaiting side reports as an error console entry.
pub(crate) fn spawn_off_thread(
    script: &str,
    context: ScriptContext,
    transport: Transport,
) -> oneshot::Receiver<ScriptOutcome> {
    let script = script.to_owned();
    let (sender, receiver) = oneshot::channel();
    // A spawn failure drops the sender together with the closure: the receiver then ends
    // without a result, which the awaiting side reports as a script failure.
    drop(
        std::thread::Builder::new()
            .name("reqflux-script".into())
            .spawn(move || {
                let outcome = run_on_thread(&script, context, transport);
                // The awaiting end is gone when the execution is dropped before finishing;
                // the outcome has nowhere to go and the thread simply ends.
                drop(sender.send(outcome));
            }),
    );
    receiver
}

/// Awaits one script run spawned by [`spawn_off_thread`], reporting a thread that ended
/// without a result — a panic, or a thread that could not start — as an error console
/// entry beside an empty outcome.
pub(crate) async fn run_off_thread_async(
    script: &str,
    context: ScriptContext,
    transport: Transport,
) -> ScriptOutcome {
    let stage = context.stage;
    let receiver = spawn_off_thread(script, context, transport);
    match receiver.await {
        Ok(outcome) => outcome,
        Err(_) => failed(stage, "the script thread panicked".into()),
    }
}

/// The body of one script run, on whichever thread calls it: a current-thread runtime
/// created once per run and shared by every `pm.sendRequest` call in it (S14).
fn run_on_thread(script: &str, mut context: ScriptContext, transport: Transport) -> ScriptOutcome {
    let stage = context.stage;
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return failed(
                stage,
                format!("the script runtime could not start: {error}"),
            );
        }
    };
    context.send_request = Some(send_through(runtime, transport));
    run_bounded(script, &context)
}

/// The host handler behind `pm.sendRequest`: one runtime per script run, one client, the
/// request's own settings. A transport failure reaches the script as the callback's
/// `error`; the request's timeout bounds how long the host can take. A request the guard
/// refuses is never sent: the refusal reaches the script the same way.
fn send_through(runtime: Runtime, transport: Transport) -> SendRequest {
    let Transport {
        client,
        options,
        guard,
        masker,
    } = transport;
    SendRequest::new(move |request: ScriptRequest| {
        if let Some(guard) = &guard {
            guard.check(&request.method, &request.url)?;
        }
        let rules = RedirectRules {
            check: guard
                .as_ref()
                .map(|guard| guard.redirect_check(&request.method)),
            mask: Some(masker.secret_mask()),
        };
        let prepared = PreparedRequest {
            method: request.method,
            url: request.url,
            headers: request.headers,
            body: match request.body {
                Some(text) => PreparedBody::Bytes {
                    content_type: None,
                    data: text.into_bytes(),
                },
                None => PreparedBody::None,
            },
            options,
        };
        runtime
            .block_on(client.send_with(prepared, CancellationToken::new(), rules))
            .map(script_response)
            .map_err(|error| error.to_string())
    })
}

/// The response a sendRequest callback receives, in `pm.response` shape.
fn script_response(response: Response) -> ScriptResponse {
    ScriptResponse {
        code: response.status,
        status: response.reason.clone().unwrap_or_default(),
        headers: response.headers.iter().cloned().collect(),
        body: String::from_utf8_lossy(&response.body).into_owned(),
    }
}

/// A run that could not even start — the thread or its runtime failed — reported as an
/// error console entry beside an empty outcome.
fn failed(stage: ScriptStage, message: String) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    outcome.console.push(ConsoleEntry {
        level: ConsoleLevel::Error,
        message: format!("{stage} script: {message}"),
    });
    outcome
}

/// Runs the effective script of `stage`, the request's own scripts being `own`, and returns
/// everything it produced.
///
/// A script error — a syntax error, a throw, a limit — never fails the request: the outcome
/// collected so far comes back with the error reported as an error console entry.
pub(crate) fn run_stage(
    stage: ScriptStage,
    target: &Target<'_>,
    own: &Scripts,
    request: &RequestView,
    scopes: &Scopes,
    response: Option<&Response>,
    transport: Transport,
) -> ScriptOutcome {
    let script = match stage {
        ScriptStage::PreRequest => effective_pre_request(target, own),
        ScriptStage::Test => effective_test(target, own),
    };
    let Some(script) = script else {
        return ScriptOutcome::default();
    };
    let context = ScriptContext {
        stage,
        request: request.clone(),
        variables: merged(scopes),
        response: response.map(response_view),
        send_request: None,
    };
    run_off_thread(script, context, transport)
}

/// Extends `outcome` with `other`, keeping the stage order: pre-request entries first.
pub(crate) fn merge(outcome: &mut ScriptOutcome, other: ScriptOutcome) {
    outcome.tests.extend(other.tests);
    outcome.console.extend(other.console);
    outcome.writes.extend(other.writes);
}
