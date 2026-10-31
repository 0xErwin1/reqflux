//! The rmcp handlers of the engine tools, a router merged with the `ui_*` one. Each runs
//! its tool on the blocking pool: the engine blocks on its own runtime, and dropping or
//! blocking on that runtime inside an async handler would panic.

use std::sync::{Arc, PoisonError};

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData},
    tool, tool_router,
};

use super::{
    CollectionCreateArgs, EngineState, EntryArgs, EnvironmentArgs, EnvironmentCreateArgs,
    EnvironmentRenameArgs, ExecuteArgs, FolderCreateArgs, HistoryListArgs, ItemArgs, MoveArgs,
    PathArgs, RenameArgs, RequestCreateArgs, ScopeArgs, UpdateArgs, VariableDeleteArgs,
    VariableSetArgs, WorkspaceCreateArgs,
};
use crate::server::{ReqFluxMcp, into_result};
use crate::tools::ToolError;

impl ReqFluxMcp {
    /// Runs `tool` on the engine state on the blocking pool.
    async fn run_engine<F>(&self, tool: F) -> Result<CallToolResult, ErrorData>
    where
        F: FnOnce(&mut EngineState) -> Result<String, ToolError> + Send + 'static,
    {
        let engine = Arc::clone(&self.engine);
        let joined = tokio::task::spawn_blocking(move || {
            // A tool that panicked leaves nothing half-made to guard: the engine replaces its
            // loaded workspace whole and every read reloads it, so the lock is taken back
            // instead of disabling the engine tools for the session.
            let mut state = engine.lock().unwrap_or_else(PoisonError::into_inner);
            tool(&mut state)
        })
        .await;
        match joined {
            Ok(result) => into_result(result),
            Err(error) => into_result(Err(ToolError::Engine(format!(
                "the engine tool did not finish: {error}"
            )))),
        }
    }
}

#[tool_router(router = engine_tool_router, vis = "pub(crate)")]
impl ReqFluxMcp {
    #[tool(
        name = "workspace_open",
        description = "Open the workspace in a directory in place of the open one"
    )]
    async fn workspace_open(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::workspace_open(state, args))
            .await
    }

    #[tool(
        name = "workspace_create",
        description = "Create a workspace in a new or empty directory and open it"
    )]
    async fn workspace_create(
        &self,
        Parameters(args): Parameters<WorkspaceCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::workspace_create(state, args))
            .await
    }

    #[tool(
        name = "workspace_info",
        description = "Return the open workspace's name, id, path, load warnings and tree of collections, folders and requests"
    )]
    async fn workspace_info(&self) -> Result<CallToolResult, ErrorData> {
        self.run_engine(super::workspace_info).await
    }

    #[tool(
        name = "collection_create",
        description = "Create a collection; returns its id"
    )]
    async fn collection_create(
        &self,
        Parameters(args): Parameters<CollectionCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::collection_create(state, args))
            .await
    }

    #[tool(
        name = "folder_create",
        description = "Create a folder at the end of a collection or folder; returns its id"
    )]
    async fn folder_create(
        &self,
        Parameters(args): Parameters<FolderCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::folder_create(state, args))
            .await
    }

    #[tool(
        name = "request_create",
        description = "Create a request at the end of a collection or folder; returns its id"
    )]
    async fn request_create(
        &self,
        Parameters(args): Parameters<RequestCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::request_create(state, args))
            .await
    }

    #[tool(
        name = "item_rename",
        description = "Rename a collection, folder or request; returns its id"
    )]
    async fn item_rename(
        &self,
        Parameters(args): Parameters<RenameArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::item_rename(state, args))
            .await
    }

    #[tool(
        name = "item_duplicate",
        description = "Duplicate a folder or request right after it; returns the copy's id"
    )]
    async fn item_duplicate(
        &self,
        Parameters(args): Parameters<ItemArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::item_duplicate(state, args))
            .await
    }

    #[tool(
        name = "item_move",
        description = "Move a folder or request into a collection or folder at an index; returns its id"
    )]
    async fn item_move(
        &self,
        Parameters(args): Parameters<MoveArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::item_move(state, args))
            .await
    }

    #[tool(
        name = "item_delete",
        description = "Delete a collection, folder or request with everything under it; returns its id"
    )]
    async fn item_delete(
        &self,
        Parameters(args): Parameters<ItemArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::item_delete(state, args))
            .await
    }

    #[tool(
        name = "request_get",
        description = "Return a request document as TOML, auth secrets and credential header values masked, with its parent's id and slug path and the base_hash request_update needs"
    )]
    async fn request_get(
        &self,
        Parameters(args): Parameters<ItemArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::request_get(state, args))
            .await
    }

    #[tool(
        name = "request_update",
        description = "Replace a request with a TOML document carrying the same id, passing the base_hash request_get returned; a masked auth secret or credential header keeps the stored value, and a file changed since that request_get is a conflict"
    )]
    async fn request_update(
        &self,
        Parameters(args): Parameters<UpdateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::request_update(state, args))
            .await
    }

    #[tool(
        name = "request_execute",
        description = "Send a request with its scripts and return status, headers, elapsed, size, body text (cut at 256 KiB), unresolved variables and test results, secrets masked; records no history"
    )]
    async fn request_execute(
        &self,
        Parameters(args): Parameters<ExecuteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::request_execute(state, args))
            .await
    }

    #[tool(
        name = "history_list",
        description = "List the open workspace's recorded sends, newest first"
    )]
    async fn history_list(
        &self,
        Parameters(args): Parameters<HistoryListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::history_list(state, args))
            .await
    }

    #[tool(
        name = "history_get",
        description = "Return one recorded send of the open workspace with its request and response, every secret of the workspace masked"
    )]
    async fn history_get(
        &self,
        Parameters(args): Parameters<EntryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::history_get(state, args))
            .await
    }

    #[tool(
        name = "environment_list",
        description = "List the workspace environments with id, name, slug and variable count"
    )]
    async fn environment_list(&self) -> Result<CallToolResult, ErrorData> {
        self.run_engine(super::environment_list).await
    }

    #[tool(
        name = "environment_create",
        description = "Create an environment; returns its id"
    )]
    async fn environment_create(
        &self,
        Parameters(args): Parameters<EnvironmentCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::environment_create(state, args))
            .await
    }

    #[tool(
        name = "environment_rename",
        description = "Rename an environment, moving its file to the new slug; returns its id"
    )]
    async fn environment_rename(
        &self,
        Parameters(args): Parameters<EnvironmentRenameArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::environment_rename(state, args))
            .await
    }

    #[tool(
        name = "environment_delete",
        description = "Delete an environment; returns its id"
    )]
    async fn environment_delete(
        &self,
        Parameters(args): Parameters<EnvironmentArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::environment_delete(state, args))
            .await
    }

    #[tool(
        name = "variables_get",
        description = "List the variables of a scope (global, collection:<id> or environment:<id>); a secret's value shows only whether it is set"
    )]
    async fn variables_get(
        &self,
        Parameters(args): Parameters<ScopeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::variables_get(state, args))
            .await
    }

    #[tool(
        name = "variable_set",
        description = "Create a variable in a scope, or update only the given fields of an existing one; a secret's value goes to the OS keyring, never to a file"
    )]
    async fn variable_set(
        &self,
        Parameters(args): Parameters<VariableSetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::variable_set(state, args))
            .await
    }

    #[tool(
        name = "variable_delete",
        description = "Delete a variable from a scope, and a secret's keyring entry with it"
    )]
    async fn variable_delete(
        &self,
        Parameters(args): Parameters<VariableDeleteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.run_engine(move |state| super::variable_delete(state, args))
            .await
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use reqflux_engine::MemorySecretStore;

    use super::*;
    use crate::AppId;
    use crate::bridge::BridgeSession;

    /// A server whose policy allows every class, from a config file in `dir`.
    fn server(dir: &tempfile::TempDir) -> ReqFluxMcp {
        let config = dir.path().join("config/reqflux/config.toml");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            "[mcp]\nwrite = \"allow\"\nexecute = \"allow\"\ndestructive = \"allow\"\n",
        )
        .unwrap();
        let paths = super::super::PolicyPaths {
            config,
            audit: dir.path().join("state/reqflux/audit.log"),
        };
        ReqFluxMcp::new(
            BridgeSession::new(
                std::env::temp_dir().join("reqflux-mcp-engine-router-tests"),
                AppId::new("reqflux").unwrap(),
            ),
            EngineState::new(
                Arc::new(MemorySecretStore::new()),
                Some(dir.path().join("state/reqflux/state.sqlite3")),
                paths,
            ),
        )
    }

    fn text(result: &CallToolResult) -> String {
        let rmcp::model::ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected a text block");
        };
        text.text.clone()
    }

    #[tokio::test]
    async fn an_engine_tool_without_a_workspace_is_an_error_result() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = server(&dir).workspace_info().await.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result).contains("workspace_open"),
            "{}",
            text(&result)
        );
    }

    /// Replacing the open engine drops its runtime and a send blocks on it: both must run
    /// off the async handler, or Tokio panics.
    #[tokio::test]
    async fn engine_tools_replace_the_engine_and_send_inside_the_async_server() {
        let dir = tempfile::TempDir::new().unwrap();
        let server = server(&dir);
        for name in ["one", "two"] {
            let args = WorkspaceCreateArgs {
                path: dir.path().join(name).display().to_string(),
                name: name.to_owned(),
            };
            let result = server.workspace_create(Parameters(args)).await.unwrap();
            assert_eq!(result.is_error, Some(false), "{}", text(&result));
        }
        let created = server
            .collection_create(Parameters(CollectionCreateArgs { name: None }))
            .await
            .unwrap();
        let collection: serde_json::Value = serde_json::from_str(&text(&created)).unwrap();
        let args = RequestCreateArgs {
            parent_id: collection["id"].as_str().unwrap().to_owned(),
            name: None,
            method: None,
            url: Some("http://127.0.0.1:9/closed".to_owned()),
        };
        let created = server.request_create(Parameters(args)).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&text(&created)).unwrap();
        let args = ExecuteArgs {
            id: request["id"].as_str().unwrap().to_owned(),
            environment: None,
            vars: None,
        };
        let result = server.request_execute(Parameters(args)).await.unwrap();
        assert_eq!(result.is_error, Some(true), "a closed port fails the send");
    }
}
