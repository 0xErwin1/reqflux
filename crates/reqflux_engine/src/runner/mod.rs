//! The collection runner (v1-features S1): runs every request of a collection or folder in
//! tree order through the same path as [`Engine::execute`], the one API the window, the CLI
//! and MCP share.
//!
//! - Each request runs its effective pre-request and test scripts. A script's writes to
//!   `pm.globals`, `pm.collectionVariables` and `pm.environment` carry to the next requests of
//!   the run as current values ([`RunSummary::current`]); nothing reaches the workspace or
//!   the app state. `pm.variables` stays local to its request, and a write to a secret
//!   variable does not carry: a secret never takes a current value.
//! - `pm.setNextRequest(name|id)` jumps to that request of the run (an id first, then the
//!   first request with that name); `pm.setNextRequest(null)` stops after the current one. A
//!   name the run does not hold ends it as [`RunEnd::UnknownNext`]. The run sends at most
//!   [`RunOptions::max_steps`] requests, so a jump loop ends as [`RunEnd::StepLimit`].
//! - A cancellation token stops the run: the send in flight is cancelled and nothing more is
//!   sent. `bail` stops at the first failed step.
//! - Every send is recorded in history when the caller passes the app state, as a window send
//!   is, secrets masked; a request that fails before it is sent, or a cancelled send, is not.
//! - Events and results carry the masked URL and masked error and test text, never a secret.

mod plan;
mod step;

use std::time::Instant;

use reqflux_script::{NextRequest, TestResult};
use tokio_util::sync::CancellationToken;

use crate::current::CurrentValues;
use crate::engine::Engine;
use crate::error::ExecuteError;
use crate::prepare::{ExecuteOptions, select_environment};
use crate::state::AppState;

/// How many requests a run sends at most when [`RunOptions::max_steps`] is `None`.
pub const DEFAULT_MAX_STEPS: usize = 1000;

/// What the caller adds to a collection run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// The options every request of the run executes with: the environment, local
    /// variables, the starting current values and the host guard.
    pub execute: ExecuteOptions,
    /// Stop at the first failed step: a request that was not sent or got no response, or a
    /// failing test.
    pub bail: bool,
    /// How many requests the run sends at most; `None` is [`DEFAULT_MAX_STEPS`].
    pub max_steps: Option<usize>,
}

/// A request of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunItem {
    /// The request id.
    pub id: String,
    /// The request name.
    pub name: String,
    /// The request slug path, `<collection>/<folder>/.../<request>`.
    pub path: String,
}

/// What a run reports while it runs, in order: `Started` then `Finished` for each step.
#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    /// A request is about to be prepared and sent.
    Started {
        /// The step number, from 0; a request a jump runs again gets a new step.
        step: usize,
        /// The request.
        item: RunItem,
    },
    /// A request finished, with or without a response.
    Finished(StepResult),
}

/// The result of one step of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct StepResult {
    /// The step number, from 0.
    pub step: usize,
    /// The request.
    pub item: RunItem,
    /// The method sent; `None` when the request did not prepare.
    pub method: Option<String>,
    /// The URL sent, secrets masked; `None` when the request did not prepare.
    pub url: Option<String>,
    /// The response status; `None` without a response.
    pub status: Option<u16>,
    /// The response reason phrase, when there is one.
    pub reason: Option<String>,
    /// Milliseconds from sending to the whole response or the failure.
    pub elapsed_ms: u64,
    /// The size of the response body in bytes; 0 without a response.
    pub size: u64,
    /// The test results of the pre-request and test scripts, text masked.
    pub tests: Vec<TestResult>,
    /// Why there is no response, masked: the request did not prepare, the host guard
    /// refused it, the transport failed or the run was cancelled.
    pub error: Option<String>,
}

impl StepResult {
    /// Whether the step failed: no response, or a failing test.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.error.is_some() || self.tests.iter().any(|test| !test.passed)
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEnd {
    /// Every request ran to the end of the run order.
    Completed,
    /// A script called `pm.setNextRequest(null)`.
    Stopped,
    /// A step failed with `bail` set.
    Bailed,
    /// The cancellation token was cancelled.
    Cancelled,
    /// The run sent this many requests, its step limit, and had more to send.
    StepLimit(usize),
    /// A script called `pm.setNextRequest` with a name or id the run does not hold.
    UnknownNext(String),
}

/// What a run did.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    /// The run order, before any jump.
    pub items: Vec<RunItem>,
    /// One result per step, in order.
    pub results: Vec<StepResult>,
    /// How the run ended.
    pub end: RunEnd,
    /// Milliseconds the whole run took.
    pub elapsed_ms: u64,
    /// The current values at the end of the run: the caller's, with the script writes of
    /// the run applied.
    pub current: CurrentValues,
}

impl RunSummary {
    /// How many requests the run ran.
    #[must_use]
    pub fn requests(&self) -> usize {
        self.results.len()
    }

    /// How many steps got no response.
    #[must_use]
    pub fn failed_requests(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.error.is_some())
            .count()
    }

    /// How many tests passed.
    #[must_use]
    pub fn tests_passed(&self) -> usize {
        self.tests().filter(|test| test.passed).count()
    }

    /// How many tests failed.
    #[must_use]
    pub fn tests_failed(&self) -> usize {
        self.tests().filter(|test| !test.passed).count()
    }

    /// Whether the run passed: every step got a response, every test passed, and the run
    /// completed or a script stopped it.
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(self.end, RunEnd::Completed | RunEnd::Stopped)
            && self.results.iter().all(|result| !result.failed())
    }

    fn tests(&self) -> impl Iterator<Item = &TestResult> {
        self.results.iter().flat_map(|result| result.tests.iter())
    }
}

impl Engine {
    /// Whether `selector` (an id or slug path) selects a request, rather than a collection, a
    /// folder or nothing.
    #[must_use]
    pub fn has_request(&self, selector: &str) -> bool {
        crate::select::find(self.workspace(), selector).is_ok()
    }

    /// The run order of `selector`, a collection, folder or request by id or slug path,
    /// without running it.
    ///
    /// # Errors
    ///
    /// [`ExecuteError::UnknownTarget`] when nothing matches.
    pub fn run_plan(&self, selector: &str) -> Result<Vec<RunItem>, ExecuteError> {
        plan::plan(self.workspace(), selector).map(|(_, items)| items)
    }

    /// Runs the collection, folder or request `selector` (by id or slug path), blocking the
    /// calling thread until the run ends; see the [module docs](self). `on_event` hears each
    /// step start and finish; `history`, when given, records every send. Must not be called
    /// from inside an async context: a front end runs it on a background thread and stops
    /// it with `cancel`.
    ///
    /// # Errors
    ///
    /// [`ExecuteError::UnknownTarget`] when nothing matches, and the environment errors of
    /// [`Engine::execute`]; both before anything is sent. A failing step is a
    /// [`StepResult`], never an error of the run.
    pub fn run_collection(
        &self,
        selector: &str,
        options: &RunOptions,
        cancel: &CancellationToken,
        mut history: Option<&mut AppState>,
        on_event: &mut dyn FnMut(&RunEvent),
    ) -> Result<RunSummary, ExecuteError> {
        let (collection, items) = plan::plan(self.workspace(), selector)?;
        select_environment(self.workspace(), collection, &options.execute)?;
        let max_steps = options.max_steps.unwrap_or(DEFAULT_MAX_STEPS);
        let mut execute = options.execute.clone();
        let started = Instant::now();
        let mut results: Vec<StepResult> = Vec::new();
        let mut index = 0;
        let end = loop {
            let Some(item) = items.get(index) else {
                break RunEnd::Completed;
            };
            if cancel.is_cancelled() {
                break RunEnd::Cancelled;
            }
            if results.len() >= max_steps {
                break RunEnd::StepLimit(max_steps);
            }
            let step = results.len();
            on_event(&RunEvent::Started {
                step,
                item: item.clone(),
            });
            let (result, next) =
                self.run_step(step, item, &mut execute, cancel, history.as_deref_mut());
            on_event(&RunEvent::Finished(result.clone()));
            let failed = result.failed();
            results.push(result);
            if cancel.is_cancelled() {
                break RunEnd::Cancelled;
            }
            if failed && options.bail {
                break RunEnd::Bailed;
            }
            match next {
                None => index += 1,
                Some(NextRequest::Stop) => break RunEnd::Stopped,
                Some(NextRequest::Request(target)) => match find_next(&items, &target) {
                    Some(found) => index = found,
                    None => break RunEnd::UnknownNext(target),
                },
            }
        };
        Ok(RunSummary {
            items,
            results,
            end,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            current: execute.current,
        })
    }
}

/// The index of the request `target` names in the run: its id, else the first with that
/// name.
fn find_next(items: &[RunItem], target: &str) -> Option<usize> {
    items
        .iter()
        .position(|item| item.id == target)
        .or_else(|| items.iter().position(|item| item.name == target))
}
