//! The client of in-app approvals (mcp-approvals S1, S2, S6): when the policy says `ask`,
//! `reqflux mcp` asks a running window through [`reqflux_engine::approvals`] and maps its
//! answer to a decision and the reason of the call's audit line.
//!
//! No live window, no answer within the limit (120 seconds), a dropped connection or an
//! answer that is not a response refuses the call. A denial carries the reason the user
//! typed, if any: "denied in the window: <reason>". "Allow for this session" lets every
//! later call of the same class and workspace run without asking until this process exits.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use reqflux_core::model::{Item, Workspace};
use reqflux_engine::approvals::{self, ApprovalDecision, ApprovalRequest, AskError, WorkspaceRef};
use reqflux_engine::policy::ToolClass;

/// How long a window has to answer.
pub const ANSWER_LIMIT: Duration = Duration::from_secs(120);

/// Where and how long this server asks for approvals, and what it may skip asking for.
#[derive(Debug)]
pub struct Approvals {
    /// The runtime dir windows advertise in; `None` asks nothing, and `ask` refuses as the
    /// policy says.
    dir: Option<PathBuf>,
    limit: Duration,
    /// The (class, workspace id) pairs allowed for this session.
    session: HashSet<(ToolClass, Option<String>)>,
    next_id: u64,
    /// The name the MCP client gave in its `initialize`, sent with every request.
    client: Option<String>,
    /// How many asks of this server a window has not answered yet, shared with the `ui_*`
    /// input tools so they refuse while one is outstanding (mcp-approvals S5).
    outstanding: Arc<AtomicUsize>,
}

impl Approvals {
    /// No approvals: `ask` refuses. What [`EngineState::new`](crate::engine_tools::EngineState::new)
    /// starts with, so tests never reach a real window.
    #[must_use]
    pub fn disabled() -> Self {
        Self::in_dir(None, ANSWER_LIMIT)
    }

    /// Approvals from the windows of this user, in `$XDG_RUNTIME_DIR/reqflux`, within
    /// [`ANSWER_LIMIT`]. Without a runtime dir there are none.
    #[must_use]
    pub fn user() -> Self {
        Self::in_dir(approvals::runtime_dir(), ANSWER_LIMIT)
    }

    /// Approvals from the windows advertised in `dir`, within `limit`.
    #[must_use]
    pub fn in_dir(dir: Option<PathBuf>, limit: Duration) -> Self {
        Self {
            dir,
            limit,
            session: HashSet::new(),
            next_id: 1,
            client: None,
            outstanding: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// The runtime dir windows advertise in, when approvals are asked at all.
    pub(crate) fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// The count of this server's asks no window has answered yet.
    pub(crate) fn outstanding(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.outstanding)
    }

    /// Names the MCP client in every later request: its `clientInfo` title, else its name.
    pub fn set_client(&mut self, client: Option<String>) {
        self.client = client.filter(|name| !name.trim().is_empty());
    }

    /// The MCP client's name, when it gave one.
    #[cfg(test)]
    pub(crate) fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    /// Whether `ask` is asked at all; when not, it refuses as the policy says.
    pub(crate) fn enabled(&self) -> bool {
        self.dir.is_some()
    }

    /// Asks a window whether `ask` may run, unless its class and workspace are allowed for
    /// this session.
    pub(crate) fn approve(&mut self, ask: Ask) -> Outcome {
        let key = (ask.class, ask.workspace.as_ref().map(|w| w.id.clone()));
        if self.session.contains(&key) {
            return Outcome::AllowedSession;
        }
        let Some(dir) = &self.dir else {
            return Outcome::Disabled;
        };
        let Some(window) = approvals::find_window(dir) else {
            return Outcome::NoWindow;
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let request = ApprovalRequest {
            version: approvals::PROTOCOL_VERSION,
            id,
            tool: ask.tool.to_owned(),
            class: ask.class.name().to_owned(),
            workspace: ask.workspace,
            targets: ask.targets,
            method: ask.method,
            url: ask.url,
            client: self.client.clone(),
            item_ids: ask.item_ids,
            environment: ask.environment,
            policy: Some(ask.policy),
            requested_at: Some(now_millis()),
        };
        let answer = {
            let _asking = Outstanding::start(&self.outstanding);
            approvals::ask(&window.socket, &request, self.limit)
        };
        match answer.map(|answer| (answer.decision, answer.reason)) {
            Ok((ApprovalDecision::AllowOnce, _)) => Outcome::AllowedOnce,
            Ok((ApprovalDecision::AllowSession, _)) => {
                self.session.insert(key);
                Outcome::AllowedSession
            }
            Ok((ApprovalDecision::Deny, reason)) => {
                Outcome::Denied(reason.as_deref().and_then(clean_reason))
            }
            Err(AskError::Unreachable(error)) => {
                log::debug!(target: "reqflux_mcp", "the approval window is unreachable: {error}");
                Outcome::NoWindow
            }
            Err(AskError::NoAnswer(error)) => {
                log::debug!(target: "reqflux_mcp", "no approval answer: {error}");
                Outcome::NoAnswer(self.limit)
            }
        }
    }
}

/// One outstanding ask, counted until it is dropped.
struct Outstanding<'a>(&'a AtomicUsize);

impl<'a> Outstanding<'a> {
    fn start(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count)
    }
}

impl Drop for Outstanding<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What one approval asks about. Every field is a name, an id or a masked value.
#[derive(Debug)]
pub(crate) struct Ask {
    pub(crate) tool: &'static str,
    pub(crate) class: ToolClass,
    pub(crate) workspace: Option<WorkspaceRef>,
    pub(crate) targets: Vec<String>,
    pub(crate) method: Option<String>,
    pub(crate) url: Option<String>,
    /// The ids of the targets, so the window can read them from its own workspace.
    pub(crate) item_ids: Vec<String>,
    /// The environment the call uses, as the agent named it.
    pub(crate) environment: Option<String>,
    /// The rule that asked, such as `execute → approval`.
    pub(crate) policy: String,
}

/// Now, in milliseconds since the Unix epoch; 0 before it.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The longest denial reason a refusal carries, in characters.
const MAX_REASON: usize = 200;

/// `reason` as a refusal carries it: without control characters, trimmed and cut to
/// [`MAX_REASON`] characters; `None` when nothing is left.
fn clean_reason(reason: &str) -> Option<String> {
    let kept: String = reason
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let cut: String = kept.trim().chars().take(MAX_REASON).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

/// How an approval ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    AllowedOnce,
    AllowedSession,
    /// Denied in the window, with the reason the user gave, cleaned.
    Denied(Option<String>),
    NoAnswer(Duration),
    NoWindow,
    /// Approvals are off: the policy's refusal stands as is.
    Disabled,
}

impl Outcome {
    /// The audit reason of an approved call, or the refusal text built on `policy`, the
    /// policy's own refusal.
    pub(crate) fn result(self, policy: &str) -> Result<&'static str, String> {
        match self {
            Self::AllowedOnce => Ok("approved once"),
            Self::AllowedSession => Ok("approved for this session"),
            Self::Denied(None) => Err(format!("{policy}; denied in the window")),
            Self::Denied(Some(reason)) => Err(format!("{policy}; denied in the window: {reason}")),
            Self::NoAnswer(limit) => Err(format!(
                "{policy}; no answer from the window within {}",
                seconds(limit)
            )),
            Self::NoWindow => Err(format!("{policy}; no window is running to approve it")),
            Self::Disabled => Err(policy.to_owned()),
        }
    }
}

/// `limit` as `<n> s`, or `<n> ms` below a second.
fn seconds(limit: Duration) -> String {
    if limit.as_secs() > 0 {
        format!("{} s", limit.as_secs())
    } else {
        format!("{} ms", limit.as_millis())
    }
}

/// The name of the collection, folder, request or environment `id` in `workspace`; the id
/// itself when none has it.
pub(crate) fn target_name(workspace: &Workspace, id: &str) -> String {
    let environment = workspace
        .environments
        .iter()
        .find(|node| node.environment.id.to_string() == id)
        .map(|node| node.environment.name.clone());
    environment
        .or_else(|| {
            workspace.collections.iter().find_map(|collection| {
                if collection.collection.id.to_string() == id {
                    Some(collection.collection.name.clone())
                } else {
                    item_name(&collection.items, id)
                }
            })
        })
        .unwrap_or_else(|| id.to_owned())
}

fn item_name(items: &[Item], id: &str) -> Option<String> {
    items.iter().find_map(|item| match item {
        Item::Folder(folder) if folder.folder.id.to_string() == id => {
            Some(folder.folder.name.clone())
        }
        Item::Folder(folder) => item_name(&folder.items, id),
        Item::Request(request) if request.request.id.to_string() == id => {
            Some(request.request.name.clone())
        }
        Item::Request(_) => None,
    })
}
