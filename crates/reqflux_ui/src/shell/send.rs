//! Sending the active request from the shell: Send and Cancel, their keys, and the tasks
//! that prepare and await each send off the UI thread. See the engine's threading model.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Context, Task, Window};
use reqflux_core::model::Request;
use reqflux_engine::state::{HistoryOutcome, RecordedResponse};
use reqflux_engine::{
    CancellationToken, ExecuteOptions, Executed, Execution, HttpError, ScriptOutcome,
};

use super::Shell;
use super::history::{Pending, Reopened};
use super::last_request::{LastSend, path_of};
use super::request::{Completed, Failure, Received, SendState};
use super::response::{BodyView, Preview, ResponseTab};
use crate::workspace::OpenedWorkspace;

/// The key context of the main window, where the command keys are bound.
pub(crate) const SHELL_CONTEXT: &str = "Shell";

/// How often the elapsed time of a send in flight is redrawn.
const TICK: Duration = Duration::from_millis(100);

/// The send of one open request: its state, the response sub-tab and body view, and, while
/// in flight, what keeps it running.
#[derive(Default)]
pub(crate) struct RequestSend {
    pub(crate) state: SendState,
    pub(crate) tab: ResponseTab,
    pub(crate) view: BodyView,
    /// The history entry the response shows, while it shows a reopened one; a new send
    /// clears it.
    pub(crate) history: Option<Reopened>,
    /// The preview of the last finished response, once computed; a new response clears it.
    pub(crate) preview: Option<Arc<Preview>>,
    /// The task computing `preview` off the UI thread.
    pub(crate) preview_task: Option<Task<()>>,
    /// Why the last `Open in browser` could not write its file.
    pub(crate) preview_error: Option<String>,
    running: Option<Running>,
    /// The task preparing the current send: it runs the engine's `execute` on the
    /// background executor and hands the execution to `running`. Held so it is not
    /// dropped, which would cancel the hand-off; the next send of this request replaces
    /// it, cancelling a prepare that has not finished yet.
    preparing: Option<Task<()>>,
}

/// A send in flight. Dropping it cancels the request and drops the tasks awaiting it.
struct Running {
    cancel: CancellationToken,
    _wait: Task<()>,
    _tick: Task<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl RequestSend {
    /// The cancellation token of the send in flight, once its execution started.
    #[cfg(test)]
    pub(crate) fn cancel_token(&self) -> Option<CancellationToken> {
        self.running.as_ref().map(|running| running.cancel.clone())
    }

    /// A send showing `completed`, reopened from history as `reopened`.
    pub(crate) fn reopened(completed: Completed, reopened: Reopened) -> Self {
        Self {
            state: SendState::Done(completed),
            history: Some(reopened),
            ..Self::default()
        }
    }
}

impl Shell {
    /// Sends the active request.
    pub(super) fn send_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.documents.active().map(str::to_owned) {
            self.send(&id, window, cx);
        }
    }

    /// Escape: closes the command
    /// palette, or the workspace, environment, auth type, body
    /// mode or method menu when one is open,
    /// else the sidebar's row menu or delete confirmation (clearing its error), else cancels
    /// the send in flight of the active request. The binding takes Escape before the menu's
    /// own key handler.
    pub(super) fn cancel_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
        } else if self.workspace_menu_open {
            if !self.cancel_new_workspace(window, cx) {
                self.close_workspace_menu(window, cx);
            }
        } else if self.environment_menu_open {
            self.close_environment_menu(window, cx);
        } else if self.auth_menu_open {
            self.close_auth_menu(window, cx);
        } else if self.body_menu_open {
            self.close_body_menu(window, cx);
        } else if self.method_menu_open {
            self.close_method_menu(window, cx);
        } else if self.dismiss_settings(window, cx)
            || self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.dismiss(window, cx))
        {
        } else if let Some(id) = self.documents.active().map(str::to_owned) {
            self.cancel(&id, cx);
        }
    }

    /// Ctrl+S: saves the draft of the active request.
    pub(super) fn save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.documents.active().map(str::to_owned) {
            self.save(&id, window, cx);
        }
    }

    /// Sends the request `id` with the active environment, cancelling its send in flight.
    /// Preparing runs on GPUI's background executor — it runs the effective pre-request
    /// script, which can take seconds — so the window never waits for a script (S14). The
    /// send shows as in flight at once; a request the engine cannot prepare fails when the
    /// prepare task returns. A send that completes or fails, not one cancelled, is recorded
    /// in history.
    pub(super) fn send(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.send_document(id, None, window, cx);
    }

    /// [`Shell::send`] of `document` in place of the request's draft or file when given:
    /// Resend sends a recorded document this way, leaving the draft and the file alone.
    pub(super) fn send_document(
        &mut self,
        id: &str,
        document: Option<Request>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The cell field shows text the draft does not have yet: commit it, so a send never
        // silently drops what was typed.
        self.commit_cell_edit(window, cx);
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let engine = Arc::clone(engine);
        if let Some(send) = self.sends.get_mut(id) {
            send.running = None;
        }
        let options = ExecuteOptions {
            environment: self.active_environment.clone(),
            current: self.current.clone(),
            ..ExecuteOptions::default()
        };
        // Kept for the completion, which persists the script writes against the
        // environment this send used.
        let persist = options.clone();
        // A request with a draft sends the draft, not the file on disk.
        let drafted = document.or_else(|| self.drafts.get(id).map(|draft| draft.request.clone()));
        let pending = self.pending_record(id, drafted.as_ref());
        // Until the execution reports its own method and URL, the request view labels the
        // status bar and the send in flight.
        let (method, url) = self
            .requests
            .get(id)
            .map_or_else(Default::default, |state| {
                let request = &state.inspection.request.request;
                (
                    request.method.as_str().to_owned(),
                    state.inspection.url.clone(),
                )
            });
        self.last_send = Some(LastSend {
            id: id.to_owned(),
            method,
            path: path_of(&url),
        });
        let started = Instant::now();
        let send = self.sends.entry(id.to_owned()).or_default();
        send.running = None;
        send.history = None;
        send.state.start(started);
        let key = id.to_owned();
        let selector = key.clone();
        // Preparing runs the effective pre-request script, which can take seconds: it runs
        // on GPUI's background executor, never on the UI thread (S14).
        let prepare = cx.background_executor().spawn(async move {
            match drafted {
                Some(draft) => engine.execute_draft(&selector, &draft, &options),
                None => engine.execute(&selector, &options),
            }
        });
        send.preparing = Some(cx.spawn(async move |this, cx| {
            match prepare.await {
                Ok(execution) => {
                    let (method, path) =
                        (execution.method.clone(), path_of(&execution.url));
                    if let Err(error) = this.update(cx, |shell, cx| {
                        let running = spawn(&key, execution, started, pending, persist, cx);
                        match shell.sends.get_mut(&key) {
                            Some(send) => {
                                // The status bar names what was actually sent, unless a
                                // newer send replaced this one meanwhile.
                                if matches!(&send.state, SendState::Sending { started: at, .. } if *at == started)
                                {
                                    shell.last_send = Some(LastSend {
                                        id: key.clone(),
                                        method,
                                        path,
                                    });
                                }
                                send.running = Some(running);
                            }
                            // The tab closed while the execution prepared; dropping
                            // `running` cancels it.
                            None => drop(running),
                        }
                        cx.notify();
                    }) {
                        log::debug!("send of {key} ended after its shell: {error}");
                    }
                }
                Err(error) => {
                    let completed = Completed::Failure(Failure {
                        error: error.to_string(),
                        elapsed: Duration::ZERO,
                        unresolved: Vec::new(),
                        // The prepare failed before any script could run.
                        scripts: ScriptOutcome::default(),
                    });
                    if let Err(error) =
                        this.update(cx, |shell, cx| shell.finish_send(&key, completed, cx))
                    {
                        log::debug!("send of {key} failed after its shell: {error}");
                    }
                }
            }
        }));
        cx.notify();
    }

    /// Cancels the send in flight of the request `id`; it then fails with
    /// `request cancelled`.
    pub(super) fn cancel(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(running) = self.sends.get(id).and_then(|send| send.running.as_ref()) {
            running.cancel.cancel();
            cx.notify();
        }
    }

    /// Selects the response sub-tab `tab` of the request `id`.
    pub(super) fn select_response_tab(
        &mut self,
        id: &str,
        tab: ResponseTab,
        cx: &mut Context<Self>,
    ) {
        if let Some(send) = self.sends.get_mut(id) {
            send.tab = tab;
            cx.notify();
        }
    }

    /// Shows the response body of the request `id` as `view`.
    pub(super) fn select_body_view(&mut self, id: &str, view: BodyView, cx: &mut Context<Self>) {
        if let Some(send) = self.sends.get_mut(id) {
            send.view = view;
            cx.notify();
        }
        if view == BodyView::Preview {
            self.ensure_preview(id, cx);
        }
    }

    fn finish_send(&mut self, id: &str, completed: Completed, cx: &mut Context<Self>) {
        if let Some(send) = self.sends.get_mut(id) {
            send.state.finish(completed);
            send.running = None;
            send.preview = None;
            send.preview_task = None;
            send.preview_error = None;
            let previewing = send.view == BodyView::Preview;
            cx.notify();
            if previewing {
                self.ensure_preview(id, cx);
            }
        }
    }
}

/// Awaits `execution` on GPUI's executor and redraws every [`TICK`] until it ends, then
/// records it in history with `pending` unless it was cancelled. A completed send persists
/// its script writes as current values against `options` (variables-editor S5).
fn spawn(
    id: &str,
    execution: Execution,
    started: Instant,
    pending: Option<Pending>,
    options: ExecuteOptions,
    cx: &mut Context<Shell>,
) -> Running {
    let cancel = execution.cancel.clone();
    let unresolved = execution.unresolved.clone();
    // The pre-request has already run when the transport fails; its results live on the
    // execution, so keep them to show beside the error banner.
    let scripts = execution.scripts.clone();
    // The redacted request as sent, kept for a failure, which consumes the execution.
    let sent = execution.sent.clone();
    // A transport error can echo the URL with a query secret, and a response can echo a
    // secret in its headers or body: history keeps them masked.
    let masker = execution.masker();
    let key = id.to_owned();
    let wait = cx.spawn(async move |this, cx| {
        let (completed, recorded, writes) = match execution.finish().await {
            Ok(executed) => {
                let Executed {
                    response,
                    unresolved,
                    scripts,
                    sent,
                    ..
                } = executed;
                let writes = scripts.writes.clone();
                let recorded = (
                    sent,
                    HistoryOutcome::Response(RecordedResponse::from_response_masked(
                        &response, &masker,
                    )),
                );
                // Splitting a large body into lines stays off the UI thread.
                let received = cx
                    .background_executor()
                    .spawn(async move { Received::new(response, unresolved, scripts) })
                    .await;
                (Completed::Response(received), Some(recorded), writes)
            }
            Err(error) => {
                let elapsed = started.elapsed();
                // A cancelled send is not recorded.
                let recorded = (!matches!(error, HttpError::Cancelled)).then(|| {
                    let outcome = HistoryOutcome::Failed {
                        error: masker.mask(&error.to_string()),
                        elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                    };
                    (sent, outcome)
                });
                let failure = Failure {
                    error: error.to_string(),
                    elapsed,
                    unresolved,
                    scripts,
                };
                (Completed::Failure(failure), recorded, Vec::new())
            }
        };
        if let Err(error) = this.update(cx, |shell, cx| {
            shell.persist_script_writes(&key, &options, &writes, cx);
            if let (Some(pending), Some((sent, outcome))) = (pending, recorded) {
                shell.record_send(pending, sent, outcome, cx);
            }
            shell.finish_send(&key, completed, cx);
        }) {
            log::debug!("send of {key} ended after its shell: {error}");
        }
    });
    let tick = cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    });
    Running {
        cancel,
        _wait: wait,
        _tick: tick,
    }
}
