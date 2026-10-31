//! What the MCP approvals view shows of a call (mcp-approvals S3), apart from how it draws.
//!
//! A request carries names, ids and masked values only. Everything else the detail shows —
//! the request breadcrumb, the method, URL template and resolved URL, the headers, the body
//! and the variables — the window reads from its own open workspace through
//! [`reqflux_engine::approvals::preview`], masked by the engine. When the call works on
//! another workspace, or the request cannot be read, the detail shows only what the request
//! carries and says why the rest is not available.

use reqflux_engine::approvals::{self, ApprovalRequest, CallPreview};

use crate::workspace::OpenedWorkspace;

/// Who asked, when the MCP client gave no name.
pub(crate) const NO_CLIENT: &str = "an agent";

/// The tool that sends a request, the only one with "What it will send".
pub(crate) const EXECUTE_TOOL: &str = "request_execute";

/// The longest value the view shows; longer ones are cut.
const MAX_VALUE: usize = 200;

/// What the detail shows under "What it will send".
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sends {
    /// The call sends nothing: a write, a destructive or a read tool.
    Nothing,
    /// The request as the window prepared it from its own workspace.
    Preview(Box<CallPreview>),
    /// Why the window could not prepare it; the detail falls back to what the request
    /// carries.
    Unavailable(String),
}

/// What the window read about one call from its own workspace, when the call arrived or the
/// view was refreshed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Detail {
    /// The call's first target as `collection › folder › name`, or the target names the
    /// request carries.
    pub(crate) target: String,
    /// The environment by name, when the call names one.
    pub(crate) environment: Option<String>,
    pub(crate) sends: Sends,
}

impl Detail {
    /// Reads what `workspace` knows of `request`.
    pub(crate) fn read(request: &ApprovalRequest, workspace: &OpenedWorkspace) -> Self {
        let engine = match workspace {
            OpenedWorkspace::Loaded(engine)
                if request.workspace.as_ref().is_some_and(|called| {
                    called.id == engine.workspace().manifest.id.to_string()
                }) =>
            {
                Some(engine)
            }
            OpenedWorkspace::Loaded(_) | OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => {
                None
            }
        };
        let first = request.item_ids.first();
        let target = engine
            .zip(first)
            .and_then(|(engine, id)| approvals::breadcrumb(engine.workspace(), id))
            .map_or_else(|| request.targets.join(", "), |path| path.join(" › "));
        let sends = if request.tool != EXECUTE_TOOL {
            Sends::Nothing
        } else {
            match (engine, first) {
                (None, _) => Sends::Unavailable(format!(
                    "The call works on {}, which is not the workspace open in this window, \
                     so its details are not available.",
                    request
                        .workspace
                        .as_ref()
                        .map_or("no workspace", |workspace| workspace.name.as_str())
                )),
                (Some(_), None) => Sends::Unavailable(
                    "The call does not name its request, so its details are not available."
                        .to_owned(),
                ),
                (Some(engine), Some(id)) => {
                    match approvals::preview(engine, id, request.environment.as_deref()) {
                        Ok(preview) => Sends::Preview(Box::new(preview)),
                        Err(error) => Sends::Unavailable(format!(
                            "The request could not be read ({error}), so its details are not \
                             available."
                        )),
                    }
                }
            }
        };
        let environment = match &sends {
            Sends::Preview(preview) => preview.environment.clone(),
            Sends::Nothing | Sends::Unavailable(_) => None,
        }
        .or_else(|| {
            request.environment.as_ref().map(|given| {
                engine
                    .and_then(|engine| approvals::find_environment(engine.workspace(), given))
                    .map_or_else(|| given.clone(), |node| node.environment.name.clone())
            })
        });
        Self {
            target: shown(&target),
            environment: environment.map(|name| shown(&name)),
            sends,
        }
    }

    /// The method and the path the row shows for an execution: from the preview, else from
    /// the masked URL the request carries.
    pub(crate) fn request_line(&self, request: &ApprovalRequest) -> Option<(String, String)> {
        let (method, url) = match &self.sends {
            Sends::Preview(preview) => (Some(preview.method.clone()), Some(preview.url.clone())),
            Sends::Nothing | Sends::Unavailable(_) => (request.method.clone(), request.url.clone()),
        };
        method.map(|method| {
            (
                shown(&method),
                shown(&path_of(url.as_deref().unwrap_or(""))),
            )
        })
    }
}

/// The path of `url`, without the scheme, host and query; `/` when it has none.
fn path_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = rest
        .find('/')
        .map_or("", |start| rest.get(start..).unwrap_or(""));
    let path = path.split(['?', '#']).next().unwrap_or("");
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

/// `<n> approval(s) pending` for the status bar, or `None` when none is.
pub(crate) fn pending_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 approval pending".to_owned()),
        n => Some(format!("{n} approvals pending")),
    }
}

/// How long a call has waited: `waiting just now`, `waiting <n> min` or `waiting <n> h`.
pub(crate) fn waiting_label(arrived_at: i64, now: i64) -> String {
    let minutes = now.saturating_sub(arrived_at).max(0) / 60_000;
    match minutes {
        0 => "waiting just now".to_owned(),
        1..60 => format!("waiting {minutes} min"),
        _ => format!("waiting {} h", minutes / 60),
    }
}

/// The MCP client of `request`, or [`NO_CLIENT`].
pub(crate) fn client_name(request: &ApprovalRequest) -> String {
    request
        .client
        .as_deref()
        .map(shown)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| NO_CLIENT.to_owned())
}

/// The class as its badge prints it: `Read`, `Write`, `Execute` or `Destructive`.
pub(crate) fn class_label(class: &str) -> String {
    let class = shown(class);
    let mut characters = class.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// `value` without control characters, cut to [`MAX_VALUE`] characters.
pub(crate) fn shown(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_VALUE)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_reads_just_now_then_minutes_then_hours() {
        assert_eq!(waiting_label(0, 59_999), "waiting just now");
        assert_eq!(waiting_label(0, 60_000), "waiting 1 min");
        assert_eq!(waiting_label(0, 59 * 60_000), "waiting 59 min");
        assert_eq!(waiting_label(0, 125 * 60_000), "waiting 2 h");
        assert_eq!(waiting_label(10, 0), "waiting just now");
    }

    #[test]
    fn a_path_drops_the_origin_and_the_query() {
        assert_eq!(path_of("https://h.test/v1/invoices?key=••"), "/v1/invoices");
        assert_eq!(path_of("https://h.test"), "/");
        assert_eq!(path_of("{{base}}/ping"), "/ping");
    }

    #[test]
    fn a_class_prints_capitalized_and_a_missing_client_is_an_agent() {
        assert_eq!(class_label("execute"), "Execute");
        assert_eq!(client_name(&ApprovalRequest::default()), "an agent");
    }
}
