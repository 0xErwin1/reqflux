//! In-app approvals of MCP calls (mcp-approvals S3, S4): the window listens for approval
//! requests from `reqflux mcp`, queues them, counts them in the status bar, the bell and
//! the rail, and lists them in the MCP approvals view ([`super::approvals_view`]), where the
//! user answers them. A new request never opens anything nor takes the focus.
//!
//! The engine's listener hands each request to [`Forward`] on a connection thread, which
//! only passes it down a channel; the shell drains the channel on a timer, so the listener
//! never waits for the UI. The same timer withdraws a request whose client is gone (it
//! timed out or exited).
//!
//! While any request is pending the window's UI automation bridge is suspended (see
//! [`crate::ui_automation::guard_input`]): the bridge offers no way to refuse its input or to
//! mark the events it injects, so the only way an agent cannot click `Allow` on its own call
//! is that no bridge is listening while the question is open. Every input the view
//! receives then comes from the user.

use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, FocusHandle, KeyBinding, Task, Window, actions};
use gpui_component::input::InputState;
use reqflux_engine::approvals::{
    ApprovalDecision, ApprovalError, ApprovalHandler, ApprovalListener, ApprovalRequest, Reply,
};

use super::Shell;
use super::history::now_millis;
use super::sidebar_history::Panel;

/// How often the shell takes new requests and drops withdrawn ones.
pub(crate) const POLL: Duration = Duration::from_millis(100);

/// How many polls pass between two redraws of the open view, so its waiting times move.
const TICK_POLLS: u32 = 10;

/// The key context of the list of the MCP approvals view.
pub(crate) const APPROVALS_CONTEXT: &str = "Approvals";

/// The placeholder of the reason field.
const REASON_PLACEHOLDER: &str = "Reason for a denial (optional, sent to the agent)";

actions!(
    approvals,
    [
        /// Selects the next pending call.
        SelectNextApproval,
        /// Selects the previous pending call.
        SelectPreviousApproval,
        /// Allows the selected call once.
        AllowSelectedApproval,
        /// Denies the selected call.
        DenySelectedApproval,
    ]
);

/// Binds the keys of the approvals list: `j`/`k` and the arrows move, `a` allows once and
/// `r` denies. The reason field is outside the list, so its typing never reaches them.
pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(APPROVALS_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("j", SelectNextApproval, context),
        KeyBinding::new("down", SelectNextApproval, context),
        KeyBinding::new("k", SelectPreviousApproval, context),
        KeyBinding::new("up", SelectPreviousApproval, context),
        KeyBinding::new("a", AllowSelectedApproval, context),
        KeyBinding::new("r", DenySelectedApproval, context),
    ]);
}

/// A request waiting for the user's answer.
pub(crate) struct Pending {
    /// The window's own key of the request: request ids are per client, so two clients
    /// may send the same one.
    pub(crate) key: u64,
    pub(crate) request: ApprovalRequest,
    /// When the request arrived, in milliseconds since the Unix epoch.
    pub(crate) arrived_at: i64,
    reply: Reply,
}

/// The window's approvals: the listener, the channel from it, the queue of pending
/// requests in arrival order (longest wait first), the selected one and the view's focus
/// and reason field.
pub(crate) struct Approvals {
    queue: VecDeque<Pending>,
    /// The key of the selected request; the first one when it names none in the queue.
    selected: Option<u64>,
    next_key: u64,
    receiver: Option<mpsc::Receiver<(ApprovalRequest, Reply)>>,
    /// Held so the listener lives as long as the shell; dropping it stops it.
    _listener: Option<ApprovalListener>,
    /// Whether the automation bridge is suspended for the pending requests.
    guarded: bool,
    /// Polls since the open view was last redrawn.
    ticks: u32,
    /// Focused while the view's list has the keyboard.
    pub(crate) focus: FocusHandle,
    /// The reason field of the view, made when the view first opens.
    pub(crate) reason: Option<Entity<InputState>>,
    _poll: Option<Task<()>>,
}

impl Approvals {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self {
            queue: VecDeque::new(),
            selected: None,
            next_key: 1,
            receiver: None,
            _listener: None,
            guarded: false,
            ticks: 0,
            focus,
            reason: None,
            _poll: None,
        }
    }

    /// The number of pending requests.
    pub(crate) fn pending(&self) -> usize {
        self.queue.len()
    }

    /// The pending requests, longest wait first.
    pub(crate) fn queue(&self) -> impl Iterator<Item = &Pending> {
        self.queue.iter()
    }

    /// The position of the selected request in the queue.
    pub(crate) fn selected_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }
        let found = self
            .selected
            .and_then(|key| self.queue.iter().position(|pending| pending.key == key));
        Some(found.unwrap_or(0))
    }

    /// The selected pending request.
    pub(crate) fn selected_pending(&self) -> Option<&Pending> {
        self.selected_index()
            .and_then(|index| self.queue.get(index))
    }

    /// The selected request.
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<&ApprovalRequest> {
        self.selected_pending().map(|pending| &pending.request)
    }

    /// Selects the request at `index`, when there is one.
    fn select_index(&mut self, index: usize) {
        if let Some(pending) = self.queue.get(index) {
            self.selected = Some(pending.key);
        }
    }

    /// Removes the request at `index`; when it was selected, the one after it (else the
    /// one before) is selected.
    fn remove(&mut self, index: usize) -> Option<Pending> {
        let was_selected = self.selected_index() == Some(index);
        let removed = self.queue.remove(index)?;
        if was_selected {
            self.selected = None;
            let next = index.min(self.queue.len().saturating_sub(1));
            self.select_index(next);
        }
        Some(removed)
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
        self.approvals.receiver = Some(receiver);
        self.approvals._listener = Some(listener);
        self.approvals._poll = Some(poll);
    }

    /// Queues the requests that arrived and drops those whose client is gone. Nothing
    /// opens and the focus stays where it is; only the counts change.
    fn poll_approvals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        let arrived: Vec<_> = self
            .approvals
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();
        for (request, reply) in arrived {
            let key = self.approvals.next_key;
            self.approvals.next_key = key.wrapping_add(1);
            self.approvals.queue.push_back(Pending {
                key,
                request,
                arrived_at: now_millis(),
                reply,
            });
            changed = true;
        }
        let mut index = self.approvals.queue.len();
        while index > 0 {
            index -= 1;
            let closed = self
                .approvals
                .queue
                .get(index)
                .is_some_and(|pending| pending.reply.is_closed());
            if closed && self.approvals.remove(index).is_some() {
                changed = true;
            }
        }
        if changed {
            self.update_input_guard(window, cx);
            cx.notify();
        } else if self.approvals_open(cx) && self.approvals.pending() > 0 {
            self.approvals.ticks += 1;
            if self.approvals.ticks >= TICK_POLLS {
                self.approvals.ticks = 0;
                cx.notify();
            }
        }
    }

    /// Whether the MCP approvals view is on screen.
    pub(super) fn approvals_open(&self, cx: &App) -> bool {
        self.sidebar.read(cx).panel == Panel::Approvals
    }

    /// Opens the MCP approvals view and gives its list the keyboard.
    pub(super) fn open_approvals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.approvals.reason.is_none() {
            let reason = cx.new(|cx| InputState::new(window, cx).placeholder(REASON_PLACEHOLDER));
            self.approvals.reason = Some(reason);
        }
        self.show_panel(Panel::Approvals, cx);
        self.approvals.focus.focus(window, cx);
    }

    /// Selects the pending request at `index` and gives the list the keyboard.
    pub(super) fn select_approval(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.approvals.select_index(index);
        self.approvals.focus.focus(window, cx);
        cx.notify();
    }

    /// Moves the selection one request down, or up, staying in the list.
    pub(super) fn step_approval(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(index) = self.approvals.selected_index() else {
            return;
        };
        let next = if down {
            (index + 1).min(self.approvals.pending().saturating_sub(1))
        } else {
            index.saturating_sub(1)
        };
        self.approvals.select_index(next);
        cx.notify();
    }

    /// Answers the selected request with `decision`, a denial with the reason field's text
    /// when it has any, and selects the next one. The reason field empties.
    pub(super) fn answer_approval(
        &mut self,
        decision: ApprovalDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.approvals.selected_index() else {
            return;
        };
        let Some(pending) = self.approvals.remove(index) else {
            return;
        };
        let reason = self.take_reason(window, cx);
        let reason = match decision {
            ApprovalDecision::Deny => reason,
            ApprovalDecision::AllowOnce | ApprovalDecision::AllowSession => None,
        };
        if !pending.reply.answer(decision, reason) {
            log::info!(
                "the agent stopped waiting for the approval of {}",
                pending.request.tool
            );
        }
        if self.approvals_open(cx) {
            self.approvals.focus.focus(window, cx);
        }
        self.update_input_guard(window, cx);
        cx.notify();
    }

    /// The reason field's text, trimmed, `None` when blank; the field empties.
    fn take_reason(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<String> {
        let input = self.approvals.reason.as_ref()?;
        let text = input.read(cx).value().trim().to_owned();
        input.update(cx, |state, cx| state.set_value("", window, cx));
        (!text.is_empty()).then_some(text)
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
