//! The execution pipeline, collection runner, history and secrets.
//!
//! This is the single API the UI, the MCP server and the CLI use.
//!
//! # Executing a request
//!
//! [`Engine::open`] loads a workspace. [`Engine::prepare`] turns a request of it into a
//! [`PreparedRequest`](reqflux_http::PreparedRequest): it selects the request (by id or slug
//! path), checks the environment, resolves `{{variables}}`, builds the query string, applies
//! the effective auth, builds the body and fills in default settings. [`prepare`] is the same
//! step as a pure function, with no IO and no network. [`prepare_draft`] does the same for
//! a draft request document in the place of the stored one, keeping its parents.
//! [`Engine::send`] sends a prepared request; [`Engine::execute`] does both, after running
//! the request's effective pre-request script (see the `Scripting` section of
//! `ARCHITECTURE.md`): the most specific script on the request, its folders or the
//! collection, whose variable writes apply to that execution's scopes only.
//! Every script runs on its own thread with a current-thread Tokio runtime (S14), so a
//! script that waits for the network never blocks a front end's executor.
//! [`Engine::wait`] runs the effective test script once the response arrives and returns
//! its results beside the response in [`Executed::scripts`].
//!
//! # Secrets
//!
//! Secret variable values never live in workspace files. The engine reads the valueless
//! secrets in scope from its [`SecretStore`] (the OS keyring by default; see
//! [`Engine::open_with_secrets`]) when it prepares, executes or inspects a request. `--var`,
//! local and script overrides still win. A store that cannot be read leaves those secrets
//! unresolved and says why in [`Prepared::secret_problem`]; it never fails the send.
//!
//! # Threading model
//!
//! The [`Engine`] owns a multi-thread Tokio runtime and every request runs on it as a spawned
//! task. [`Engine::send`] returns an [`Execution`] holding the task's
//! [`JoinHandle`](tokio::task::JoinHandle), which is an executor-agnostic future: a GPUI
//! front end awaits it on its own executor, so no GPUI code runs on Tokio and no HTTP code
//! runs on GPUI's executor. An async front end that also wants the script results calls
//! [`Execution::finish`], which awaits the handle and runs the effective test script with
//! what the execution already carries — no engine borrow needed. Synchronous callers such
//! as the CLI use [`Engine::wait`] or [`Engine::execute_blocking`], which block the calling thread and must not be called from
//! inside an async context. Dropping the engine shuts the runtime down, so it must not be
//! dropped from inside an async context either.
//!
//! # Creating a workspace
//!
//! [`create_workspace`] writes a new workspace's `reqflux.toml` into a missing or empty
//! directory. [`create_managed_workspace`] creates one the app keeps, under
//! [`managed_workspaces_dir`].
//!
//! # App state
//!
//! [`state::AppState`] is the per-user SQLite app state (drafts, the send history, current
//! values and the recent workspaces), at
//! `$XDG_STATE_HOME/reqflux/state.sqlite3`.
//!
//! # User configuration
//!
//! [`config`] reads and edits the user's config file,
//! `$XDG_CONFIG_HOME/reqflux/config.toml`: the theme and the MCP policy.

pub mod audit;
pub mod config;
mod create;
mod current;
mod engine;
mod error;
mod guard;
mod import;
mod managed;
pub mod policy;
mod prepare;
mod scripts;
mod secrets;
mod select;
mod sent;
pub mod state;
mod tree;
mod variables;

pub use create::create_workspace;
pub use current::CurrentValues;
pub use engine::{Engine, Executed, Execution, PendingExecution};
pub use error::{
    CreateWorkspaceError, ExecuteError, ImportError, OpenError, PersistError, SaveError, TreeError,
    VariableError,
};
pub use guard::HostGuard;
pub use import::{ImportSummary, postman};
pub use managed::{
    create_managed_workspace, create_managed_workspace_in, is_managed, is_managed_in,
    managed_workspaces_dir,
};
pub use prepare::{
    DEFAULT_TIMEOUT_MS, ExecuteOptions, Inspection, Prepared, Span, inspect, prepare, prepare_draft,
};
/// The transport types front ends read, re-exported so they depend on the engine only.
pub use reqflux_http::{HttpError, PreparedBody, PreparedRequest, Response};
pub use reqflux_script::{
    ConsoleEntry, ConsoleLevel, ScriptOutcome, TestResult, VariableScope, VariableWrite,
};
/// The file hash a hash-checked save compares against, and the parse error text that never
/// quotes the source, re-exported so front ends depend on the engine only.
pub use reqflux_store::{ContentHash, toml_error_location};
pub use secrets::{
    KeyringSecretStore, MemorySecretStore, SECRET_SERVICE, SecretError, SecretKey, SecretScope,
    SecretStore,
};
pub use sent::{MASK, Masker, SentBody, SentRequest, is_credential_header};
/// The token type of [`Execution::cancel`], re-exported so front ends can hold it.
pub use tokio_util::sync::CancellationToken;
pub use variables::VariableEdit;
