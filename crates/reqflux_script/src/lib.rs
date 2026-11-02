//! The QuickJS sandbox that runs Postman `pm.*` scripts.
//!
//! The crate takes plain data in and returns plain data out: the caller supplies the stage,
//! the request view, the merged variable scope and, for a test script, the response view. It
//! never performs HTTP and never touches the engine; variable writes come back as data for
//! the caller to apply.
//!
//! `pm.setNextRequest`, `pm.execution.setNextRequest` and `postman.setNextRequest` record
//! where a collection run goes next in [`ScriptOutcome::next_request`]: a string names the
//! request by name or id, `null` stops the run, the last call wins, and any other argument
//! throws. Both stages record it; a single send ignores it.

mod modules;
mod pm;
mod sandbox;
mod shim;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

/// The stage a script runs in. The stage names the run in errors and picks the response view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptStage {
    /// A pre-request script: it runs before the request is sent.
    PreRequest,
    /// A test script: it runs after the response arrives, with `pm.response` present.
    Test,
}

impl ScriptStage {
    /// The stage name used in errors and script info.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreRequest => "pre-request",
            Self::Test => "test",
        }
    }
}

impl std::fmt::Display for ScriptStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One of the four Postman variable scopes a script can write to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableScope {
    /// The local scope of the current execution (`pm.variables`).
    Variables,
    /// The active environment (`pm.environment`).
    Environment,
    /// The collection variables (`pm.collectionVariables`).
    CollectionVariables,
    /// The workspace globals (`pm.globals`).
    Globals,
}

impl VariableScope {
    /// The scope name used in the wire data and the shim.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Variables => "variables",
            Self::Environment => "environment",
            Self::CollectionVariables => "collectionVariables",
            Self::Globals => "globals",
        }
    }
}

/// The severity of one console entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    /// `console.log`
    Log,
    /// `console.info`
    Info,
    /// `console.warn`
    Warn,
    /// `console.error`
    Error,
}

impl ConsoleLevel {
    /// The level name used in the wire data and the shim.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// A read-only view of the request being sent, as `pm.request` exposes it.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestView {
    /// The HTTP method, upper case.
    pub method: String,
    /// The request URL with its query string.
    pub url: String,
    /// The request headers by name.
    pub headers: BTreeMap<String, String>,
    /// The raw body text, when the request has a body.
    pub body: Option<String>,
}

/// A read-only view of the response, as `pm.response` exposes it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseView {
    /// The HTTP status code.
    pub code: u16,
    /// The HTTP status text.
    pub status: String,
    /// The response time in milliseconds, when the caller measured one.
    pub response_time_ms: u64,
    /// The response size in bytes, when the caller measured one.
    pub response_size: u64,
    /// The response headers by name.
    pub headers: BTreeMap<String, String>,
    /// The raw body text.
    pub body_text: String,
    /// The parsed body, when the caller parsed it.
    pub parsed_json: Option<Value>,
}

/// The request a script asks the host to send with `pm.sendRequest` (S13). The host turns
/// it into the same prepared request its own transport sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptRequest {
    /// The HTTP method, upper case.
    pub method: String,
    /// The request URL.
    pub url: String,
    /// The headers in the order the script gave them; a name may repeat.
    pub headers: Vec<(String, String)>,
    /// The raw body text, when the script supplied one.
    pub body: Option<String>,
}

/// The host's answer to a [`ScriptRequest`], as the script's callback receives it (S13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptResponse {
    /// The HTTP status code.
    pub code: u16,
    /// The HTTP status text.
    pub status: String,
    /// The response headers by name.
    pub headers: BTreeMap<String, String>,
    /// The raw body text.
    pub body: String,
}

/// The host side of `pm.sendRequest` (S13): sends one [`ScriptRequest`] and answers with
/// the response, or the failure text that reaches the script as the callback's `error`.
/// The sandbox calls it from the script thread and it may block for as long as the host
/// needs; cloning shares the handler.
#[derive(Clone)]
pub struct SendRequest(
    pub Arc<dyn Fn(ScriptRequest) -> Result<ScriptResponse, String> + Send + Sync>,
);

impl std::fmt::Debug for SendRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SendRequest(<host handler>)")
    }
}

impl PartialEq for SendRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl SendRequest {
    /// Wraps a handler.
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(ScriptRequest) -> Result<ScriptResponse, String> + Send + Sync + 'static,
    {
        Self(Arc::new(handler))
    }
}

/// Everything a script can read, supplied by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptContext {
    /// The stage the script runs in.
    pub stage: ScriptStage,
    /// The request view for `pm.request`.
    pub request: RequestView,
    /// The merged variables the script reads, resolved by the caller.
    pub variables: BTreeMap<String, String>,
    /// The response view for `pm.response`, present only in a test script.
    pub response: Option<ResponseView>,
    /// The host side of `pm.sendRequest`; absent when the host offers no transport.
    pub send_request: Option<SendRequest>,
}

/// One recorded `pm.test` result.
#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    /// The test name.
    pub name: String,
    /// Whether the test body ran without throwing.
    pub passed: bool,
    /// The thrown message when the test failed.
    pub message: Option<String>,
}

/// One recorded `console.*` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsoleEntry {
    /// The severity of the entry.
    pub level: ConsoleLevel,
    /// The stringified message.
    pub message: String,
}

/// One variable change a script made, for the caller to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableWrite {
    /// The scope the change belongs to.
    pub scope: VariableScope,
    /// The variable name.
    pub key: String,
    /// The new value, or `None` for an unset.
    pub value: Option<String>,
}

/// Where a collection run goes after the current request, as `pm.setNextRequest` asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextRequest {
    /// `setNextRequest(null)`: the run stops after the current request.
    Stop,
    /// `setNextRequest("...")`: the run jumps to the request with this name or id, as given.
    Request(String),
}

/// Everything a script produced: test results, console entries, variable writes and the
/// next request a collection run should send.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptOutcome {
    /// The `pm.test` results in call order.
    pub tests: Vec<TestResult>,
    /// The console entries in call order.
    pub console: Vec<ConsoleEntry>,
    /// The variable writes in call order.
    pub writes: Vec<VariableWrite>,
    /// The last `pm.setNextRequest` / `postman.setNextRequest` call, or `None` when the
    /// script made none. Only a collection run reads it.
    pub next_request: Option<NextRequest>,
}

/// The kind of a script failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScriptErrorKind {
    /// The script did not parse.
    #[error("syntax error")]
    Syntax,
    /// The script threw an error outside `pm.test`.
    #[error("uncaught error")]
    Thrown,
    /// The script ran past the time limit.
    #[error("time limit exceeded")]
    Timeout,
    /// The script allocated past the memory limit.
    #[error("memory limit exceeded")]
    MemoryLimit,
    /// The sandbox itself failed.
    #[error("internal error")]
    Internal,
}

/// A script failure, with the stage that failed and the outcome collected so far.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{stage} script {kind}: {message}")]
pub struct ScriptError {
    /// What failed.
    pub kind: ScriptErrorKind,
    /// The stage the script ran in.
    pub stage: ScriptStage,
    /// The failure message.
    pub message: String,
    /// The tests, console entries and writes collected before the failure.
    /// Boxed so the error stays small on the `Ok` path of every run.
    pub outcome: Box<ScriptOutcome>,
}

/// Runs one script in an isolated QuickJS context and collects its effects.
///
/// The script is bounded by a memory limit and a time limit; a syntax error, a thrown error,
/// a time-out or an out-of-memory run returns a [`ScriptError`] naming the stage together with
/// the outcome collected so far, and never panics or hangs the caller.
pub fn run(script: &str, context: &ScriptContext) -> Result<ScriptOutcome, ScriptError> {
    sandbox::run(script, context)
}
