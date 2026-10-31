//! The client of in-app approvals (mcp-approvals S1, S2, S6): when the policy says `ask`,
//! `reqflux mcp` asks a running window through [`reqflux_engine::approvals`] and maps its
//! answer to a decision and the reason of the call's audit line.
//!
//! No live window, no answer within the limit (120 seconds), a dropped connection or an
//! answer that is not a response refuses the call. "Allow for this session" lets every
//! later call of the same class and workspace run without asking until this process exits.

use std::collections::HashSet;
use std::path::PathBuf;
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
        }
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
        };
        match approvals::ask(&window.socket, &request, self.limit) {
            Ok(ApprovalDecision::AllowOnce) => Outcome::AllowedOnce,
            Ok(ApprovalDecision::AllowSession) => {
                self.session.insert(key);
                Outcome::AllowedSession
            }
            Ok(ApprovalDecision::Deny) => Outcome::Denied,
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

/// What one approval asks about. Every field is a name, an id or a masked value.
#[derive(Debug)]
pub(crate) struct Ask {
    pub(crate) tool: &'static str,
    pub(crate) class: ToolClass,
    pub(crate) workspace: Option<WorkspaceRef>,
    pub(crate) targets: Vec<String>,
    pub(crate) method: Option<String>,
    pub(crate) url: Option<String>,
}

/// How an approval ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    AllowedOnce,
    AllowedSession,
    Denied,
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
            Self::Denied => Err(format!("{policy}; denied in the window")),
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
