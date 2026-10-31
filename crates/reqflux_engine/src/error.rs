//! Engine errors.

use std::path::PathBuf;

use reqflux_http::HttpError;
use reqflux_store::StoreError;

use crate::secrets::SecretError;

/// Why a workspace could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The workspace does not load.
    #[error(transparent)]
    Load(#[from] StoreError),
    /// The Tokio runtime could not start.
    #[error("could not start the async runtime: {0}")]
    Runtime(std::io::Error),
}

/// Why a workspace could not be created.
#[derive(Debug, thiserror::Error)]
pub enum CreateWorkspaceError {
    /// The name is empty or blank.
    #[error("a workspace needs a name")]
    EmptyName,
    /// The target exists and is not a directory.
    #[error("{} is not a directory", .0.display())]
    NotADirectory(PathBuf),
    /// The directory already holds a `reqflux.toml`.
    #[error("{} is already a workspace", .0.display())]
    AlreadyWorkspace(PathBuf),
    /// The directory holds other files.
    #[error("{} is not empty; a new workspace needs a missing or empty directory", .0.display())]
    NotEmpty(PathBuf),
    /// Inspecting or creating the directory failed.
    #[error("could not prepare {}: {source}", path.display())]
    Io {
        /// The directory involved.
        path: PathBuf,
        /// The underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// The store could not write `reqflux.toml`.
    #[error(transparent)]
    Save(#[from] StoreError),
    /// Neither `XDG_DATA_HOME` nor `HOME` is set, so there is nowhere to keep workspaces.
    #[error("neither XDG_DATA_HOME nor HOME is set; cannot locate the app's workspaces")]
    NoDataDirectory,
}

/// Why a request could not be prepared or sent.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    /// No request has this id or slug path.
    #[error("no request matches {0}")]
    UnknownRequest(String),
    /// No workspace environment has this slug.
    #[error("unknown environment {0}")]
    UnknownEnvironment(String),
    /// The collection's environment allowlist excludes the environment.
    #[error("environment {environment} is not allowed for collection {collection}")]
    EnvironmentNotAllowed {
        /// The environment slug.
        environment: String,
        /// The collection name.
        collection: String,
    },
    /// The effective auth is a type the engine cannot send yet.
    #[error("auth type {0} is not supported yet")]
    UnsupportedAuth(String),
    /// The GraphQL variables do not parse as JSON.
    #[error("graphql variables are not valid JSON: {0}")]
    GraphqlVariables(String),
    /// The transport failed.
    #[error(transparent)]
    Http(#[from] HttpError),
    /// The execution task panicked or was aborted.
    #[error("execution task failed: {0}")]
    Task(String),
    /// The store failed, for example when a slug from the workspace cannot form a path.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Why a Postman import could not be written (S9).
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The document does not map to the ReqFlux model (S8).
    #[error(transparent)]
    Postman(#[from] reqflux_postman::PostmanError),
    /// The store failed to write `path`.
    #[error("{}: {message}", path.display())]
    Store {
        /// The file being written.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The file exists; an import never overwrites (S9).
    #[error("{} already exists; an import never overwrites a file", path.display())]
    Exists {
        /// The existing file.
        path: PathBuf,
    },
}

/// Why a request document could not be saved back to its file.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The file changed since the workspace was loaded; the draft is kept (S6).
    #[error("{} changed on disk since the workspace was loaded", path.display())]
    Conflict {
        /// The file that was left untouched.
        path: PathBuf,
    },
    /// No request has this id or slug path; nothing was written.
    #[error("no request matches {0}")]
    UnknownRequest(String),
    /// The store failed for any other reason.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Why a scope variable edit failed (variables-editor S1, S2). No message contains a
/// variable's value.
#[derive(Debug, thiserror::Error)]
pub enum VariableError {
    /// The store refused the edit; nothing changed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The secret store refused the edit before the file was written; nothing changed.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// The file was written, but the old secret store entry could not be deleted. The edit
    /// stands; the entry `account` is left in the store.
    #[error(
        "the variable was saved, but the keyring entry {account} could not be deleted: {source}"
    )]
    SecretCleanup {
        /// The keyring account left behind.
        account: String,
        /// The secret store error.
        source: SecretError,
    },
    /// The file write failed after a new secret store entry was set, and deleting that entry
    /// failed too: the file is unchanged but the entry `account` is left in the store.
    #[error(
        "{source}; the keyring entry {account} written for it could not be deleted: {rollback}"
    )]
    Orphaned {
        /// Why the file was not written.
        source: Box<StoreError>,
        /// The keyring account left behind.
        account: String,
        /// Why deleting it failed.
        rollback: SecretError,
    },
    /// The edit was written but the workspace no longer loads; the engine keeps the
    /// workspace it had.
    #[error("the workspace does not reload after the change: {0}")]
    Reload(#[source] OpenError),
}

/// Why a tree operation (create, rename, duplicate, delete, move, reorder) failed.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    /// The store refused the operation; nothing on disk changed and the engine keeps the
    /// workspace it had.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The operation was written but the workspace no longer loads; the engine keeps the
    /// workspace it had.
    #[error("the workspace does not reload after the change: {0}")]
    Reload(#[source] OpenError),
}

impl ExecuteError {
    /// Whether the error comes from what the caller asked for (an unknown request or
    /// environment) rather than from executing it.
    #[must_use]
    pub fn is_usage(&self) -> bool {
        matches!(self, Self::UnknownRequest(_) | Self::UnknownEnvironment(_))
    }
}

/// Why script writes could not be persisted as current values (variables-editor S5). The
/// message names keys and keyring accounts, never a value.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// The request or the environment of the send no longer resolves.
    #[error(transparent)]
    Execute(#[from] ExecuteError),
    /// The app state refused a current value.
    #[error(transparent)]
    State(#[from] crate::state::StateError),
    /// The secret store refused the value of a secret variable.
    #[error("could not store the secret {account}: {source}")]
    Secret {
        /// The keyring account of the secret.
        account: String,
        /// The store failure.
        #[source]
        source: SecretError,
    },
}
