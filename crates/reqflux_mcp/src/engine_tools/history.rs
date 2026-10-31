//! `history_list` and `history_get` (S5): the window's history of the open workspace,
//! read from the per-user app state.
//!
//! What history recorded is masked again on the way out (S7) with every secret of the
//! workspace now ([`Engine::workspace_masker`](reqflux_engine::Engine::workspace_masker)):
//! the request document keeps literal auth values as written, and the response headers,
//! body and error text were recorded as received, so they can echo a secret. Credential
//! header values are masked whole. When the secret store cannot be read, the masker lacks
//! those values: `history_get` then leaves out the response body and header values and
//! says why, rather than risk returning a secret.

use reqflux_core::model::Request;
use reqflux_engine::state::{AppState, HistoryEntry, HistoryOutcome};
use reqflux_engine::{MASK, Masker, SentBody};
use serde_json::{Value, json};

use super::policy::Scope;
use super::request::{masked, masked_headers};
use super::{EngineState, EntryArgs, HistoryListArgs, cut, encode};
use crate::tools::ToolError;

/// How many entries `history_list` returns unless told otherwise.
const DEFAULT_LIMIT: u32 = 20;

/// The most entries `history_list` returns.
const MAX_LIMIT: u32 = 200;

impl EngineState {
    /// The app state, the open workspace's id and what masks the workspace's secrets now,
    /// with why it lacks some. Only the history tools need the app state, so failing to
    /// open it fails them alone.
    fn history_source(&mut self) -> Result<(AppState, String, Masking), ToolError> {
        let engine = self.engine()?;
        // The secrets in scope now: a workspace that no longer loads keeps the last one.
        if let Err(error) = engine.reload() {
            log::warn!(target: "reqflux_mcp", "history masks with the workspace as last loaded: {error}");
        }
        let workspace_id = engine.workspace().manifest.id.to_string();
        let (masker, problem) = engine.workspace_masker();
        let opened = match &self.state_path {
            Some(path) => AppState::open(path),
            None => AppState::open_default(),
        };
        let app = opened
            .map_err(|error| ToolError::Engine(format!("could not open the app state: {error}")))?;
        Ok((app, workspace_id, Masking { masker, problem }))
    }
}

/// What masks history output, and why it lacks some secrets, if it does.
struct Masking {
    masker: Masker,
    problem: Option<String>,
}

impl Masking {
    fn text(&self, text: &str) -> String {
        self.masker.mask(text)
    }

    fn headers(&self, headers: &[(String, String)]) -> Vec<(String, String)> {
        headers
            .iter()
            .map(|(name, value)| self.masker.header_for_agent(name, value))
            .collect()
    }
}

fn state_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::Engine(format!("could not read the history: {error}"))
}

/// `history_list`: the newest `limit` entries of the open workspace, newest first.
///
/// # Errors
///
/// No open workspace, or the app state cannot open or be read.
pub fn history_list(state: &mut EngineState, args: HistoryListArgs) -> Result<String, ToolError> {
    state.authorize("history_list", Scope::Open, &[])?;
    let (app, workspace_id, masking) = state.history_source()?;
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let entries = app
        .history(&workspace_id, usize::try_from(limit).unwrap_or(usize::MAX))
        .map_err(state_error)?;
    let entries: Vec<Value> = entries
        .iter()
        .map(|entry| summary(entry, &masking))
        .collect();
    encode(&json!({ "entries": entries }))
}

/// `history_get`: one entry of the open workspace with its request and response.
///
/// # Errors
///
/// No open workspace, the app state cannot open or be read, or no entry of the open
/// workspace has this id.
pub fn history_get(state: &mut EngineState, args: EntryArgs) -> Result<String, ToolError> {
    state.authorize("history_get", Scope::Open, &[])?;
    let (app, workspace_id, masking) = state.history_source()?;
    let entry = app
        .history_entry(args.entry_id)
        .map_err(state_error)?
        .filter(|entry| entry.workspace_id == workspace_id)
        .ok_or_else(|| {
            ToolError::Engine(format!(
                "history entry {} not found in the open workspace",
                args.entry_id
            ))
        })?;
    encode(&detail(&entry, &masking))
}

fn summary(entry: &HistoryEntry, masking: &Masking) -> Value {
    let record = &entry.record;
    let (status, error, elapsed_ms) = match &record.outcome {
        HistoryOutcome::Response(response) => (Some(response.status), None, response.elapsed_ms),
        HistoryOutcome::Failed { error, elapsed_ms } => {
            (None, Some(masking.text(error)), *elapsed_ms)
        }
    };
    json!({
        "entry_id": entry.id,
        "sent_at": record.sent_at,
        "request_id": record.request_id,
        "request_name": record.request_name,
        "request_path": record.request_path,
        "method": record.sent.method,
        "url": masking.text(&record.sent.url),
        "status": status,
        "elapsed_ms": elapsed_ms,
        "error": error,
    })
}

fn detail(entry: &HistoryEntry, masking: &Masking) -> Value {
    let record = &entry.record;
    let (kind, text) = match &record.sent.body {
        SentBody::None => ("none", None),
        SentBody::Text(text) => ("text", Some(masking.text(text))),
        SentBody::Omitted(note) => ("omitted", Some(masking.text(note))),
    };
    let (response, error) = match &record.outcome {
        HistoryOutcome::Response(response) => {
            let (body, body_bytes, cut_here) = match std::str::from_utf8(&response.body) {
                Ok(_) if masking.problem.is_some() => (None, Some(response.body.len()), false),
                Ok(text) => {
                    let (text, cut_here) = cut(masking.text(text));
                    (Some(text), None, cut_here)
                }
                Err(_) => (None, Some(response.body.len()), false),
            };
            let headers: Vec<(String, String)> = match &masking.problem {
                Some(_) => response
                    .headers
                    .iter()
                    .map(|(name, _)| (masking.text(name), MASK.to_owned()))
                    .collect(),
                None => masking.headers(&response.headers),
            };
            let omitted = masking.problem.as_ref().map(|problem| {
                format!("the response body and header values are left out: {problem}")
            });
            let response = json!({
                "status": response.status,
                "reason": response.reason.as_deref().map(|reason| masking.text(reason)),
                "headers": headers,
                "body": body,
                "body_bytes": body_bytes,
                "truncated": response.truncated || cut_here,
                "elapsed_ms": response.elapsed_ms,
                "size": response.size,
                "omitted": omitted,
            });
            (response, None)
        }
        HistoryOutcome::Failed { error, .. } => (Value::Null, Some(masking.text(error))),
    };
    json!({
        "entry_id": entry.id,
        "sent_at": record.sent_at,
        "request_id": record.request_id,
        "request_name": record.request_name,
        "request_path": record.request_path,
        "document": masked_document(&record.document).map(|document| masking.text(&document)),
        "request": {
            "method": record.sent.method,
            "url": masking.text(&record.sent.url),
            "headers": masking.headers(&record.sent.headers),
            "body": { "kind": kind, "text": text },
        },
        "response": response,
        "error": error,
    })
}

/// The recorded document with its auth secrets and credential header values masked, or
/// `None` when it no longer parses as a request (its auth could not be masked, so it is not
/// returned).
fn masked_document(document: &str) -> Option<String> {
    let mut request: Request = toml::from_str(document).ok()?;
    request.auth = masked(&request.auth);
    request.headers = masked_headers(&request.headers);
    toml::to_string(&request).ok()
}
