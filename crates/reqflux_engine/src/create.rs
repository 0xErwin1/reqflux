//! Creating a workspace (recent-workspaces S2).

use std::fs;
use std::io;
use std::path::Path;

use reqflux_core::model::{Id, WorkspaceManifest};
use reqflux_store::{FORMAT, save_manifest};

use crate::error::CreateWorkspaceError;

/// Creates a workspace in `dir` named `name` and returns its new id.
///
/// `dir` must not exist (it is created, parents too) or be an empty directory. The
/// `reqflux.toml` written through the store holds a new id, the name and the current
/// format, so [`Engine::open`](crate::Engine::open) loads it with no warnings.
///
/// # Errors
///
/// An empty or blank name, a `dir` that is a file, already holds `reqflux.toml` or is not
/// empty: each is refused before anything is written. Creating the directory or saving the
/// manifest can also fail.
pub fn create_workspace(dir: &Path, name: &str) -> Result<Id, CreateWorkspaceError> {
    let id = Id::new();
    create_workspace_with_id(dir, name, id)?;
    Ok(id)
}

/// [`create_workspace`] with the id the manifest takes.
pub(crate) fn create_workspace_with_id(
    dir: &Path,
    name: &str,
    id: Id,
) -> Result<(), CreateWorkspaceError> {
    if name.trim().is_empty() {
        return Err(CreateWorkspaceError::EmptyName);
    }
    let io_error = |source| CreateWorkspaceError::Io {
        path: dir.to_owned(),
        source,
    };
    match fs::metadata(dir) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(CreateWorkspaceError::NotADirectory(dir.to_owned()));
        }
        Ok(_) => {
            if dir.join("reqflux.toml").exists() {
                return Err(CreateWorkspaceError::AlreadyWorkspace(dir.to_owned()));
            }
            if fs::read_dir(dir).map_err(io_error)?.next().is_some() {
                return Err(CreateWorkspaceError::NotEmpty(dir.to_owned()));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).map_err(io_error)?;
        }
        Err(source) => return Err(io_error(source)),
    }
    let manifest = WorkspaceManifest {
        id,
        name: name.to_owned(),
        format: FORMAT,
        globals: Vec::new(),
        extensions: toml::Table::new(),
    };
    save_manifest(dir, &manifest, None)?;
    Ok(())
}
