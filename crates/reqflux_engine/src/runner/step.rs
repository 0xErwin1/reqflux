//! One step of a collection run: prepare, send, carry the script writes, record history.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use reqflux_http::HttpError;
use reqflux_script::{NextRequest, ScriptOutcome, TestResult};
use tokio_util::sync::CancellationToken;

use super::{RunItem, StepResult};
use crate::current::{CurrentValues, write_scope};
use crate::engine::{Engine, Executed};
use crate::error::ExecuteError;
use crate::prepare::{ExecuteOptions, select_environment};
use crate::sent::{Masker, SentRequest};
use crate::state::{AppState, HistoryOutcome, HistoryRecord, RecordedResponse};

impl Engine {
    /// Runs the request `item` with `options`, cancelled with the run's `cancel`, and returns
    /// its result and where the run goes next. The script writes are applied to
    /// `options.current` for the next steps.
    pub(super) fn run_step(
        &self,
        step: usize,
        item: &RunItem,
        options: &mut ExecuteOptions,
        cancel: &CancellationToken,
        history: Option<&mut AppState>,
    ) -> (StepResult, Option<NextRequest>) {
        let mut result = StepResult {
            step,
            item: item.clone(),
            method: None,
            url: None,
            status: None,
            reason: None,
            elapsed_ms: 0,
            size: 0,
            timings: None,
            tests: Vec::new(),
            error: None,
            unresolved: Vec::new(),
        };
        let sent_at = now_millis();
        let pending = match self.prepare_execution(&item.id, options) {
            Ok(pending) => pending,
            Err(error) => {
                result.error = Some(error.to_string());
                return (result, None);
            }
        };
        let masker = pending.masker();
        if let Some(guard) = &options.host_guard {
            let request = pending.request();
            if let Err(refusal) = guard.check(request.method.as_str(), &request.url) {
                result.method = Some(request.method.clone());
                result.url = Some(masker.mask(&request.url));
                result.error = Some(masker.mask(&ExecuteError::HostRefused(refusal).to_string()));
                return (result, None);
            }
        }
        let execution = self.start(pending);
        let sent = execution.sent.clone();
        result.method = Some(sent.method.clone());
        result.url = Some(sent.url.clone());
        result.unresolved.clone_from(&execution.unresolved);
        let pre_request = execution.scripts.clone();
        let step_cancel = execution.cancel.clone();
        let (run, request) = (cancel.clone(), step_cancel.clone());
        // Ends with the send: the step token is cancelled once the send is over.
        drop(self.runtime.spawn(async move {
            tokio::select! {
                () = run.cancelled() => request.cancel(),
                () = request.cancelled() => {}
            }
        }));
        let started = Instant::now();
        let outcome = self.wait(execution);
        step_cancel.cancel();
        result.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let (scripts, recorded) = match outcome {
            Ok(executed) => {
                let Executed {
                    response, scripts, ..
                } = executed;
                result.status = Some(response.status);
                result.reason = response.reason.as_deref().map(|reason| masker.mask(reason));
                result.elapsed_ms = u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX);
                result.size = u64::try_from(response.size()).unwrap_or(u64::MAX);
                result.timings = Some(response.timings.clone());
                let recorded = HistoryOutcome::Response(RecordedResponse::from_response_masked(
                    &response, &masker,
                ));
                (scripts, Some(recorded))
            }
            Err(error) => {
                let cancelled = matches!(error, ExecuteError::Http(HttpError::Cancelled));
                let text = masker.mask(&error.to_string());
                result.error = Some(text.clone());
                // A cancelled send is not recorded, as in the window.
                let recorded = (!cancelled).then_some(HistoryOutcome::Failed {
                    error: text,
                    elapsed_ms: result.elapsed_ms,
                });
                (pre_request, recorded)
            }
        };
        result.tests = scripts
            .tests
            .iter()
            .map(|test| masked_test(test, &masker))
            .collect();
        self.carry(item, options, &scripts);
        if let (Some(state), Some(outcome)) = (history, recorded) {
            self.record(state, item, sent_at, sent, outcome);
        }
        (result, scripts.next_request)
    }

    /// Applies the writes of `scripts` to the current values the next steps resolve with.
    fn carry(&self, item: &RunItem, options: &mut ExecuteOptions, scripts: &ScriptOutcome) {
        let Ok(target) = crate::select::find(self.workspace(), &item.id) else {
            return;
        };
        let Ok(environment) = select_environment(self.workspace(), target.collection, options)
        else {
            return;
        };
        let mut current: CurrentValues = std::mem::take(&mut options.current);
        for write in &scripts.writes {
            match write_scope(self.workspace(), &target, environment, write) {
                Some((_, true)) | None => {}
                Some((scope, false)) => match &write.value {
                    Some(value) => current.insert(scope, write.key.clone(), value.clone()),
                    None => {
                        current.remove(scope, &write.key);
                    }
                },
            }
        }
        options.current = current;
    }

    /// Records one send of the run in history; a failure is logged and never fails the run.
    fn record(
        &self,
        state: &mut AppState,
        item: &RunItem,
        sent_at: i64,
        sent: SentRequest,
        outcome: HistoryOutcome,
    ) {
        let document = match crate::select::find(self.workspace(), &item.id)
            .map(|target| crate::history_document(&target.request.request))
        {
            Ok(Ok(document)) => document,
            Ok(Err(error)) => {
                log::warn!("cannot serialize request {} for history: {error}", item.id);
                return;
            }
            Err(error) => {
                log::warn!("cannot record request {} in history: {error}", item.id);
                return;
            }
        };
        let record = HistoryRecord {
            sent_at,
            request_id: item.id.clone(),
            request_name: item.name.clone(),
            request_path: item.path.clone(),
            document,
            sent,
            outcome,
        };
        let workspace_id = self.workspace().manifest.id.to_string();
        if let Err(error) = state.record_history(&workspace_id, record) {
            log::warn!("cannot record the send of {} in history: {error}", item.id);
        }
    }
}

/// `test` with its name and message masked.
fn masked_test(test: &TestResult, masker: &Masker) -> TestResult {
    TestResult {
        name: masker.mask(&test.name),
        passed: test.passed,
        message: test.message.as_deref().map(|message| masker.mask(message)),
    }
}

/// Milliseconds since the Unix epoch.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}
