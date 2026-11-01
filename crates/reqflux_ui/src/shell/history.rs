//! Send history in the window (history S6-S8): recording every completed or failed send in
//! the app state, the History panel rows, the history tab and Resend
//! (design/history/01-sidebar-readonly-tab).
//!
//! Recording and reading go through `reqflux_engine::state`; the window never writes the
//! table itself. Opening an entry never touches the request or its draft: it opens the one
//! read-only history tab (see [`HistoryTab`]) with the recorded request and response. Resend
//! sends the recorded document again in the request's own tab; it is disabled when the
//! request no longer exists, because the engine prepares a document only in the place of a
//! stored request.

mod clock;
mod view;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, Window};
use reqflux_core::model::{Item, Request, Workspace};
use reqflux_engine::state::{HistoryEntry, HistoryOutcome, HistoryRecord, RecordedResponse};
use reqflux_engine::{Response, ScriptOutcome, SentRequest};

use super::Shell;
use super::request::{Completed, Failure, Received};
use super::send::RequestSend;
use super::sidebar_history::{HistoryRow, Panel};

pub(super) use view::{render as render_tab, tab_chip};

/// How many entries the History panel lists.
const PANEL_LIMIT: usize = 200;

/// The note beside a disabled Resend.
pub(crate) const RESEND_NOTE: &str = "Resend needs the request to exist";

/// What a send records besides its outcome, captured when it starts.
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub(crate) sent_at: i64,
    pub(crate) request_id: String,
    pub(crate) request_name: String,
    pub(crate) request_path: String,
    /// The request document sent, as TOML.
    pub(crate) document: String,
}

/// The recorded response of the history tab, as its send carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reopened {
    /// The request document that was sent, as TOML; what Resend sends again.
    pub(crate) document: String,
    /// Whether Resend can run: the request still exists.
    pub(crate) resendable: bool,
}

/// The Params, Headers and Body views of the recorded request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RecordedTab {
    Params,
    #[default]
    Headers,
    Body,
}

/// The history tab: one recorded send, read only, in a document tab of its own beside the
/// request tabs. Activating a request tab leaves it in the row; its close button drops it.
/// Its response lives in the shell's sends under [`HistoryTab::key`], so the response
/// sub-tabs work as usual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryTab {
    /// The id of the history entry.
    pub(crate) entry: i64,
    pub(crate) request_id: String,
    /// The request's name when it was sent.
    pub(crate) name: String,
    /// The request as sent, secrets masked.
    pub(crate) sent: SentRequest,
    /// The local `HH:MM` the tab shows.
    pub(crate) time: String,
    /// The local day and `HH:MM:SS` the banner shows.
    pub(crate) moment: String,
    /// Whether the request still exists, so Open source request and Resend can run.
    pub(crate) source_exists: bool,
    /// The selected view of the recorded request.
    pub(crate) tab: RecordedTab,
    /// Whether the tab is the active document.
    pub(crate) active: bool,
}

impl HistoryTab {
    /// The key of its send in the shell's sends, apart from every request id.
    pub(crate) fn key(&self) -> String {
        format!("history-{}", self.entry)
    }
}

/// Milliseconds since the Unix epoch now.
pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The History panel row of `entry` seen at `now`, in the local time zone.
fn row_of(entry: &HistoryEntry, now: i64, deleted: bool, selected: bool) -> HistoryRow {
    let record = &entry.record;
    let (day, time) = clock::day_and_time(record.sent_at, now, clock::local_offset(now));
    HistoryRow {
        entry: entry.id,
        method: record.sent.method.clone(),
        name: record.request_name.clone(),
        url: record.sent.url.clone(),
        status: match &record.outcome {
            HistoryOutcome::Response(response) => Some(response.status),
            HistoryOutcome::Failed { .. } => None,
        },
        day,
        time,
        deleted,
        selected,
    }
}

/// The slug path (`collection/folder/request`) and the saved document of the request `id`.
pub(crate) fn find_request<'a>(
    workspace: &'a Workspace,
    id: &str,
) -> Option<(String, &'a Request)> {
    fn walk<'a>(items: &'a [Item], id: &str, path: &str) -> Option<(String, &'a Request)> {
        items.iter().find_map(|item| match item {
            Item::Request(node) => (node.request.id.to_string() == id)
                .then(|| (format!("{path}/{}", node.slug), &node.request)),
            Item::Folder(folder) => walk(&folder.items, id, &format!("{path}/{}", folder.slug)),
        })
    }
    workspace
        .collections
        .iter()
        .find_map(|collection| walk(&collection.items, id, &collection.slug))
}

/// The outcome a recorded entry shows in the response pane.
fn completed_of(entry: &HistoryEntry) -> Completed {
    match &entry.record.outcome {
        HistoryOutcome::Response(recorded) => {
            Completed::Response(received_of(recorded, &entry.record.sent))
        }
        HistoryOutcome::Failed { error, elapsed_ms } => Completed::Failure(Failure {
            error: error.clone(),
            elapsed: Duration::from_millis(*elapsed_ms),
            unresolved: Vec::new(),
            scripts: ScriptOutcome::default(),
        }),
    }
}

fn received_of(recorded: &RecordedResponse, sent: &SentRequest) -> Received {
    let response = Response {
        status: recorded.status,
        reason: recorded.reason.clone(),
        version: String::new(),
        headers: recorded.headers.clone(),
        body: recorded.body.clone(),
        final_url: sent.url.clone(),
        elapsed: Duration::from_millis(recorded.elapsed_ms),
    };
    Received::new(response, Vec::new(), ScriptOutcome::default())
}

impl Shell {
    /// What a send of the request `id` with `document` records, or `None` when it cannot
    /// be serialized; it is logged.
    pub(super) fn pending_record(&self, id: &str, document: Option<&Request>) -> Option<Pending> {
        let super::OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return None;
        };
        let (path, saved) = find_request(engine.workspace(), id)?;
        let request = document.unwrap_or(saved);
        match toml::to_string(request) {
            Ok(document) => Some(Pending {
                sent_at: now_millis(),
                request_id: id.to_owned(),
                request_name: request.name.clone(),
                request_path: path,
                document,
            }),
            Err(error) => {
                log::warn!("cannot serialize request {id} for history: {error}");
                None
            }
        }
    }

    /// Records a finished send in the app state and refreshes the History panel. A failure
    /// is logged and never reaches the send; without the app state nothing is recorded.
    pub(super) fn record_send(
        &mut self,
        pending: Pending,
        sent: SentRequest,
        outcome: HistoryOutcome,
        cx: &mut Context<Self>,
    ) {
        let (Some(state), Some(workspace_id)) = (self.state.as_mut(), self.workspace_id.as_deref())
        else {
            return;
        };
        let request_id = pending.request_id.clone();
        let record = HistoryRecord {
            sent_at: pending.sent_at,
            request_id: pending.request_id,
            request_name: pending.request_name,
            request_path: pending.request_path,
            document: pending.document,
            sent,
            outcome,
        };
        if let Err(error) = state.record_history(workspace_id, record) {
            log::warn!("cannot record the send of {request_id} in history: {error}");
        }
        self.split.forget_recorded();
        self.refresh_history(cx);
    }

    /// Reads the newest entries of the workspace into the History panel.
    pub(super) fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let (Some(state), Some(workspace_id)) = (self.state.as_ref(), self.workspace_id.as_deref())
        else {
            return;
        };
        let entries = match state.history(workspace_id, PANEL_LIMIT) {
            Ok(entries) => entries,
            Err(error) => {
                log::warn!("cannot read the history of workspace {workspace_id}: {error}");
                return;
            }
        };
        let now = now_millis();
        let open = self.orphan.as_ref().map(|tab| tab.entry);
        let rows = entries
            .iter()
            .map(|entry| {
                let deleted =
                    super::documents::locate(&self.roots, &entry.record.request_id).is_none();
                row_of(entry, now, deleted, open == Some(entry.id))
            })
            .collect();
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_history(rows, cx));
    }

    /// Shows `panel` in the sidebar island; History reads the entries again so their times
    /// are current.
    pub(super) fn show_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        if panel == Panel::History {
            self.refresh_history(cx);
        }
        if panel != Panel::Environments {
            self.commit_variable_edit(cx);
        }
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_panel(panel, cx));
        cx.notify();
    }

    /// Opens the history entry `entry` in the history tab, replacing the entry it showed, and
    /// activates it. The request, its tab and its draft are untouched.
    pub(super) fn open_history_entry(&mut self, entry: i64, cx: &mut Context<Self>) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let entry = match state.history_entry(entry) {
            Ok(Some(entry)) => entry,
            Ok(None) => return,
            Err(error) => {
                log::warn!("cannot read history entry {entry}: {error}");
                return;
            }
        };
        let id = entry.record.request_id.clone();
        let exists = super::documents::locate(&self.roots, &id).is_some();
        self.leave_approvals(cx);
        self.drop_history_tab();
        let now = now_millis();
        let offset = clock::local_offset(now);
        let (day, time) = clock::day_and_time(entry.record.sent_at, now, offset);
        let tab = HistoryTab {
            entry: entry.id,
            request_id: id,
            name: entry.record.request_name.clone(),
            sent: entry.record.sent.clone(),
            time,
            moment: format!("{day} {}", clock::seconds(entry.record.sent_at, offset)),
            source_exists: exists,
            tab: RecordedTab::default(),
            active: true,
        };
        let send = RequestSend::reopened(
            completed_of(&entry),
            Reopened {
                document: entry.record.document.clone(),
                resendable: exists,
            },
        );
        self.sends.insert(tab.key(), send);
        self.orphan = Some(tab);
        self.auth_menu_open = false;
        self.body_menu_open = false;
        self.method_menu_open = false;
        self.refresh_history(cx);
        cx.notify();
    }

    /// Shows the last recorded send of the request `id` in its tab when it has no send in
    /// this session (request-pane-feedback S12): its response, masked as history stored it,
    /// or its failure. A request never sent keeps no send.
    pub(super) fn seed_recorded_send(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.sends.contains_key(id) {
            return;
        }
        let (Some(state), Some(workspace_id)) = (self.state.as_ref(), self.workspace_id.as_deref())
        else {
            return;
        };
        match state.last_history_of(workspace_id, id) {
            Ok(Some(entry)) => {
                self.sends
                    .insert(id.to_owned(), RequestSend::recorded(completed_of(&entry)));
                cx.notify();
            }
            Ok(None) => {}
            Err(error) => log::warn!("cannot read the last send of {id}: {error}"),
        }
    }

    /// Whether the history tab is the active document.
    pub(super) fn history_tab_active(&self) -> bool {
        self.orphan.as_ref().is_some_and(|tab| tab.active)
    }

    /// The id of the active request tab; `None` while the history tab is active.
    pub(super) fn active_request(&self) -> Option<&str> {
        if self.history_tab_active() {
            None
        } else {
            self.documents.active()
        }
    }

    /// Leaves the history tab in the row, inactive, as activating a request does.
    pub(super) fn leave_history_tab(&mut self) {
        if let Some(tab) = self.orphan.as_mut() {
            tab.active = false;
        }
    }

    /// Activates the history tab.
    pub(super) fn activate_history_tab(&mut self, cx: &mut Context<Self>) {
        self.leave_approvals(cx);
        if let Some(tab) = self.orphan.as_mut() {
            tab.active = true;
        }
        cx.notify();
    }

    /// Closes the history tab; the request tab that was active before shows again.
    pub(super) fn close_history_tab(&mut self, cx: &mut Context<Self>) {
        self.drop_history_tab();
        self.refresh_history(cx);
        cx.notify();
    }

    fn drop_history_tab(&mut self) {
        if let Some(tab) = self.orphan.take() {
            self.sends.remove(&tab.key());
        }
    }

    /// Selects the view `tab` of the recorded request.
    pub(super) fn select_recorded_tab(&mut self, tab: RecordedTab, cx: &mut Context<Self>) {
        if let Some(history) = self.orphan.as_mut() {
            history.tab = tab;
            cx.notify();
        }
    }

    /// Whether Resend can run in the history tab: it is active and its request exists.
    pub(super) fn can_resend(&self) -> Result<(), &'static str> {
        match &self.orphan {
            Some(tab) if tab.active && tab.source_exists => Ok(()),
            Some(tab) if tab.active => Err(RESEND_NOTE),
            _ => Err("no recorded request is open"),
        }
    }

    /// Open source request: opens the request of the history tab in its own tab.
    pub(super) fn open_history_source(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .orphan
            .as_ref()
            .filter(|tab| tab.source_exists)
            .map(|tab| tab.request_id.clone())
        else {
            return;
        };
        self.open_document(&id, cx);
    }

    /// Resend: opens the request of the history tab and sends the recorded document again in
    /// its tab through the engine's draft path, without touching its draft or file. Does
    /// nothing when the request no longer exists.
    pub(super) fn resend_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.orphan.as_ref().filter(|tab| tab.source_exists) else {
            return;
        };
        let id = tab.request_id.clone();
        let Some(reopened) = self
            .sends
            .get(&tab.key())
            .and_then(|send| send.history.clone())
            .filter(|reopened| reopened.resendable)
        else {
            return;
        };
        match toml::from_str::<Request>(&reopened.document) {
            Ok(request) => {
                self.open_document(&id, cx);
                self.send_document(&id, Some(request), window, cx);
            }
            Err(error) => log::warn!("the recorded document of {id} did not parse: {error}"),
        }
    }
}
