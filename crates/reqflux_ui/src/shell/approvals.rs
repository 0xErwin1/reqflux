//! In-app approvals of MCP calls (mcp-approvals S3, S4): the window listens for approval
//! requests from `reqflux mcp`, queues them and shows the first as a modal dialog.
//!
//! The engine's listener hands each request to [`Forward`] on a connection thread, which
//! only passes it down a channel; the shell drains the channel on a timer, so the listener
//! never waits for the UI. The same timer withdraws a request whose client is gone (it
//! timed out or exited).
//!
//! While any request is pending the window's UI automation bridge is suspended (see
//! [`crate::ui_automation::guard_input`]): the bridge offers no way to refuse its input or to
//! mark the events it injects, so the only way an agent cannot click `Allow` on its own call
//! is that no bridge is listening while the question is open. Every input the dialog
//! receives then comes from the user.

use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gpui::{Context, Task, Window};
use reqflux_engine::approvals::{
    ApprovalDecision, ApprovalError, ApprovalHandler, ApprovalListener, ApprovalRequest, Reply,
};

use super::Shell;

/// How often the shell takes new requests and drops withdrawn ones.
pub(crate) const POLL: Duration = Duration::from_millis(100);

/// A request waiting for the user's answer.
pub(crate) struct Pending {
    pub(crate) request: ApprovalRequest,
    reply: Reply,
}

/// The window's approvals: the listener, the channel from it and the queue of pending
/// requests, the one shown first.
#[derive(Default)]
pub(crate) struct Approvals {
    queue: VecDeque<Pending>,
    receiver: Option<mpsc::Receiver<(ApprovalRequest, Reply)>>,
    /// Held so the listener lives as long as the shell; dropping it stops it.
    _listener: Option<ApprovalListener>,
    /// Whether the automation bridge is suspended for the pending requests.
    guarded: bool,
    _poll: Option<Task<()>>,
}

impl Approvals {
    /// The number of pending requests.
    pub(crate) fn pending(&self) -> usize {
        self.queue.len()
    }

    /// The request the dialog shows.
    pub(crate) fn shown(&self) -> Option<&ApprovalRequest> {
        self.queue.front().map(|pending| &pending.request)
    }

    /// The socket the window listens on, while it listens.
    #[cfg(test)]
    pub(crate) fn socket(&self) -> Option<&std::path::Path> {
        self._listener.as_ref().map(ApprovalListener::socket)
    }
}

/// The handler the engine's listener calls: forwards each request to the shell.
struct Forward(mpsc::Sender<(ApprovalRequest, Reply)>);

impl ApprovalHandler for Forward {
    fn request(&self, request: ApprovalRequest, reply: Reply) {
        // A closed channel drops the reply, which the client takes as no answer.
        if self.0.send((request, reply)).is_err() {
            log::debug!("an approval request arrived after the window stopped listening");
        }
    }
}

impl Shell {
    /// Starts listening for approvals with `start` (the engine's `serve` in the app, a
    /// temporary directory in tests) for as long as the shell lives. A listener that cannot
    /// start leaves the window without approvals: `reqflux mcp` then refuses `ask` calls.
    pub(crate) fn listen_for_approvals(
        &mut self,
        start: impl FnOnce(Arc<dyn ApprovalHandler>) -> Result<ApprovalListener, ApprovalError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (sender, receiver) = mpsc::channel();
        let listener = match start(Arc::new(Forward(sender))) {
            Ok(listener) => listener,
            Err(error) => {
                log::info!("no in-app approvals: {error}");
                return;
            }
        };
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this
                    .update_in(cx, |this, window, cx| this.poll_approvals(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.approvals = Approvals {
            queue: VecDeque::new(),
            receiver: Some(receiver),
            _listener: Some(listener),
            guarded: false,
            _poll: Some(poll),
        };
    }

    /// Queues the requests that arrived and drops those whose client is gone.
    fn poll_approvals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.approvals.queue.len();
        let mut arrived = false;
        if let Some(receiver) = &self.approvals.receiver {
            for (request, reply) in receiver.try_iter() {
                self.approvals.queue.push_back(Pending { request, reply });
                arrived = true;
            }
        }
        self.approvals
            .queue
            .retain(|pending| !pending.reply.is_closed());
        if arrived || self.approvals.queue.len() != before {
            self.update_input_guard(window, cx);
            cx.notify();
        }
    }

    /// Answers the request the dialog shows with `decision` and shows the next one.
    pub(super) fn answer_approval(
        &mut self,
        decision: ApprovalDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.approvals.queue.pop_front() else {
            return;
        };
        if !pending.reply.send(decision) {
            log::info!(
                "the agent stopped waiting for the approval of {}",
                pending.request.tool
            );
        }
        self.update_input_guard(window, cx);
        cx.notify();
    }

    /// Suspends the automation bridge while a request is pending and brings it back when
    /// none is.
    fn update_input_guard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let guarded = !self.approvals.queue.is_empty();
        if guarded != self.approvals.guarded {
            self.approvals.guarded = guarded;
            crate::ui_automation::guard_input(window, cx, guarded);
        }
    }
}
