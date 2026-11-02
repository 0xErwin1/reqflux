//! Atomic, hash-checked saves (S7).
//!
//! Every save takes `expected`: the [`ContentHash`] of the bytes the caller last read from
//! the target file, or `None` to create a new file. The save fails with
//! [`StoreError::Conflict`] and leaves the file untouched when the file on disk no longer
//! matches: different bytes, deleted since it was read, or present when creating. The new
//! bytes go to a temporary file in the same directory that is then renamed over the target,
//! so a reader never sees a partial file. The check and the rename are not locked together:
//! a write landing between them is overwritten, a window of one rename.
//!
//! Scripts are sidecar files with their own hashes. [`save_scripts`] checks both sidecars
//! before touching either, then writes a non-empty script and deletes the sidecar of an
//! empty or absent one, so the workspace never holds an empty `.js` file.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use reqflux_core::model::{Collection, Environment, Folder, Request, Scripts, WorkspaceManifest};
use serde::Serialize;
use tempfile::NamedTempFile;

use crate::error::StoreError;
use crate::hash::ContentHash;
use crate::layout::{
    Parent, ScriptOwner, collection_file, environment_file, folder_file, manifest_file,
    request_file, validate_parent, validate_request_slug, validate_slug,
};

/// The hashes of an entity's two sidecar scripts. `None` means no file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScriptHashes {
    /// The pre-request script file.
    pub pre_request: Option<ContentHash>,
    /// The test script file.
    pub test: Option<ContentHash>,
}

/// Saves `reqflux.toml`.
///
/// # Errors
///
/// [`StoreError::Conflict`] when the file does not match `expected`, or an IO or
/// serialization error naming the file.
pub fn save_manifest(
    root: &Path,
    manifest: &WorkspaceManifest,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    save_document(&manifest_file(root), manifest, expected)
}

/// Saves `environments/<slug>.env.toml`. Secret values are never written (S10).
///
/// # Errors
///
/// As [`save_manifest`], plus [`StoreError::InvalidSlug`].
pub fn save_environment(
    root: &Path,
    slug: &str,
    environment: &Environment,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    validate_slug(slug)?;
    save_document(&environment_file(root, slug), environment, expected)
}

/// Saves `collections/<slug>/collection.toml`, creating the directory if needed.
///
/// # Errors
///
/// As [`save_environment`].
pub fn save_collection(
    root: &Path,
    slug: &str,
    collection: &Collection,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    validate_slug(slug)?;
    save_document(&collection_file(root, slug), collection, expected)
}

/// Saves `<parent>/<slug>/folder.toml`, creating the directory if needed.
///
/// # Errors
///
/// As [`save_environment`], plus [`StoreError::SlugCollision`] when
/// `<parent>/<slug>.req.toml` already exists: a request and a folder cannot share a
/// slug, because `Collection::order` holds slugs only.
pub fn save_folder(
    root: &Path,
    parent: &Parent,
    slug: &str,
    folder: &Folder,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    validate_parent(parent)?;
    validate_slug(slug)?;
    refuse_shared_slug(request_file(root, parent, slug), slug, "request")?;
    save_document(&folder_file(root, parent, slug), folder, expected)
}

/// Saves `<parent>/<slug>.req.toml`.
///
/// # Errors
///
/// As [`save_environment`], plus [`StoreError::SlugCollision`] when the folder directory
/// `<parent>/<slug>` already exists: a request and a folder cannot share a slug, because
/// `Collection::order` holds slugs only.
pub fn save_request(
    root: &Path,
    parent: &Parent,
    slug: &str,
    request: &Request,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    validate_parent(parent)?;
    validate_request_slug(slug)?;
    refuse_shared_slug(parent.dir(root).join(slug), slug, "folder")?;
    save_document(&request_file(root, parent, slug), request, expected)
}

/// Fails with [`StoreError::SlugCollision`] when `path`, the sibling entity that would
/// share `slug` with the one being saved, already exists. The check looks at the symlink
/// itself, so a broken symlink holding the slug is refused too.
fn refuse_shared_slug(path: PathBuf, slug: &str, held: &'static str) -> Result<(), StoreError> {
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(StoreError::SlugCollision {
            path,
            slug: slug.to_owned(),
            held,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(&path, source)),
    }
}

/// Saves the sidecar scripts of `owner`. Each file is checked against its own expected
/// hash; on any conflict neither file is touched. Returns the hashes of the files as they
/// are now, `None` for a deleted or absent sidecar.
///
/// # Errors
///
/// As [`save_environment`].
pub fn save_scripts(
    root: &Path,
    owner: &ScriptOwner,
    scripts: &Scripts,
    expected: ScriptHashes,
) -> Result<ScriptHashes, StoreError> {
    owner.validate()?;
    let files = owner.files(root);
    check(&files.pre_request, expected.pre_request)?;
    check(&files.test, expected.test)?;
    Ok(ScriptHashes {
        pre_request: write_script(&files.pre_request, scripts.pre_request.as_deref())?,
        test: write_script(&files.test, scripts.test.as_deref())?,
    })
}

pub(crate) fn write_script(
    path: &Path,
    script: Option<&str>,
) -> Result<Option<ContentHash>, StoreError> {
    match script.filter(|text| !text.is_empty()) {
        Some(text) => write_atomic(path, text.as_bytes()).map(Some),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(io_error(path, source)),
        },
    }
}

pub(crate) fn save_document<T: Serialize>(
    path: &Path,
    document: &T,
    expected: Option<ContentHash>,
) -> Result<ContentHash, StoreError> {
    let text = toml::to_string(document).map_err(|source| StoreError::Serialize {
        path: path.to_owned(),
        source,
    })?;
    check(path, expected)?;
    write_atomic(path, text.as_bytes())
}

/// Fails with a conflict unless the file matches `expected` (`None`: does not exist).
pub(crate) fn check(path: &Path, expected: Option<ContentHash>) -> Result<(), StoreError> {
    let current = match fs::read(path) {
        Ok(bytes) => Some(ContentHash::of(&bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(source) => return Err(io_error(path, source)),
    };
    if current == expected {
        Ok(())
    } else {
        Err(StoreError::Conflict {
            path: path.to_owned(),
        })
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<ContentHash, StoreError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| io_error(dir, source))?;
    let mut file = NamedTempFile::new_in(dir).map_err(|source| io_error(dir, source))?;
    file.write_all(bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|source| io_error(path, source))?;
    set_permissions(path, &file)?;
    file.persist(path)
        .map_err(|error| io_error(path, error.error))?;
    sync_parent(dir)?;
    Ok(ContentHash::of(bytes))
}

/// The file fsync makes the bytes durable but not the rename: the directory entry itself
/// needs a fsync, so the parent directory is synced too. Only unix has a directory fsync;
/// elsewhere there is none and the rename keeps the platform's default durability.
#[cfg(unix)]
fn sync_parent(dir: &Path) -> Result<(), StoreError> {
    fs::File::open(dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|source| io_error(dir, source))
}

#[cfg(not(unix))]
fn sync_parent(_dir: &Path) -> Result<(), StoreError> {
    Ok(())
}

/// The temporary file is private; give the target the permissions it had, or the usual
/// mode for a new file.
fn set_permissions(path: &Path, file: &NamedTempFile) -> Result<(), StoreError> {
    let permissions = match fs::metadata(path) {
        Ok(metadata) => metadata.permissions(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => new_file_permissions(file)?,
        Err(source) => return Err(io_error(path, source)),
    };
    file.as_file()
        .set_permissions(permissions)
        .map_err(|source| io_error(path, source))
}

#[cfg(unix)]
fn new_file_permissions(_file: &NamedTempFile) -> Result<fs::Permissions, StoreError> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
fn new_file_permissions(file: &NamedTempFile) -> Result<fs::Permissions, StoreError> {
    file.as_file()
        .metadata()
        .map(|metadata| metadata.permissions())
        .map_err(|source| io_error(file.path(), source))
}

pub(crate) fn io_error(path: &Path, source: io::Error) -> StoreError {
    StoreError::Io {
        path: path.to_owned(),
        source,
    }
}
