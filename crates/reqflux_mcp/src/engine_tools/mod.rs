//! The engine tools: workspace, tree, request, execute, history, environments and variables
//! (mcp-engine-tools S1-S6).
//!
//! Handlers are plain synchronous functions over [`EngineState`] returning the JSON text
//! result, so they can be tested on temp workspaces without rmcp. They block (the engine
//! owns its own Tokio runtime and its sends block on it), so the server runs them on the
//! blocking pool. The engine state is separate from the `ui_*` bridge session.
//!
//! One lock guards the state, and `request_execute` holds it for the whole send, so other
//! engine tools wait until the response (or the request timeout) arrives. A tool that
//! panicked does not disable the others: the lock is recovered.

mod environments;
mod execute;
mod history;
mod policy;
mod request;
pub(crate) mod router;
mod tree;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqflux_core::model::{Id, Item, Workspace};
use reqflux_engine::{Engine, SecretStore, create_workspace};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::approvals::Approvals;
use crate::tools::ToolError;
use policy::Scope;

pub use environments::{
    EnvironmentArgs, EnvironmentCreateArgs, EnvironmentRenameArgs, ScopeArgs, VariableDeleteArgs,
    VariableSetArgs, environment_create, environment_delete, environment_list, environment_rename,
    variable_delete, variable_set, variables_get,
};
pub use execute::request_execute;
pub use history::{history_get, history_list};
pub use policy::{PolicyPaths, TOOL_CLASSES, UI_TOOL_CLASSES, tool_class, ui_tool_class};
pub use request::{request_get, request_update};
pub use tree::{
    collection_create, folder_create, item_delete, item_duplicate, item_move, item_rename,
    request_create,
};

/// The workspace one MCP server works on: at most one is open at a time (S1).
pub struct EngineState {
    engine: Option<Engine>,
    secrets: Arc<dyn SecretStore>,
    /// The app state file history is read from; `None` is the per-user default.
    state_path: Option<PathBuf>,
    /// The user's config file the policy is read from and the audit log.
    paths: PolicyPaths,
    /// Where `ask` is approved, and what this session already allowed.
    approvals: Approvals,
}

impl fmt::Debug for EngineState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EngineState")
            .field("root", &self.engine.as_ref().map(Engine::root))
            .field("state_path", &self.state_path)
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl Drop for EngineState {
    /// The engine owns a Tokio runtime, and dropping one inside an async context panics; the
    /// server is dropped inside one when the client disconnects. Drop the engine on its own
    /// thread then.
    fn drop(&mut self) {
        let Some(engine) = self.engine.take() else {
            return;
        };
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let dropped = std::thread::Builder::new()
            .name("reqflux-engine-drop".to_owned())
            .spawn(move || drop(engine))
            .map(std::thread::JoinHandle::join);
        match dropped {
            Ok(Ok(())) => {}
            Ok(Err(_)) => log::warn!(target: "reqflux_mcp", "dropping the engine panicked"),
            Err(error) => {
                log::warn!(target: "reqflux_mcp", "could not drop the engine off the runtime: {error}");
            }
        }
    }
}

impl EngineState {
    /// A state with no workspace open that reads secrets from `secrets`, history from
    /// the app state at `state_path` (the per-user one when `None`), the MCP policy from
    /// the config file in `paths` and appends to the audit log in `paths`.
    #[must_use]
    pub fn new(
        secrets: Arc<dyn SecretStore>,
        state_path: Option<PathBuf>,
        paths: PolicyPaths,
    ) -> Self {
        Self {
            engine: None,
            secrets,
            state_path,
            paths,
            approvals: Approvals::disabled(),
        }
    }

    /// The same state asking `approvals` when the policy says `ask` (mcp-approvals S1). A
    /// new state has approvals off, so `ask` refuses.
    #[must_use]
    pub fn with_approvals(mut self, approvals: Approvals) -> Self {
        self.approvals = approvals;
        self
    }

    /// Where and how this state asks for approvals.
    pub(crate) fn approvals(&self) -> &Approvals {
        &self.approvals
    }

    /// Opens the startup workspace: `flag` when given, else `cwd` when it holds a
    /// `reqflux.toml`, else the first of `recents` (newest first) that loads, as the window
    /// does, else none (S1; user-config-mcp-policy S8).
    ///
    /// # Errors
    ///
    /// The workspace `flag` names does not load. A workspace in `cwd` that does not load is
    /// logged and left closed.
    pub fn open_startup(
        &mut self,
        flag: Option<&Path>,
        cwd: &Path,
        recents: &[PathBuf],
    ) -> Result<(), ToolError> {
        if let Some(root) = flag {
            return self.open(root);
        }
        if cwd.join("reqflux.toml").is_file() {
            if let Err(error) = self.open(cwd) {
                log::warn!(
                    target: "reqflux_mcp",
                    "the workspace in the current directory did not open: {error}"
                );
            }
            return Ok(());
        }
        for recent in recents {
            match self.open(recent) {
                Ok(()) => return Ok(()),
                Err(error) => log::info!(
                    target: "reqflux_mcp",
                    "skipping a recent workspace that does not load: {error}"
                ),
            }
        }
        Ok(())
    }

    /// Opens the workspace at `root`, keeping the open one when it fails.
    fn open(&mut self, root: &Path) -> Result<(), ToolError> {
        self.engine = Some(self.load(root)?);
        Ok(())
    }

    /// The workspace at `root`, loaded without opening it.
    fn load(&self, root: &Path) -> Result<Engine, ToolError> {
        Engine::open_with_secrets(root, Arc::clone(&self.secrets))
            .map_err(|error| ToolError::Engine(format!("{}: {error}", root.display())))
    }

    /// The warnings of the user's config file and the open workspace's manifest `[mcp]`:
    /// unknown keys and `[mcp.workspaces]` keys that are not workspace ids. The server
    /// prints them at startup; every refusal names them too.
    #[must_use]
    pub fn policy_warnings(&self) -> Vec<String> {
        policy::warnings(&self.paths, self.engine.as_ref())
    }

    /// The open engine as loaded, without reloading.
    fn engine(&mut self) -> Result<&mut Engine, ToolError> {
        self.engine.as_mut().ok_or(ToolError::NoWorkspace)
    }

    /// The open engine reloaded from disk, so a read sees the files' current state.
    fn reloaded(&mut self) -> Result<&mut Engine, ToolError> {
        let engine = self.engine()?;
        engine
            .reload()
            .map_err(|error| ToolError::Engine(format!("the workspace does not load: {error}")))?;
        Ok(engine)
    }
}

/// Arguments of `workspace_open`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArgs {
    /// The workspace directory, holding `reqflux.toml`.
    pub path: String,
}

/// Arguments of `workspace_create`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceCreateArgs {
    /// A new or empty directory.
    pub path: String,
    /// The workspace name.
    pub name: String,
}

/// Arguments of `collection_create`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CollectionCreateArgs {
    /// The name; `New collection` (numbered when taken) unless passed.
    pub name: Option<String>,
}

/// Arguments of `folder_create`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FolderCreateArgs {
    /// The id of the collection or folder to create it in.
    pub parent_id: String,
    /// The name; a default name unless passed.
    pub name: Option<String>,
}

/// Arguments of `request_create`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestCreateArgs {
    /// The id of the collection or folder to create it in.
    pub parent_id: String,
    /// The name; a default name unless passed.
    pub name: Option<String>,
    /// The HTTP method; `GET` unless passed.
    pub method: Option<String>,
    /// The URL; empty unless passed.
    pub url: Option<String>,
}

/// Arguments of the tools that take one item id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ItemArgs {
    /// The id of the collection, folder or request.
    pub id: String,
}

/// Arguments of `item_rename`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameArgs {
    /// The id of the collection, folder or request.
    pub id: String,
    /// The new name.
    pub name: String,
}

/// Arguments of `item_move`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveArgs {
    /// The id of the folder or request to move.
    pub id: String,
    /// The id of the collection or folder to move it into.
    pub parent_id: String,
    /// The position among the new siblings, from 0.
    pub index: usize,
}

/// Arguments of `request_update`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    /// The request id; the document must carry the same id.
    pub id: String,
    /// The whole request document as TOML, as `request_get` returns it. A masked auth
    /// secret or credential header value keeps the stored value.
    pub document: String,
    /// The `base_hash` `request_get` returned with the document: the save is refused as a
    /// conflict when the file changed since that read.
    pub base_hash: String,
}

/// Arguments of `request_execute`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteArgs {
    /// The request id.
    pub id: String,
    /// The environment name or slug; the collection's default unless passed.
    pub environment: Option<String>,
    /// Variable overrides for this send only.
    pub vars: Option<BTreeMap<String, String>>,
}

/// Arguments of `history_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryListArgs {
    /// Maximum entries, newest first; 20 unless passed, capped at 200.
    pub limit: Option<u32>,
}

/// Arguments of `history_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntryArgs {
    /// The entry id from `history_list`.
    pub entry_id: i64,
}

/// `workspace_open`: opens the workspace at `path` in place of the open one (S1).
///
/// # Errors
///
/// The workspace does not load, or the policy refuses reading it (its
/// `[mcp.workspaces."<id>"]` or manifest `[mcp]` denies read); the open one stays open.
pub fn workspace_open(state: &mut EngineState, args: PathArgs) -> Result<String, ToolError> {
    state.authorize("workspace_open", Scope::Global, &[])?;
    let engine = state.load(Path::new(&args.path))?;
    state.authorize_read_of("workspace_open", &engine)?;
    state.engine = Some(engine);
    summary(state.engine()?)
}

/// `workspace_create`: creates a workspace at `path` named `name` and opens it (S1).
///
/// # Errors
///
/// The engine refused the directory or the name, or the new workspace does not load.
pub fn workspace_create(
    state: &mut EngineState,
    args: WorkspaceCreateArgs,
) -> Result<String, ToolError> {
    state.authorize("workspace_create", Scope::Global, &[])?;
    let root = PathBuf::from(&args.path);
    create_workspace(&root, &args.name).map_err(|error| ToolError::Engine(error.to_string()))?;
    state.open(&root)?;
    summary(state.engine()?)
}

/// `workspace_info`: the open workspace's name, id, path, load warnings and tree (S1).
///
/// # Errors
///
/// No workspace is open, or it no longer loads.
pub fn workspace_info(state: &mut EngineState) -> Result<String, ToolError> {
    state.authorize("workspace_info", Scope::Open, &[])?;
    let engine = state.reloaded()?;
    let mut value = describe(engine);
    if let Value::Object(map) = &mut value {
        map.insert("tree".to_owned(), Value::Array(tree(engine.workspace())));
    }
    encode(&value)
}

/// The name, id, path and warnings of `engine`'s workspace, as JSON text.
fn summary(engine: &Engine) -> Result<String, ToolError> {
    encode(&describe(engine))
}

fn describe(engine: &Engine) -> Value {
    let manifest = &engine.workspace().manifest;
    let warnings: Vec<String> = engine.warnings().iter().map(ToString::to_string).collect();
    json!({
        "name": manifest.name,
        "id": manifest.id.to_string(),
        "path": engine.root().display().to_string(),
        "warnings": warnings,
    })
}

/// Every collection, folder and request, depth first in display order, each naming its
/// parent's id.
fn tree(workspace: &Workspace) -> Vec<Value> {
    let mut nodes = Vec::new();
    for collection in &workspace.collections {
        nodes.push(node(
            collection.collection.id,
            &collection.collection.name,
            "collection",
            None,
            None,
        ));
        items(&collection.items, collection.collection.id, &mut nodes);
    }
    nodes
}

fn items(items: &[Item], parent: Id, nodes: &mut Vec<Value>) {
    for item in items {
        match item {
            Item::Folder(folder) => {
                nodes.push(node(
                    folder.folder.id,
                    &folder.folder.name,
                    "folder",
                    None,
                    Some(parent),
                ));
                self::items(&folder.items, folder.folder.id, nodes);
            }
            Item::Request(request) => nodes.push(node(
                request.request.id,
                &request.request.name,
                "request",
                Some(request.request.method.as_str()),
                Some(parent),
            )),
        }
    }
}

fn node(id: Id, name: &str, kind: &str, method: Option<&str>, parent: Option<Id>) -> Value {
    json!({
        "id": id.to_string(),
        "name": name,
        "kind": kind,
        "method": method,
        "parent": parent.map(|parent| parent.to_string()),
    })
}

/// Parses an entity id argument.
fn parse_id(text: &str) -> Result<Id, ToolError> {
    text.parse()
        .map_err(|error| ToolError::Invalid(format!("invalid id `{text}`: {error}")))
}

/// `{"id": <id>}`, the result of a tree tool.
fn id_result(id: Id) -> Result<String, ToolError> {
    encode(&json!({ "id": id.to_string() }))
}

fn encode(value: &Value) -> Result<String, ToolError> {
    Ok(serde_json::to_string(value)?)
}

/// The longest text output, in bytes, before it is cut.
const MAX_BODY_TEXT: usize = 256 * 1024;

/// `text` cut to at most [`MAX_BODY_TEXT`] bytes on a character boundary, and whether it
/// was cut.
fn cut(mut text: String) -> (String, bool) {
    if text.len() <= MAX_BODY_TEXT {
        return (text, false);
    }
    let end = (0..=MAX_BODY_TEXT)
        .rev()
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(0);
    text.truncate(end);
    (text, true)
}
