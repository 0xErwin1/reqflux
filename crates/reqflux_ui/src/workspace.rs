//! The workspace the main window shows, opened through `reqflux_engine`.

use std::path::PathBuf;
use std::sync::Arc;

use reqflux_engine::{Engine, KeyringSecretStore, MemorySecretStore, SecretStore};

/// The text the document island shows when no workspace is open.
pub(crate) const OPEN_HINT: &str = "Open a workspace: reqflux <directory>";

/// The OS keyring in the app; an empty in-memory store in tests, which never touch the real
/// keyring (keyring-secrets S8).
pub(crate) fn secret_store() -> Arc<dyn SecretStore> {
    if cfg!(test) {
        Arc::new(MemorySecretStore::new())
    } else {
        Arc::new(KeyringSecretStore)
    }
}

/// The outcome of opening the workspace of a window.
#[derive(Debug)]
pub(crate) enum OpenedWorkspace {
    /// No workspace was requested.
    None,
    /// The workspace at `path` did not load.
    Failed {
        /// The directory that was opened.
        path: PathBuf,
        /// Why it did not load.
        error: String,
    },
    /// The workspace loaded. Shared, so a send can run the engine's `execute` — whose
    /// pre-request script can take seconds — on a background executor task that owns it.
    Loaded(Arc<Engine>),
}

impl OpenedWorkspace {
    /// Opens the workspace at `dir`, if any.
    ///
    /// This loads the whole workspace from disk on the calling thread, so the window opens
    /// only after it. Loading is fast enough for this; a large workspace will need the load
    /// to move to a background task with a loading state.
    pub(crate) fn open(dir: Option<PathBuf>) -> Self {
        let Some(path) = dir else {
            return Self::None;
        };
        match Engine::open_with_secrets(path.clone(), secret_store()) {
            Ok(engine) => Self::Loaded(Arc::new(engine)),
            Err(error) => {
                log::warn!("could not open workspace {}: {error}", path.display());
                Self::Failed {
                    path,
                    error: error.to_string(),
                }
            }
        }
    }

    /// The display name of the loaded workspace.
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Self::Loaded(engine) => Some(engine.workspace().manifest.name.as_str()),
            Self::None | Self::Failed { .. } => None,
        }
    }

    /// The number of non-fatal warnings the workspace load reported.
    pub(crate) fn warning_count(&self) -> usize {
        match self {
            Self::Loaded(engine) => engine.warnings().len(),
            Self::None | Self::Failed { .. } => 0,
        }
    }

    /// The text the document island shows in place of a document, if any.
    pub(crate) fn document_message(&self) -> Option<String> {
        match self {
            Self::None => Some(OPEN_HINT.to_owned()),
            Self::Failed { path, error } => {
                Some(format!("Could not open {}: {error}", path.display()))
            }
            Self::Loaded(_) => None,
        }
    }
}
