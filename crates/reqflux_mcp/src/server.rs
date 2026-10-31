//! The rmcp stdio server that exposes the `ui_*` tools (S6, S7, S8) and the engine tools
//! (mcp-engine-tools S1-S6), each group with its own router.
//!
//! Every tool handler delegates to the plain functions in [`crate::tools`]
//! under a lock on the shared [`ServerState`], and maps a [`ToolError`] to a
//! `CallToolResult` with `is_error: true`, so a failing bridge call is an
//! error result with the bridge's message, never a panic.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};

use gpui_mcp_protocol::AppId;
use reqflux_engine::policy::ToolClass;
use rmcp::{
    ServerHandler, ServiceExt as _,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorData, Implementation, ServerCapabilities, ServerConfig,
    },
    tool, tool_handler, tool_router,
    transport::stdio,
};
use tokio::sync::{Mutex, MutexGuard};

use crate::bridge::BridgeSession;
use crate::engine_tools::{EngineState, ui_tool_class};
use crate::tools::{
    self, ClickArgs, FindArgs, IdArgs, KeyArgs, PAUSED, ScrollArgs, SelectTargetArgs, ServerState,
    TextArgs, ToolError, WaitArgs,
};

/// The ReqFlux MCP server: one bridge session, one engine state and the router of both
/// tool groups. The engine state is behind a std mutex because its tools run on the
/// blocking pool.
#[derive(Clone, Debug)]
pub struct ReqFluxMcp {
    state: Arc<Mutex<ServerState>>,
    pub(crate) engine: Arc<StdMutex<EngineState>>,
    /// How many of this server's approval asks are unanswered: input tools refuse meanwhile.
    outstanding: Arc<AtomicUsize>,
    tool_router: ToolRouter<Self>,
}

impl ReqFluxMcp {
    /// The bridge session once the policy allows the policed `ui_*` tool `tool` (mcp-approvals
    /// S5), or the refusal. An input tool refuses without asking while
    /// one of this server's asks is unanswered, so an agent cannot reach the approval it waits
    /// on.
    async fn ui_state(&self, tool: &'static str) -> Result<MutexGuard<'_, ServerState>, ToolError> {
        let Some(class) = ui_tool_class(tool) else {
            return Ok(self.state.lock().await);
        };
        if class == ToolClass::Execute && self.outstanding.load(Ordering::SeqCst) > 0 {
            return Err(ToolError::Refused(PAUSED.to_owned()));
        }
        let target = self.state.lock().await.policy_target().await;
        let engine = Arc::clone(&self.engine);
        let checked = tokio::task::spawn_blocking(move || {
            engine
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .authorize_ui(tool, target.as_deref())
        })
        .await
        .unwrap_or_else(|error| {
            Err(ToolError::Engine(format!(
                "the policy check of {tool} did not finish: {error}"
            )))
        });
        checked?;
        Ok(self.state.lock().await)
    }
}

#[tool_router]
impl ReqFluxMcp {
    /// A server that drives the app through `session` and works on `engine`'s workspace.
    #[must_use]
    pub fn new(session: BridgeSession, engine: EngineState) -> Self {
        let state =
            ServerState::new(session).with_approval_dir(engine.approvals().dir().map(Into::into));
        Self {
            state: Arc::new(Mutex::new(state)),
            outstanding: engine.approvals().outstanding(),
            engine: Arc::new(StdMutex::new(engine)),
            tool_router: Self::tool_router() + Self::engine_tool_router(),
        }
    }

    #[tool(
        name = "ui_list_targets",
        description = "List the live bridge targets of the app, each marked when it is selected"
    )]
    async fn ui_list_targets(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        into_result(tools::list_targets(&mut state).await)
    }

    #[tool(
        name = "ui_select_target",
        description = "Choose the target the ui_* tools drive for the rest of the session"
    )]
    async fn ui_select_target(
        &self,
        Parameters(args): Parameters<SelectTargetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        into_result(tools::select_target(&mut state, args).await)
    }

    #[tool(
        name = "ui_ping",
        description = "Check that the selected target is alive; returns its app id and pid"
    )]
    async fn ui_ping(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        into_result(tools::ping(&mut state).await)
    }

    #[tool(
        name = "ui_tree",
        description = "Return the latest semantic element tree with its generation, ids, labels, roles, bounds and state"
    )]
    async fn ui_tree(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_tree").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::tree(&mut state).await)
    }

    #[tool(
        name = "ui_find",
        description = "Find elements by label or id substring and role in the latest tree"
    )]
    async fn ui_find(
        &self,
        Parameters(args): Parameters<FindArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_find").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::find(&mut state, args).await)
    }

    #[tool(
        name = "ui_element",
        description = "Return one element with its parent, children, state, text and value"
    )]
    async fn ui_element(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_element").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::element(&mut state, args).await)
    }

    #[tool(
        name = "ui_element_bounds",
        description = "Return one element's bounds in logical window pixels"
    )]
    async fn ui_element_bounds(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_element_bounds").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::element_bounds(&mut state, args).await)
    }

    #[tool(
        name = "ui_wait_for_element",
        description = "Poll the tree until a visible element matches the query, then return it"
    )]
    async fn ui_wait_for_element(
        &self,
        Parameters(args): Parameters<WaitArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_wait_for_element").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::wait_for_element(&mut state, args).await)
    }

    #[tool(
        name = "ui_click",
        description = "Click an element at its center through the app's pointer pipeline"
    )]
    async fn ui_click(
        &self,
        Parameters(args): Parameters<ClickArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_click").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::click(&mut state, args).await)
    }

    #[tool(
        name = "ui_type",
        description = "Type text into the focused field; the app refuses redacted fields"
    )]
    async fn ui_type(
        &self,
        Parameters(args): Parameters<TextArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_type").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::type_text(&mut state, args).await)
    }

    #[tool(
        name = "ui_key",
        description = "Dispatch one GPUI keystroke, such as `enter` or `ctrl-a`"
    )]
    async fn ui_key(
        &self,
        Parameters(args): Parameters<KeyArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_key").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::key(&mut state, args).await)
    }

    #[tool(
        name = "ui_focus",
        description = "Move the app's keyboard focus to one element"
    )]
    async fn ui_focus(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_focus").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::focus(&mut state, args).await)
    }

    #[tool(
        name = "ui_scroll",
        description = "Scroll at an element's center or at window coordinates, with pixel deltas"
    )]
    async fn ui_scroll(
        &self,
        Parameters(args): Parameters<ScrollArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = match self.ui_state("ui_scroll").await {
            Ok(state) => state,
            Err(error) => return into_result(Err(error)),
        };
        into_result(tools::scroll(&mut state, args).await)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ReqFluxMcp {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new("reqflux", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Drive the running ReqFlux window through its gpui-mcp bridge: call \
                 ui_list_targets first when more than one window may be running, then \
                 ui_select_target; a single live target is selected automatically. Read \
                 the tree with ui_tree or ui_find before any input. Coordinates are \
                 logical pixels relative to the selected window. The engine tools \
                 (workspace_*, collection_create, folder_create, request_*, item_*, \
                 history_*) work on files directly: call workspace_info first; when no \
                 workspace is open, call workspace_open or workspace_create. Auth \
                 secrets show masked; send the mask back to keep a stored value. The \
                 engine tools and the ui_* reads (read) and input (execute) follow the \
                 user's MCP policy: by default only reads run, and a refusal names the \
                 setting that refused it."
                    .to_owned(),
            )
    }
}

/// A tool result or an `isError` result carrying the failure's message (S8).
pub(crate) fn into_result(result: Result<String, ToolError>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(text) => Ok(CallToolResult::success(vec![ContentBlock::text(text)])),
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(
            error.to_string(),
        )])),
    }
}

/// Serve the `ui_*` and engine tools on stdio until the client disconnects (S1's
/// server half; the `reqflux mcp` subcommand is wired separately).
///
/// # Errors
///
/// Returns an error when the server cannot start or the transport fails.
pub async fn serve(
    endpoint_dir: PathBuf,
    app_id: AppId,
    engine: EngineState,
) -> anyhow::Result<()> {
    log::info!(target: "reqflux_mcp", "serving ui_* and engine tools on stdio for app {app_id}");
    let server = ReqFluxMcp::new(BridgeSession::new(endpoint_dir, app_id), engine);
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}

#[cfg(all(test, unix))]
mod ui_policy_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const TOOL_NAMES: [&str; 13] = [
        "ui_list_targets",
        "ui_select_target",
        "ui_ping",
        "ui_tree",
        "ui_find",
        "ui_element",
        "ui_element_bounds",
        "ui_wait_for_element",
        "ui_click",
        "ui_type",
        "ui_key",
        "ui_focus",
        "ui_scroll",
    ];

    const ENGINE_TOOL_NAMES: [&str; 22] = [
        "workspace_open",
        "workspace_create",
        "workspace_info",
        "collection_create",
        "folder_create",
        "request_create",
        "item_rename",
        "item_duplicate",
        "item_move",
        "item_delete",
        "request_get",
        "request_update",
        "request_execute",
        "history_list",
        "history_get",
        "environment_list",
        "environment_create",
        "environment_rename",
        "environment_delete",
        "variables_get",
        "variable_set",
        "variable_delete",
    ];

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn server() -> ReqFluxMcp {
        ReqFluxMcp::new(
            BridgeSession::new(
                std::env::temp_dir().join("reqflux-mcp-server-tests"),
                AppId::new("reqflux").unwrap(),
            ),
            EngineState::new(
                Arc::new(reqflux_engine::MemorySecretStore::new()),
                None,
                crate::engine_tools::PolicyPaths {
                    config: std::env::temp_dir().join("reqflux-mcp-server-tests/no-config.toml"),
                    audit: std::env::temp_dir().join("reqflux-mcp-server-tests/audit.log"),
                },
            ),
        )
    }

    #[test]
    fn every_ui_tool_is_registered() {
        let server = server();
        for name in TOOL_NAMES {
            assert!(server.tool_router.has_route(name), "{name} is registered");
        }
    }

    #[test]
    fn the_tool_list_holds_the_ui_and_engine_tools_and_nothing_else() {
        let server = server();
        let mut listed: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        listed.sort();
        let mut expected: Vec<String> = TOOL_NAMES
            .iter()
            .chain(ENGINE_TOOL_NAMES.iter())
            .map(|name| (*name).to_owned())
            .collect();
        expected.sort();
        assert_eq!(listed, expected);
    }

    /// The target tools are outside the MCP policy: they run under the default one, which
    /// refuses every engine write and execute, and write no audit line.
    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn the_target_tools_run_under_the_default_policy_unaudited() {
        let dir = tempfile::TempDir::new().unwrap();
        let audit = dir.path().join("state/reqflux/audit.log");
        let server = ReqFluxMcp::new(
            BridgeSession::new(
                dir.path().join("no-endpoints"),
                AppId::new("reqflux").unwrap(),
            ),
            EngineState::new(
                Arc::new(reqflux_engine::MemorySecretStore::new()),
                None,
                crate::engine_tools::PolicyPaths {
                    config: dir.path().join("config/reqflux/config.toml"),
                    audit: audit.clone(),
                },
            ),
        );
        let result = server.ui_list_targets().await.unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(!audit.exists());
    }

    #[test]
    fn the_server_names_reqflux_with_the_crate_version() {
        let info = server().get_info();
        assert_eq!(info.server_info.name, "reqflux");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.capabilities.tools.is_some());
    }

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    #[test]
    fn a_tool_error_becomes_an_is_error_result_with_the_message() {
        let result =
            into_result(Err(ToolError::Invalid("boom".to_owned()))).unwrap_or_else(|error| {
                panic!("error results are values, not protocol errors: {error}")
            });
        assert_eq!(result.is_error, Some(true));
        let rmcp::model::ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected a text block");
        };
        assert_eq!(text.text, "boom");
    }

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    #[test]
    fn a_tool_success_is_a_text_result_without_is_error() {
        let result = into_result(Ok("{\"ok\": true}".to_owned()))
            .unwrap_or_else(|error| panic!("expected a result: {error}"));
        assert_eq!(result.is_error, Some(false), "success is not an error");
        let rmcp::model::ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected a text block");
        };
        assert_eq!(text.text, "{\"ok\": true}");
    }
}
