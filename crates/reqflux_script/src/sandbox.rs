//! The QuickJS runtime: limits, evaluation and error classification.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rquickjs::context::EvalOptions;
use rquickjs::{Context, Ctx, Runtime};

use crate::pm;
use crate::shim::PRELUDE;
use crate::{
    ConsoleEntry, NextRequest, ScriptContext, ScriptError, ScriptErrorKind, ScriptOutcome,
    ScriptStage, TestResult, VariableScope, VariableWrite,
};

/// QuickJS heap cap for one script run.
const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
/// Wall-clock cap enforced by the QuickJS interrupt handler and by every host call.
const TIME_LIMIT: Duration = Duration::from_secs(5);
/// Cap on what the host itself collects for a script: the interrupt handler cannot bound a
/// run whose wall time sits in host calls, so the collected state is capped too.
const HOST_STATE_LIMIT_BYTES: usize = 8 * 1024 * 1024;

/// The [`State`] shared between the host functions and the sandbox driver.
type SharedState = Arc<Mutex<State>>;

/// Everything the host collects during one script run.
pub(super) struct State {
    deadline: Instant,
    /// Bytes the host has copied out of the script so far.
    bytes: usize,
    /// Set when the host stopped the run, so the failure kind needs no message sniffing.
    aborted: Option<ScriptErrorKind>,
    /// Whether the user script started executing, which tells a parse failure from a throw.
    reached: bool,
    variables: BTreeMap<String, String>,
    /// Writes and unsets applied during the run, checked before `variables` on a read.
    overlay: HashMap<(VariableScope, String), Option<String>>,
    writes: Vec<VariableWrite>,
    tests: Vec<TestResult>,
    console: Vec<ConsoleEntry>,
    next_request: Option<NextRequest>,
}

impl State {
    pub(super) fn new(context: &ScriptContext, deadline: Instant) -> Self {
        Self {
            deadline,
            bytes: 0,
            aborted: None,
            reached: false,
            variables: context.variables.clone(),
            overlay: HashMap::new(),
            writes: Vec::new(),
            tests: Vec::new(),
            console: Vec::new(),
            next_request: None,
        }
    }

    /// The kind that stopped the run, when the host stopped it.
    pub(super) fn aborted(&self) -> Option<ScriptErrorKind> {
        self.aborted
    }

    /// Whether the user script began executing.
    pub(super) fn reached(&self) -> bool {
        self.reached
    }

    pub(super) fn mark_reached(&mut self) {
        self.reached = true;
    }

    /// Refuses more host work once the deadline passed or the collected state is too big,
    /// remembering why so the driver reports the right kind.
    pub(super) fn charge(&mut self, bytes: usize) -> Option<ScriptErrorKind> {
        if Instant::now() >= self.deadline {
            self.aborted.get_or_insert(ScriptErrorKind::Timeout);
        }
        if self.bytes.saturating_add(bytes) > HOST_STATE_LIMIT_BYTES {
            self.aborted.get_or_insert(ScriptErrorKind::MemoryLimit);
        }
        self.aborted
    }

    fn spent(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
    }

    pub(super) fn var_get(&self, scope: VariableScope, key: &str) -> Option<String> {
        match self.overlay.get(&(scope, key.to_string())) {
            Some(Some(value)) => Some(value.clone()),
            Some(None) => None,
            None => self.variables.get(key).cloned(),
        }
    }

    pub(super) fn var_set(&mut self, scope: VariableScope, key: String, value: String) {
        self.spent(key.len().saturating_add(value.len()));
        self.writes.push(VariableWrite {
            scope,
            key: key.clone(),
            value: Some(value.clone()),
        });
        self.overlay.insert((scope, key), Some(value));
    }

    pub(super) fn var_unset(&mut self, scope: VariableScope, key: String) {
        self.spent(key.len());
        self.writes.push(VariableWrite {
            scope,
            key: key.clone(),
            value: None,
        });
        self.overlay.insert((scope, key), None);
    }

    pub(super) fn record_test(&mut self, name: String, passed: bool, message: Option<String>) {
        self.spent(
            name.len()
                .saturating_add(message.as_deref().map_or(0, str::len)),
        );
        self.tests.push(TestResult {
            name,
            passed,
            message,
        });
    }

    pub(super) fn record_console(&mut self, level: crate::ConsoleLevel, message: String) {
        self.spent(message.len());
        self.console.push(ConsoleEntry { level, message });
    }

    /// Records a `setNextRequest` call; the last one wins.
    pub(super) fn set_next_request(&mut self, next: NextRequest) {
        if let NextRequest::Request(target) = &next {
            self.spent(target.len());
        }
        self.next_request = Some(next);
    }

    pub(super) fn outcome(&self) -> ScriptOutcome {
        ScriptOutcome {
            tests: self.tests.clone(),
            console: self.console.clone(),
            writes: self.writes.clone(),
            next_request: self.next_request.clone(),
        }
    }
}

pub(super) fn run(script: &str, context: &ScriptContext) -> Result<ScriptOutcome, ScriptError> {
    let runtime = Runtime::new().map_err(|error| {
        internal(
            context.stage,
            format!("creating the runtime failed: {error}"),
        )
    })?;
    runtime.set_memory_limit(MEMORY_LIMIT_BYTES);
    let deadline = Instant::now() + TIME_LIMIT;
    // In rquickjs the handler returns `true` when the run must stop.
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let js_context = Context::full(&runtime).map_err(|error| {
        internal(
            context.stage,
            format!("creating the context failed: {error}"),
        )
    })?;
    js_context.with(|ctx| execute(&ctx, script, context, deadline))
}

fn execute(
    ctx: &Ctx<'_>,
    script: &str,
    context: &ScriptContext,
    deadline: Instant,
) -> Result<ScriptOutcome, ScriptError> {
    let state: SharedState = Arc::new(Mutex::new(State::new(context, deadline)));
    pm::install(ctx, &state, context.send_request.clone()).map_err(|error| {
        internal(
            context.stage,
            format!("installing the pm shim failed: {error}"),
        )
    })?;
    eval_source(ctx, &pm::init_source(context), context, &state, false)?;
    eval_source(ctx, PRELUDE, context, &state, false)?;
    // The marker runs before any user statement, so a script that never reached it did not
    // parse; anything that fails after it threw at run time.
    let marked = format!("__reqfluxReach();\n{script}");
    eval_source(ctx, &marked, context, &state, true)?;
    Ok(lock(&state).outcome())
}

fn lock(state: &SharedState) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Evaluates one source in the sandbox, mapping a failure to a [`ScriptError`] that carries
/// the outcome collected so far.
fn eval_source(
    ctx: &Ctx<'_>,
    source: &str,
    context: &ScriptContext,
    state: &SharedState,
    user_script: bool,
) -> Result<(), ScriptError> {
    let mut options = EvalOptions::default();
    options.strict = false;
    options.filename = Some("script".into());
    ctx.eval_with_options::<(), _>(source, options)
        .map_err(|error| {
            let exception = ctx.catch();
            let message = exception_message(&exception);
            // QuickJS leaves undefined, or null, where an error object would be when the heap
            // itself gave out. A script that throws a bare null is reported as a memory
            // failure too; that is the cost of telling them apart without asking the runtime,
            // which is unavailable while the eval is unwinding.
            let bare = exception.is_undefined() || exception.is_null();
            let kind = classify(&error, &message, state, user_script, bare);
            ScriptError {
                kind,
                stage: context.stage,
                message,
                outcome: Box::new(current_outcome(state)),
            }
        })
}

/// Maps a QuickJS failure to a failure kind.
///
/// The host's own reason wins, then QuickJS's two internal failures, which are matched
/// exactly so a script that throws `new Error("interrupted")` is a plain throw. A user script
/// that never reached its first statement did not parse, which is how a parse failure is told
/// apart from a `SyntaxError` thrown at run time.
fn classify(
    error: &rquickjs::Error,
    message: &str,
    state: &SharedState,
    user_script: bool,
    bare_failure: bool,
) -> ScriptErrorKind {
    let (aborted, reached) = {
        let state = lock(state);
        (state.aborted(), state.reached())
    };
    if let Some(kind) = aborted {
        return kind;
    }
    if matches!(error, rquickjs::Error::Allocation) || message == "InternalError: out of memory" {
        return ScriptErrorKind::MemoryLimit;
    }
    if message == "InternalError: interrupted" {
        return ScriptErrorKind::Timeout;
    }
    if user_script && !reached {
        return ScriptErrorKind::Syntax;
    }
    // Growing the heap past the limit fails without leaving any exception value, which is how
    // it is told apart from a script that throws something of its own.
    if matches!(error, rquickjs::Error::Exception) && bare_failure {
        return ScriptErrorKind::MemoryLimit;
    }
    ScriptErrorKind::Thrown
}

/// Reads the message of a caught exception: its text, its `message`, its `name`, or a fallback.
fn exception_message(exception: &rquickjs::Value<'_>) -> String {
    if exception.type_of() == rquickjs::Type::String
        && let Some(text) = exception.as_string()
        && let Ok(text) = text.to_string()
    {
        return text;
    }
    if let Some(object) = exception.as_object() {
        let name = object.get::<_, Option<String>>("name").ok().flatten();
        let message = object.get::<_, Option<String>>("message").ok().flatten();
        match (name, message) {
            (Some(name), Some(message)) if !name.is_empty() => return format!("{name}: {message}"),
            (None, Some(message)) => return message,
            (Some(name), None) if !name.is_empty() => return name,
            _ => {}
        }
    }
    "unknown error".into()
}

fn current_outcome(state: &SharedState) -> ScriptOutcome {
    lock(state).outcome()
}

fn internal(stage: ScriptStage, message: String) -> ScriptError {
    ScriptError {
        kind: ScriptErrorKind::Internal,
        stage,
        message,
        outcome: Box::default(),
    }
}
