//! Workspaces the app keeps (standalone-workspaces S1): each lives in
//! `$XDG_DATA_HOME/reqflux/workspaces/<workspace id>/` (`$HOME/.local/share/...` when the
//! variable is unset), in the same on-disk format as any workspace. The directory is named by
//! the id, so a rename never moves files.

use std::fs;
use std::path::{Path, PathBuf};

use reqflux_core::model::Id;

use crate::create::create_workspace_with_id;
use crate::error::CreateWorkspaceError;

/// The managed workspaces directory, from the given variable values. An absolute
/// `XDG_DATA_HOME` wins; otherwise it lives under `home`. `None` means there is no home.
pub(crate) fn managed_workspaces_path(
    data_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let data_home = data_home.filter(|candidate| candidate.is_absolute());
    let base = match data_home {
        Some(data_home) => data_home.to_path_buf(),
        None => home?.join(".local/share"),
    };
    Some(base.join("reqflux/workspaces"))
}

/// The directory the app keeps its workspaces in: `$XDG_DATA_HOME/reqflux/workspaces`, or
/// `$HOME/.local/share/reqflux/workspaces` when the variable is unset. Nothing is created.
///
/// # Errors
///
/// [`CreateWorkspaceError::NoDataDirectory`] when neither variable is set.
pub fn managed_workspaces_dir() -> Result<PathBuf, CreateWorkspaceError> {
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    managed_workspaces_path(data_home.as_deref(), home.as_deref())
        .ok_or(CreateWorkspaceError::NoDataDirectory)
}

/// Creates a workspace named `name` in [`managed_workspaces_dir`] and returns its id and
/// directory.
///
/// # Errors
///
/// No data directory, or any failure of [`create_managed_workspace_in`].
pub fn create_managed_workspace(name: &str) -> Result<(Id, PathBuf), CreateWorkspaceError> {
    create_managed_workspace_in(&managed_workspaces_dir()?, name)
}

/// Creates a workspace named `name` in `<root>/<new id>` through
/// [`create_workspace`](crate::create_workspace) and returns its id and directory.
///
/// # Errors
///
/// An empty or blank name is refused before anything is written; creating the directory or
/// saving the manifest can also fail.
pub fn create_managed_workspace_in(
    root: &Path,
    name: &str,
) -> Result<(Id, PathBuf), CreateWorkspaceError> {
    let id = Id::new();
    let dir = root.join(id.to_string());
    create_workspace_with_id(&dir, name, id)?;
    Ok((id, dir))
}

/// Whether `path` is a workspace the app keeps: a directory under
/// [`managed_workspaces_dir`], after canonicalizing both.
pub fn is_managed(path: &Path) -> bool {
    managed_workspaces_dir().is_ok_and(|root| is_managed_in(&root, path))
}

/// Whether `path` is a directory strictly under `root`, after canonicalizing both; a path
/// that does not exist is not managed.
pub fn is_managed_in(root: &Path, path: &Path) -> bool {
    let (Ok(root), Ok(path)) = (fs::canonicalize(root), fs::canonicalize(path)) else {
        return false;
    };
    path != root && path.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CreateWorkspaceError;

    fn temp_dir() -> tempfile::TempDir {
        match tempfile::TempDir::new() {
            Ok(dir) => dir,
            Err(error) => panic!("temp dir: {error}"),
        }
    }

    #[test]
    fn the_managed_dir_prefers_xdg_data_home() {
        assert_eq!(
            managed_workspaces_path(Some(Path::new("/tmp/data")), Some(Path::new("/home/u"))),
            Some(PathBuf::from("/tmp/data/reqflux/workspaces"))
        );
    }

    #[test]
    fn the_managed_dir_falls_back_to_home_and_needs_one() {
        assert_eq!(
            managed_workspaces_path(None, Some(Path::new("/home/u"))),
            Some(PathBuf::from("/home/u/.local/share/reqflux/workspaces"))
        );
        assert_eq!(
            managed_workspaces_path(Some(Path::new("relative")), Some(Path::new("/home/u"))),
            Some(PathBuf::from("/home/u/.local/share/reqflux/workspaces"))
        );
        assert_eq!(managed_workspaces_path(None, None), None);
    }

    #[test]
    fn a_managed_workspace_lives_in_a_directory_named_by_its_id() {
        let root = temp_dir();
        let (id, dir) = match create_managed_workspace_in(root.path(), "Personal") {
            Ok(created) => created,
            Err(error) => panic!("create: {error}"),
        };
        assert_eq!(dir, root.path().join(id.to_string()));
        let manifest = match std::fs::read_to_string(dir.join("reqflux.toml")) {
            Ok(text) => text,
            Err(error) => panic!("read: {error}"),
        };
        assert!(manifest.contains("name = \"Personal\""), "{manifest}");
        assert!(manifest.contains(&format!("id = \"{id}\"")), "{manifest}");
        assert!(is_managed_in(root.path(), &dir));
        assert!(!is_managed_in(root.path(), root.path()));
    }

    #[test]
    fn a_managed_workspace_needs_a_name() {
        let root = temp_dir();
        assert!(matches!(
            create_managed_workspace_in(root.path(), "  "),
            Err(CreateWorkspaceError::EmptyName)
        ));
        let entries = match std::fs::read_dir(root.path()) {
            Ok(entries) => entries.count(),
            Err(error) => panic!("read_dir: {error}"),
        };
        assert_eq!(entries, 0);
    }

    #[test]
    fn a_folder_elsewhere_is_not_managed() {
        let root = temp_dir();
        let other = temp_dir();
        assert!(!is_managed_in(root.path(), other.path()));
        assert!(!is_managed_in(&root.path().join("missing"), other.path()));
    }
}
