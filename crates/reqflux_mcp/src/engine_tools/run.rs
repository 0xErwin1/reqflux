//! `collection_run` (v1-features S1): runs a collection, folder or request through
//! [`Engine::run_collection`] and returns the run summary, every URL, error and test text
//! masked.
//!
//! It is policed as `request_execute` is: the class and the environment before the
//! workspace is read and again after it is reloaded; an `ask` is asked in the window with
//! the target's name and how many requests the run holds. Every request of the run, and
//! every `pm.sendRequest` and redirect it makes, passes the hosts check through the engine's
//! host guard: a refused request is not sent and fails its step, and the run goes on unless
//! `bail` is set.
//!
//! One audit line records the run (the target, no URL), written before anything is sent:
//! when it cannot be written nothing runs. Then one line per request, with its method and
//! masked URL: refused with the refusal when the guard refused it or one of its script
//! sends or redirects, allowed otherwise. A request line that cannot be written stops the
//! run. Every send is recorded in the window's history, secrets masked; a request that was
//! not sent is not.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use reqflux_core::model::{Item, Workspace};
use reqflux_engine::audit::Decision;
use reqflux_engine::config::Rule;
use reqflux_engine::policy::{Policy, Refusal as PolicyRefusal, ToolClass};
use reqflux_engine::state::AppState;
use reqflux_engine::{
    CancellationToken, ExecuteOptions, HostGuard, Masker, RunEnd, RunEvent, RunOptions, RunSummary,
    StepResult,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::policy::{self, Call, Scope};
use super::{EngineState, encode};
use crate::tools::ToolError;

const TOOL: &str = "collection_run";

/// Arguments of `collection_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunArgs {
    /// The collection, folder or request to run, by id or slug path
    /// (`<collection>/<folder>/...`).
    pub target: String,
    /// The environment name or slug; the collection's default unless passed.
    pub environment: Option<String>,
    /// Stop at the first failed request or test; `false` unless passed.
    pub bail: Option<bool>,
}

/// What the host guard refused during one step, raw: the URL may hold a secret until it is
/// masked.
#[derive(Debug)]
struct Refusal {
    url: String,
    reason: String,
    redirect: bool,
}

/// The refusals of the step running now, shared with the guard on the script thread and the
/// transport.
#[derive(Debug, Clone, Default)]
struct Refused(Arc<Mutex<Vec<Refusal>>>);

impl Refused {
    fn guard(&self, policy: Policy) -> HostGuard {
        let refused = self.clone();
        HostGuard::with_redirects(move |_method, url, redirect| {
            policy.check_host(TOOL, url).map_err(|refusal| {
                let reason = refusal.to_string();
                refused.lock().push(Refusal {
                    url: url.to_owned(),
                    reason: reason.clone(),
                    redirect,
                });
                reason
            })
        })
    }

    fn take(&self) -> Vec<Refusal> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Refusal>> {
        // A poisoned list still holds every refusal pushed before the panic.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `collection_run`: runs `target` with `environment`, stopping at the first failure with
/// `bail`.
///
/// # Errors
///
/// The policy refused the run, an audit line could not be written, no open workspace, or
/// the target or environment is unknown.
pub fn collection_run(state: &mut EngineState, args: RunArgs) -> Result<String, ToolError> {
    let paths = state.paths.clone();
    let checked = |policy: &Policy| -> Result<Option<PolicyRefusal>, PolicyRefusal> {
        let ask = match policy.check_class(TOOL, ToolClass::Execute) {
            Ok(()) => None,
            Err(refusal) if policy.rule(ToolClass::Execute) == Rule::Ask => Some(refusal),
            Err(refusal) => return Err(refusal),
        };
        policy.check_environment(TOOL, args.environment.as_deref())?;
        Ok(ask)
    };
    let target = state
        .engine
        .as_ref()
        .and_then(|engine| resolve(engine.workspace(), &args.target));
    let call = state.call(
        TOOL,
        Scope::Open,
        &[target.as_ref().map_or("", |t| t.id.as_str())],
    );
    if let Err(refusal) = checked(&policy::evaluate(&paths, state.engine.as_ref())) {
        return Err(call.refuse(&paths, &refusal.to_string(), None));
    }
    let planned = state
        .reloaded()
        .and_then(|engine| {
            engine
                .run_plan(&args.target)
                .map_err(|error| ToolError::Engine(error.to_string()))
        })
        .map(|items| items.len());
    let count = match planned {
        Ok(count) => count,
        Err(error) => {
            call.audit(&paths, Decision::Allowed, None, None)?;
            return Err(error);
        }
    };
    // The workspace was reloaded: check again with its manifest as it is now.
    let policy = policy::evaluate(&paths, state.engine.as_ref());
    let ask = match checked(&policy) {
        Ok(ask) => ask,
        Err(refusal) => return Err(call.refuse(&paths, &refusal.to_string(), None)),
    };
    let approval = match ask {
        None => None,
        Some(refusal) => {
            let name = state
                .engine
                .as_ref()
                .and_then(|engine| resolve(engine.workspace(), &args.target))
                .map_or_else(|| args.target.clone(), |target| target.name);
            let noun = if count == 1 { "request" } else { "requests" };
            let shown = vec![format!("{name} · {count} {noun}")];
            let environment = args.environment.as_deref();
            match state.approve_as(&call, &refusal, None, environment, Some(shown)) {
                Ok(reason) => Some(reason),
                Err(text) => return Err(call.refuse(&paths, &text, None)),
            }
        }
    };
    call.audit(&paths, Decision::Allowed, approval, None)?;

    let mut history = match &state.state_path {
        Some(path) => AppState::open(path),
        None => AppState::open_default(),
    }
    .map_err(|error| {
        log::warn!(target: "reqflux_mcp", "the run is not recorded in history: {error}");
    })
    .ok();
    let engine = state.engine()?;
    let (masker, _) = engine.workspace_masker();
    let refused = Refused::default();
    let options = RunOptions {
        execute: ExecuteOptions {
            environment: args.environment.clone(),
            host_guard: Some(refused.guard(policy)),
            // The server never reads current values: they belong to the window's app state.
            ..ExecuteOptions::default()
        },
        bail: args.bail.unwrap_or(false),
        max_steps: None,
    };
    let cancel = CancellationToken::new();
    let mut audit_error = None;
    let mut on_event = |event: &RunEvent| match event {
        RunEvent::Started { .. } => drop(refused.take()),
        RunEvent::Finished(result) => {
            if let Err(error) = audit_step(&call, &paths, result, refused.take(), &masker) {
                audit_error = Some(error);
                cancel.cancel();
            }
        }
    };
    let summary = engine
        .run_collection(
            &args.target,
            &options,
            &cancel,
            history.as_mut(),
            &mut on_event,
        )
        .map_err(|error| ToolError::Engine(masker.mask(&error.to_string())))?;
    if let Some(error) = audit_error {
        return Err(error);
    }
    encode(&output(&summary, &masker))
}

/// Writes the audit line of one step: refused when the guard refused the request or one of
/// its script sends or redirects.
fn audit_step(
    call: &Call,
    paths: &policy::PolicyPaths,
    result: &StepResult,
    refusals: Vec<Refusal>,
    masker: &Masker,
) -> Result<(), ToolError> {
    let step = Call {
        targets: vec![result.item.id.clone()],
        ..call.clone()
    };
    let sent = result.method.as_deref().zip(result.url.as_deref());
    // The guard refused the request itself: it failed with the refusal, nothing sent.
    let main = result.status.is_none()
        && refusals.last().is_some_and(|refusal| {
            !refusal.redirect
                && result
                    .error
                    .as_deref()
                    .is_some_and(|error| error == masker.mask(&refusal.reason))
        });
    let reason = if main {
        result.error.clone()
    } else {
        refusals
            .first()
            .map(|refusal| refusal_text(refusal, masker))
    };
    match reason {
        Some(reason) => step.audit(paths, Decision::Refused, Some(&reason), sent),
        None => step.audit(paths, Decision::Allowed, None, sent),
    }
}

/// The text of a refused script send or redirect, masked.
fn refusal_text(refusal: &Refusal, masker: &Masker) -> String {
    if refusal.redirect {
        masker.mask(&format!(
            "a redirect to {} was refused: {}",
            refusal.url, refusal.reason
        ))
    } else {
        masker.mask(&format!(
            "a script's pm.sendRequest was refused: {}",
            refusal.reason
        ))
    }
}

/// The run summary as the tool returns it; the engine masked every URL, error and test.
fn output(summary: &RunSummary, masker: &Masker) -> Value {
    let results: Vec<Value> = summary
        .results
        .iter()
        .map(|result| {
            let tests: Vec<Value> = result
                .tests
                .iter()
                .map(|test| {
                    json!({
                        "name": test.name,
                        "passed": test.passed,
                        "message": test.message,
                    })
                })
                .collect();
            json!({
                "step": result.step,
                "id": result.item.id,
                "name": result.item.name,
                "path": result.item.path,
                "method": result.method,
                "url": result.url,
                "status": result.status,
                "elapsed_ms": result.elapsed_ms,
                "size": result.size,
                "tests": tests,
                "error": result.error,
            })
        })
        .collect();
    let (end, detail) = match &summary.end {
        RunEnd::Completed => ("completed", None),
        RunEnd::Stopped => ("stopped", None),
        RunEnd::Bailed => ("bailed", None),
        RunEnd::Cancelled => ("cancelled", None),
        RunEnd::StepLimit(limit) => ("step_limit", Some(limit.to_string())),
        // A script wrote the name: mask it like any other script text.
        RunEnd::UnknownNext(target) => ("unknown_next", Some(masker.mask(target))),
    };
    json!({
        "results": results,
        "totals": {
            "requests": summary.requests(),
            "failed_requests": summary.failed_requests(),
            "tests_passed": summary.tests_passed(),
            "tests_failed": summary.tests_failed(),
        },
        "passed": summary.passed(),
        "end": end,
        "end_detail": detail,
        "elapsed_ms": summary.elapsed_ms,
    })
}

/// The collection, folder or request a run target names.
#[derive(Debug)]
struct Target {
    id: String,
    name: String,
}

/// The node `selector` names, by id first, then by slug path, a request winning over a
/// folder at the same path, as the engine's run plan resolves it.
fn resolve(workspace: &Workspace, selector: &str) -> Option<Target> {
    by_id(workspace, selector).or_else(|| by_path(workspace, selector))
}

fn by_id(workspace: &Workspace, id: &str) -> Option<Target> {
    workspace.collections.iter().find_map(|collection| {
        if collection.collection.id.to_string() == id {
            Some(Target {
                id: id.to_owned(),
                name: collection.collection.name.clone(),
            })
        } else {
            item_by_id(&collection.items, id)
        }
    })
}

fn item_by_id(items: &[Item], id: &str) -> Option<Target> {
    items.iter().find_map(|item| match item {
        Item::Folder(folder) if folder.folder.id.to_string() == id => Some(Target {
            id: id.to_owned(),
            name: folder.folder.name.clone(),
        }),
        Item::Folder(folder) => item_by_id(&folder.items, id),
        Item::Request(request) if request.request.id.to_string() == id => Some(Target {
            id: id.to_owned(),
            name: request.request.name.clone(),
        }),
        Item::Request(_) => None,
    })
}

fn by_path(workspace: &Workspace, path: &str) -> Option<Target> {
    let mut segments = path.split('/');
    let first = segments.next()?;
    let collection = workspace
        .collections
        .iter()
        .find(|collection| collection.slug == first)?;
    let mut found = Target {
        id: collection.collection.id.to_string(),
        name: collection.collection.name.clone(),
    };
    let mut items: &[Item] = &collection.items;
    for slug in segments {
        let request = items.iter().find_map(|item| match item {
            Item::Request(request) if request.slug == slug => Some(request),
            _ => None,
        });
        if let Some(request) = request {
            found = Target {
                id: request.request.id.to_string(),
                name: request.request.name.clone(),
            };
            items = &[];
            continue;
        }
        let folder = items.iter().find_map(|item| match item {
            Item::Folder(folder) if folder.slug == slug => Some(folder),
            _ => None,
        })?;
        found = Target {
            id: folder.folder.id.to_string(),
            name: folder.folder.name.clone(),
        };
        items = &folder.items;
    }
    Some(found)
}
