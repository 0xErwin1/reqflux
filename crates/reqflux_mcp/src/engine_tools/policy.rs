//! The MCP policy on the engine tools (user-config-mcp-policy S3-S7): each tool's class, the
//! check every engine tool runs before it reads or writes a file, and the audit line of every
//! write, execute and destructive call.
//!
//! The user's config file is read again before each call, so an edit applies without a
//! restart; one that does not load leaves the defaults, and every refusal says so. The
//! workspace overrides come from `[mcp.workspaces."<id>"]` and the open workspace's manifest
//! as last loaded. The `ui_*` tools are outside the policy.

use std::path::PathBuf;

use reqflux_core::model::Id;
use reqflux_engine::Engine;
use reqflux_engine::audit::{self, AuditEntry, Decision};
use reqflux_engine::config;
use reqflux_engine::policy::{Policy, ToolClass, WorkspaceScope};

use super::EngineState;
use crate::tools::ToolError;

/// Where the server reads the user's config and appends the audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyPaths {
    /// The user's config file.
    pub config: PathBuf,
    /// The audit log.
    pub audit: PathBuf,
}

impl PolicyPaths {
    /// The per-user paths: `$XDG_CONFIG_HOME/reqflux/config.toml` and
    /// `$XDG_STATE_HOME/reqflux/audit.log`.
    ///
    /// # Errors
    ///
    /// There is no home to locate them in.
    pub fn user() -> Result<Self, String> {
        Ok(Self {
            config: config::default_path().map_err(|error| error.to_string())?,
            audit: audit::default_path().map_err(|error| error.to_string())?,
        })
    }
}

/// The class of every engine tool (S5).
pub const TOOL_CLASSES: [(&str, ToolClass); 22] = [
    ("workspace_open", ToolClass::Read),
    ("workspace_info", ToolClass::Read),
    ("request_get", ToolClass::Read),
    ("history_list", ToolClass::Read),
    ("history_get", ToolClass::Read),
    ("environment_list", ToolClass::Read),
    ("variables_get", ToolClass::Read),
    ("workspace_create", ToolClass::Write),
    ("collection_create", ToolClass::Write),
    ("folder_create", ToolClass::Write),
    ("request_create", ToolClass::Write),
    ("item_rename", ToolClass::Write),
    ("item_duplicate", ToolClass::Write),
    ("item_move", ToolClass::Write),
    ("request_update", ToolClass::Write),
    ("environment_create", ToolClass::Write),
    ("environment_rename", ToolClass::Write),
    ("variable_set", ToolClass::Write),
    ("request_execute", ToolClass::Execute),
    ("item_delete", ToolClass::Destructive),
    ("environment_delete", ToolClass::Destructive),
    ("variable_delete", ToolClass::Destructive),
];

/// The class of the engine tool `tool`; `None` for a tool outside the policy.
#[must_use]
pub fn tool_class(tool: &str) -> Option<ToolClass> {
    TOOL_CLASSES
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, class)| *class)
}

/// Which workspace's restrictions apply to a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    /// The open workspace's, when one is open.
    Open,
    /// None: the call opens or creates another workspace.
    Global,
}

/// What an audit line of one call carries besides the decision.
#[derive(Debug, Clone)]
pub(super) struct Call {
    pub(super) tool: &'static str,
    pub(super) class: ToolClass,
    pub(super) workspace_id: Option<String>,
    pub(super) targets: Vec<String>,
}

impl Call {
    /// Writes the audit line of this call, unless it is a read.
    ///
    /// # Errors
    ///
    /// The line could not be written: the caller refuses the call.
    pub(super) fn audit(
        &self,
        paths: &PolicyPaths,
        decision: Decision,
        reason: Option<&str>,
        sent: Option<(&str, &str)>,
    ) -> Result<(), ToolError> {
        if self.class == ToolClass::Read {
            return Ok(());
        }
        let entry = AuditEntry {
            tool: self.tool,
            class: self.class,
            decision,
            reason,
            workspace_id: self.workspace_id.as_deref(),
            targets: &self.targets,
            method: sent.map(|(method, _)| method),
            url: sent.map(|(_, url)| url),
        };
        audit::append(&paths.audit, &entry).map_err(|error| {
            ToolError::Refused(format!(
                "{} was refused because its audit line could not be written: {error}",
                self.tool
            ))
        })
    }

    /// Audits the refusal `reason` and returns it as the tool error.
    pub(super) fn refuse(
        &self,
        paths: &PolicyPaths,
        reason: &str,
        sent: Option<(&str, &str)>,
    ) -> ToolError {
        match self.audit(paths, Decision::Refused, Some(reason), sent) {
            Ok(()) => ToolError::Refused(reason.to_owned()),
            Err(error) => ToolError::Refused(format!("{reason}; {error}")),
        }
    }
}

/// The policy for one call, from the config file as it is now and `engine`'s manifest.
pub(super) fn evaluate(paths: &PolicyPaths, engine: Option<&Engine>) -> Policy {
    let (loaded, load_error) = match config::load(&paths.config) {
        Ok(loaded) => (loaded, None),
        Err(error) => (config::UserConfig::default(), Some(error.to_string())),
    };
    let id = engine.map(|engine| engine.workspace().manifest.id.to_string());
    let manifest_path = engine.map(|engine| engine.root().join("reqflux.toml"));
    let scope = match (engine, &id, &manifest_path) {
        (Some(engine), Some(id), Some(manifest_path)) => Some(WorkspaceScope {
            id,
            manifest: engine.workspace().manifest.mcp.as_ref(),
            manifest_path,
        }),
        _ => None,
    };
    let policy = Policy::evaluate(&loaded.mcp, &paths.config, scope);
    match load_error {
        Some(error) => policy.with_load_error(&error),
        None => policy,
    }
}

impl EngineState {
    /// The call `tool` makes on `targets` (only the arguments that are ids are kept, so no
    /// argument value reaches the log).
    pub(super) fn call(&self, tool: &'static str, scope: Scope, targets: &[&str]) -> Call {
        let workspace_id = match scope {
            Scope::Open => self
                .engine
                .as_ref()
                .map(|engine| engine.workspace().manifest.id.to_string()),
            Scope::Global => None,
        };
        Call {
            tool,
            class: tool_class(tool).unwrap_or(ToolClass::Destructive),
            workspace_id,
            targets: targets
                .iter()
                .filter_map(|target| target.parse::<Id>().ok())
                .map(|id| id.to_string())
                .collect(),
        }
    }

    /// Checks the class of `tool` before it touches anything, and audits the decision of a
    /// write or destructive tool (S6, S7). Every engine tool but `request_execute` calls this
    /// first; `request_execute` audits once it knows the URL.
    ///
    /// # Errors
    ///
    /// The policy refused the call, or its audit line could not be written.
    pub(super) fn authorize(
        &self,
        tool: &'static str,
        scope: Scope,
        targets: &[&str],
    ) -> Result<(), ToolError> {
        let call = self.call(tool, scope, targets);
        let engine = match scope {
            Scope::Open => self.engine.as_ref(),
            Scope::Global => None,
        };
        match evaluate(&self.paths, engine).check_class(tool, call.class) {
            Ok(()) => call.audit(&self.paths, Decision::Allowed, None, None),
            Err(refusal) => Err(call.refuse(&self.paths, &refusal.to_string(), None)),
        }
    }

    /// The ids of a `global`, `collection:<id>` or `environment:<id>` scope argument.
    pub(super) fn scope_target(scope: &str) -> &str {
        scope.split_once(':').map_or("", |(_, id)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_tool_has_one_class() {
        let mut names: Vec<&str> = TOOL_CLASSES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TOOL_CLASSES.len());
        assert_eq!(tool_class("request_execute"), Some(ToolClass::Execute));
        assert_eq!(tool_class("ui_click"), None);
    }
}
