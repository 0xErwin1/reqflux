//! The MCP policy on the engine tools (user-config-mcp-policy S3-S7): each tool's class, the
//! check every engine tool runs before it reads or writes a file, and the audit line of every
//! write, execute and destructive call.
//!
//! The user's config file is read again before each call, so an edit applies without a
//! restart; one that does not load leaves the defaults, and every refusal says so. The
//! workspace overrides come from `[mcp.workspaces."<id>"]` and the open workspace's manifest
//! as last loaded.
//!
//! The `ui_*` tools that read the window or drive it are policed too (mcp-approvals S5): reads
//! of the tree are `read`, input is `execute`. `hosts` and `environments` do not apply to
//! them: the window sends what it sends. `ui_list_targets`, `ui_select_target` and `ui_ping`
//! stay outside the policy.
//!
//! A class whose rule is `ask` is asked in the running window (mcp-approvals S2); its audit
//! line records the approval in its reason (S6).

use std::path::PathBuf;

use reqflux_core::model::Id;
use reqflux_engine::Engine;
use reqflux_engine::approvals::WorkspaceRef;
use reqflux_engine::audit::{self, AuditEntry, Decision};
use reqflux_engine::config::{self, Rule};
use reqflux_engine::policy::{Policy, Refusal, ToolClass, WorkspaceScope};

use super::EngineState;
use crate::approvals::{Ask, target_name};
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

/// The class of every policed `ui_*` tool (mcp-approvals S5).
pub const UI_TOOL_CLASSES: [(&str, ToolClass); 10] = [
    ("ui_tree", ToolClass::Read),
    ("ui_find", ToolClass::Read),
    ("ui_element", ToolClass::Read),
    ("ui_element_bounds", ToolClass::Read),
    ("ui_wait_for_element", ToolClass::Read),
    ("ui_click", ToolClass::Execute),
    ("ui_type", ToolClass::Execute),
    ("ui_key", ToolClass::Execute),
    ("ui_focus", ToolClass::Execute),
    ("ui_scroll", ToolClass::Execute),
];

/// The class of the `ui_*` tool `tool`; `None` for one outside the policy.
#[must_use]
pub fn ui_tool_class(tool: &str) -> Option<ToolClass> {
    UI_TOOL_CLASSES
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, class)| *class)
}

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

/// The policy for one call, from the config file as it is now and `engine`'s manifest, with
/// the warnings of both in every refusal.
pub(super) fn evaluate(paths: &PolicyPaths, engine: Option<&Engine>) -> Policy {
    let (loaded, load_error, mut warnings) = match config::load_with_warnings(&paths.config) {
        Ok((loaded, warnings)) => (loaded, None, warnings),
        Err(error) => (
            config::UserConfig::default(),
            Some(error.to_string()),
            Vec::new(),
        ),
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
    if let Some(scope) = scope
        && let Some(manifest) = scope.manifest
    {
        warnings.extend(config::override_warnings(
            manifest,
            scope.manifest_path,
            "[mcp]",
        ));
    }
    let policy = Policy::evaluate(&loaded.mcp, &paths.config, scope).with_warnings(&warnings);
    match load_error {
        Some(error) => policy.with_load_error(&error),
        None => policy,
    }
}

/// The warnings of the config file and of `engine`'s manifest `[mcp]`: unknown keys and
/// `[mcp.workspaces]` keys that are not ids.
pub(super) fn warnings(paths: &PolicyPaths, engine: Option<&Engine>) -> Vec<String> {
    let mut warnings = config::load_with_warnings(&paths.config)
        .map(|(_, warnings)| warnings)
        .unwrap_or_default();
    if let Some(engine) = engine
        && let Some(manifest) = engine.workspace().manifest.mcp.as_ref()
    {
        warnings.extend(config::override_warnings(
            manifest,
            &engine.root().join("reqflux.toml"),
            "[mcp]",
        ));
    }
    warnings
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

    /// Checks the class of `tool` against the open workspace as it is on disk before it
    /// touches anything else, and audits the decision of a
    /// write or destructive tool (S6, S7). Every engine tool but `request_execute` calls this
    /// first; `request_execute` audits once it knows the URL.
    ///
    /// # Errors
    ///
    /// The policy refused the call, or its audit line could not be written.
    pub(super) fn authorize(
        &mut self,
        tool: &'static str,
        scope: Scope,
        targets: &[&str],
    ) -> Result<(), ToolError> {
        let call = self.call(tool, scope, targets);
        self.authorize_call(&call, scope)
    }

    /// Checks the `ui_*` tool `tool` against the open workspace's policy, or the user's
    /// alone with none open, and audits it unless it is a read (mcp-approvals S5). `target` is
    /// the bridge target the call drives; it is what the window is asked about, never an
    /// argument. A tool outside the policy passes.
    ///
    /// # Errors
    ///
    /// The policy refused the call, or its audit line could not be written.
    pub(crate) fn authorize_ui(
        &mut self,
        tool: &'static str,
        target: Option<&str>,
    ) -> Result<(), ToolError> {
        let Some(class) = ui_tool_class(tool) else {
            return Ok(());
        };
        let call = Call {
            class,
            targets: target.into_iter().map(str::to_owned).collect(),
            ..self.call(tool, Scope::Open, &[])
        };
        self.authorize_call(&call, Scope::Open)
    }

    /// Checks `call` and audits its decision; `ask` goes to the window.
    fn authorize_call(&mut self, call: &Call, scope: Scope) -> Result<(), ToolError> {
        let tool = call.tool;
        // The manifest may have changed on disk since the last call: reload it first, so a
        // `[mcp]` restriction added there refuses before anything is written.
        if scope == Scope::Open && self.engine.is_some() {
            self.reloaded()?;
        }
        let engine = match scope {
            Scope::Open => self.engine.as_ref(),
            Scope::Global => None,
        };
        let policy = evaluate(&self.paths, engine);
        match policy.check_class(tool, call.class) {
            Ok(()) => call.audit(&self.paths, Decision::Allowed, None, None),
            Err(refusal) if policy.rule(call.class) == Rule::Ask => {
                match self.approve(call, &refusal, None, None) {
                    Ok(reason) => call.audit(&self.paths, Decision::Allowed, Some(reason), None),
                    Err(text) => Err(call.refuse(&self.paths, &text, None)),
                }
            }
            Err(refusal) => Err(call.refuse(&self.paths, &refusal.to_string(), None)),
        }
    }

    /// Asks the window to approve `call`, which the policy refused with `refusal` because
    /// its rule is `ask`; `sent` is the method and masked URL of an execute and
    /// `environment` the environment it names. Returns the audit reason of the approval, or
    /// the refusal text.
    pub(super) fn approve(
        &mut self,
        call: &Call,
        refusal: &Refusal,
        sent: Option<(&str, &str)>,
        environment: Option<&str>,
    ) -> Result<&'static str, String> {
        let workspace = self.engine.as_ref().map(Engine::workspace);
        let in_workspace = workspace.filter(|_| call.workspace_id.is_some());
        let policy = ask_rule(
            &self.paths,
            in_workspace.map(|w| w.manifest.name.as_str()),
            call.class,
        );
        let ask = Ask {
            tool: call.tool,
            class: call.class,
            workspace: call.workspace_id.as_ref().map(|id| WorkspaceRef {
                id: id.clone(),
                name: workspace.map_or_else(String::new, |w| w.manifest.name.clone()),
            }),
            targets: call
                .targets
                .iter()
                .map(|id| workspace.map_or_else(|| id.clone(), |w| target_name(w, id)))
                .collect(),
            method: sent.map(|(method, _)| method.to_owned()),
            url: sent.map(|(_, url)| url.to_owned()),
            item_ids: call.targets.clone(),
            environment: environment.map(str::to_owned),
            policy,
        };
        self.approvals.approve(ask).result(&refusal.to_string())
    }

    /// Checks `tool`, a read, against `engine`, a workspace loaded but not open yet, so a
    /// read deny in `[mcp.workspaces."<id>"]` or its manifest refuses before anything of it
    /// is returned.
    ///
    /// # Errors
    ///
    /// The policy refused the read.
    pub(super) fn authorize_read_of(&self, tool: &str, engine: &Engine) -> Result<(), ToolError> {
        evaluate(&self.paths, Some(engine))
            .check_class(tool, ToolClass::Read)
            .map_err(|refusal| ToolError::Refused(refusal.to_string()))
    }

    /// Names the MCP client in every later approval request (mcp-approvals S3).
    pub fn set_client(&mut self, client: Option<String>) {
        self.approvals.set_client(client);
    }

    /// The ids of a `global`, `collection:<id>` or `environment:<id>` scope argument.
    pub(super) fn scope_target(scope: &str) -> &str {
        scope.split_once(':').map_or("", |(_, id)| id)
    }
}

/// The rule that made `class` ask, as the window shows it: `<class> → approval` when the
/// user's `[mcp]` (or its default) asks, `<class> in <workspace> → approval` when only the
/// open workspace's override or manifest raised it to `ask`.
fn ask_rule(paths: &PolicyPaths, workspace: Option<&str>, class: ToolClass) -> String {
    let loaded = config::load_with_warnings(&paths.config)
        .map(|(loaded, _)| loaded)
        .unwrap_or_default();
    let global = Policy::evaluate(&loaded.mcp, &paths.config, None).rule(class);
    match workspace {
        Some(name) if global != Rule::Ask => format!("{} in {name} → approval", class.name()),
        _ => format!("{} → approval", class.name()),
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

    #[test]
    fn ui_reads_are_read_input_is_execute_and_target_tools_are_unpoliced() {
        assert_eq!(ui_tool_class("ui_tree"), Some(ToolClass::Read));
        assert_eq!(ui_tool_class("ui_wait_for_element"), Some(ToolClass::Read));
        assert_eq!(ui_tool_class("ui_type"), Some(ToolClass::Execute));
        assert_eq!(ui_tool_class("ui_scroll"), Some(ToolClass::Execute));
        for tool in ["ui_list_targets", "ui_select_target", "ui_ping"] {
            assert_eq!(ui_tool_class(tool), None, "{tool}");
        }
    }
}
