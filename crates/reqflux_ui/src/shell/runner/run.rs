//! Running: the engine's `run_collection` on a thread of its own, its events polled into the
//! run document, Stop through the cancellation token, and what the end of a run persists.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use gpui::{Context, Task};
use reqflux_engine::state::AppState;
use reqflux_engine::{
    CancellationToken, CurrentValues, ExecuteOptions, RunEvent, RunOptions, RunSummary,
};

use super::model::{Phase, changes};
use crate::shell::history::{completed_of, now_millis};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

/// How often the document takes the run's events and redraws its time.
const POLL: Duration = Duration::from_millis(50);

/// What the run's thread sends the document.
enum Message {
    Event(RunEvent),
    Done(Result<RunSummary, String>),
}

/// A run in flight. Dropping it cancels the run and stops polling it.
pub(super) struct Running {
    cancel: CancellationToken,
    started: Instant,
    /// When the run started, in milliseconds since the Unix epoch: a recorded send older
    /// than this is not one of the run's.
    started_at: i64,
    /// The current values the run started with, which its changes are measured against.
    current: CurrentValues,
    _poll: Task<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Shell {
    /// Runs the open run document with its setup, replacing what the last run reported.
    /// Does nothing while a run is in flight or without a loaded workspace.
    pub(in crate::shell) fn start_run(&mut self, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let engine = Arc::clone(engine);
        let state_path = self.state.as_ref().map(|state| state.path().to_path_buf());
        let current = self.current.clone();
        let Some(run) = self.run.as_mut() else {
            return;
        };
        if run.is_running() {
            return;
        }
        for key in run.responses.drain().map(|(_, key)| key) {
            self.sends.remove(&key);
        }
        run.model.start();
        run.environment_menu = false;
        let options = RunOptions {
            execute: ExecuteOptions {
                environment: run.model.environment.clone(),
                current: current.clone(),
                ..ExecuteOptions::default()
            },
            bail: run.model.bail,
            max_steps: None,
        };
        let target = run.model.target.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("reqflux-run".to_owned())
            .spawn(move || {
                // The window keeps its own connection; this one records the run's sends.
                let mut history = state_path.and_then(|path| match AppState::open(&path) {
                    Ok(state) => Some(state),
                    Err(error) => {
                        log::warn!("the run records no history: {error}");
                        None
                    }
                });
                let events = sender.clone();
                let outcome = engine.run_collection(
                    &target,
                    &options,
                    &token,
                    history.as_mut(),
                    &mut |event| {
                        if events.send(Message::Event(event.clone())).is_err() {
                            log::debug!("the run document closed during the run");
                        }
                    },
                );
                let done = Message::Done(outcome.map_err(|error| error.to_string()));
                if sender.send(done).is_err() {
                    log::debug!("the run document closed before the run ended");
                }
            });
        if let Err(error) = spawned {
            run.model
                .finish(Err(format!("cannot start the run: {error}")));
            cx.notify();
            return;
        }
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                match this.update(cx, |shell, cx| shell.take_run_messages(&receiver, cx)) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
        });
        run.running = Some(Running {
            cancel,
            started: Instant::now(),
            started_at: now_millis(),
            current,
            _poll: poll,
        });
        cx.notify();
    }

    /// Stop: cancels the run in flight. The send in flight fails as cancelled and the run
    /// ends as cancelled once the engine returns.
    pub(in crate::shell) fn stop_run(&mut self, cx: &mut Context<Self>) {
        if let Some(running) = self.run.as_ref().and_then(|run| run.running.as_ref()) {
            running.cancel.cancel();
            cx.notify();
        }
    }

    /// Applies every message the run's thread sent since the last poll; `false` once the
    /// run ended or its document closed, which stops the polling.
    fn take_run_messages(&mut self, receiver: &Receiver<Message>, cx: &mut Context<Self>) -> bool {
        loop {
            let Some(run) = self.run.as_mut() else {
                return false;
            };
            match receiver.try_recv() {
                Ok(Message::Event(event)) => run.model.apply(event),
                Ok(Message::Done(outcome)) => {
                    self.end_run(outcome, cx);
                    return false;
                }
                Err(TryRecvError::Empty) => {
                    if let Some(running) = &run.running {
                        run.model.elapsed = running.started.elapsed();
                    }
                    cx.notify();
                    return true;
                }
                Err(TryRecvError::Disconnected) => {
                    run.model
                        .finish(Err("the run ended without a result".to_owned()));
                    run.running = None;
                    cx.notify();
                    return false;
                }
            }
        }
    }

    /// Ends the run: shows its outcome, persists the current values its scripts changed,
    /// reads history again and keeps each step's recorded response for its result.
    fn end_run(&mut self, outcome: Result<RunSummary, String>, cx: &mut Context<Self>) {
        let Some(run) = self.run.as_mut() else {
            return;
        };
        let running = run.running.take();
        run.model.finish(outcome.as_ref().map_err(Clone::clone));
        if let (Ok(summary), Some(running)) = (&outcome, &running) {
            self.persist_run_values(&running.current, &summary.current, cx);
            self.keep_run_responses(running.started_at);
        }
        self.split.forget_recorded();
        self.refresh_history(cx);
        cx.notify();
    }

    /// Persists the current values the run changed, from `started` to `ended`, in the app
    /// state and the window's copy, then inspects the open requests again. A failure shows
    /// in an error toast without values.
    fn persist_run_values(
        &mut self,
        started: &CurrentValues,
        ended: &CurrentValues,
        cx: &mut Context<Self>,
    ) {
        let changed = changes(started, ended);
        if changed.is_empty() {
            return;
        }
        let mut failure = None;
        for change in changed {
            if let (Some(state), Some(workspace_id)) =
                (self.state.as_mut(), self.workspace_id.as_deref())
            {
                let written = match &change.value {
                    Some(value) => {
                        state.set_current_value(workspace_id, change.scope, &change.key, value)
                    }
                    None => state
                        .clear_current_value(workspace_id, change.scope, &change.key)
                        .map(|_| ()),
                };
                if let Err(error) = written {
                    log::warn!("cannot persist the run's current value {}", change.key);
                    failure.get_or_insert(error.to_string());
                    continue;
                }
            }
            match change.value {
                Some(value) => self.current.insert(change.scope, change.key, value),
                None => {
                    self.current.remove(change.scope, &change.key);
                }
            }
        }
        if let Some(error) = failure {
            toasts::failure("Cannot remember the values the scripts set", error, cx);
        }
        self.inspect_open_requests(cx);
    }

    /// Keeps the recorded response of each finished step in the shell's sends, read from
    /// history: the last send of its request, when the run recorded it after `started_at`.
    fn keep_run_responses(&mut self, started_at: i64) {
        let (Some(state), Some(workspace_id), Some(run)) = (
            self.state.as_ref(),
            self.workspace_id.as_deref(),
            self.run.as_mut(),
        ) else {
            return;
        };
        if run.model.phase != Phase::Finished {
            return;
        }
        let mut keys = std::collections::HashMap::new();
        for result in &run.model.results {
            let id = &result.item.id;
            if !keys.contains_key(id) {
                let key = match state.last_history_of(workspace_id, id) {
                    Ok(Some(entry)) if entry.record.sent_at >= started_at => {
                        let key = format!("history-run-{}", entry.id);
                        self.sends
                            .insert(key.clone(), RequestSend::recorded(completed_of(&entry)));
                        Some(key)
                    }
                    Ok(_) => None,
                    Err(error) => {
                        log::warn!("cannot read the recorded send of {id}: {error}");
                        None
                    }
                };
                keys.insert(id.clone(), key);
            }
            if let Some(Some(key)) = keys.get(id) {
                run.responses.insert(result.step, key.clone());
            }
        }
    }
}
