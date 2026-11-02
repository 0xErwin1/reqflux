//! Where each entity lives on disk (S4).
//!
//! ```text
//! <root>/reqflux.toml
//! <root>/environments/<slug>.env.toml
//! <root>/collections/<slug>/collection.toml   (+ collection.pre.js, collection.test.js)
//! <root>/collections/<slug>/.../<folder>/folder.toml   (+ folder.pre.js, folder.test.js)
//! <root>/collections/<slug>/.../<slug>.req.toml   (+ <slug>.pre.js, <slug>.test.js)
//! ```
//!
//! The public helpers that compose a path from a slug are the `checked_` ones: they
//! validate every slug and fail with a [`StoreError`] instead of ever building a path
//! outside the root. The raw, validation-free composers of the same names are
//! `pub(crate)`, for the store itself at the points where the slug was just validated.

use std::path::{Path, PathBuf};

use crate::error::StoreError;

pub(crate) const MANIFEST: &str = "reqflux.toml";
pub(crate) const ENVIRONMENTS: &str = "environments";
pub(crate) const COLLECTIONS: &str = "collections";
pub(crate) const COLLECTION_FILE: &str = "collection.toml";
pub(crate) const FOLDER_FILE: &str = "folder.toml";
pub(crate) const ENV_SUFFIX: &str = ".env.toml";
pub(crate) const REQUEST_SUFFIX: &str = ".req.toml";
pub(crate) const PRE_SUFFIX: &str = ".pre.js";
pub(crate) const TEST_SUFFIX: &str = ".test.js";
/// The directory of a request's saved examples, `<request-slug>.examples`; the load skips
/// it silently.
pub(crate) const EXAMPLES_SUFFIX: &str = ".examples";
/// A saved example inside it, `<example-slug>.example.toml`.
pub(crate) const EXAMPLE_SUFFIX: &str = ".example.toml";
/// The body file of a saved example whose body is not text, `<example-slug>.body`.
pub(crate) const EXAMPLE_BODY_SUFFIX: &str = ".body";

/// The container of a folder or request: a collection and the folders below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parent {
    /// The collection slug.
    pub collection: String,
    /// Folder slugs from the collection down, outermost first.
    pub folders: Vec<String>,
}

impl Parent {
    /// The top level of collection `slug`.
    #[must_use]
    pub fn collection(slug: impl Into<String>) -> Self {
        Self {
            collection: slug.into(),
            folders: Vec::new(),
        }
    }

    /// The folder `slug` inside this parent.
    #[must_use]
    pub fn child(&self, slug: impl Into<String>) -> Self {
        let mut folders = self.folders.clone();
        folders.push(slug.into());
        Self {
            collection: self.collection.clone(),
            folders,
        }
    }

    /// The directory of this parent under `root`. Raw: the slugs must already have been
    /// validated; see [`Self::collection`], [`Self::child`] and the `checked_` helpers.
    #[must_use]
    pub(crate) fn dir(&self, root: &Path) -> PathBuf {
        let mut dir = collection_dir(root, &self.collection);
        dir.extend(&self.folders);
        dir
    }

    fn validate(&self) -> Result<(), StoreError> {
        validate_slug(&self.collection)?;
        self.folders.iter().try_for_each(|slug| validate_slug(slug))
    }
}

/// The entity whose sidecar scripts are meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptOwner {
    /// Collection `slug`: `collection.pre.js`, `collection.test.js`.
    Collection(String),
    /// Folder `slug` in a parent: `folder.pre.js`, `folder.test.js`.
    Folder(Parent, String),
    /// Request `slug` in a parent: `<slug>.pre.js`, `<slug>.test.js`.
    Request(Parent, String),
}

/// The two sidecar script paths of an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFiles {
    /// The pre-request script.
    pub pre_request: PathBuf,
    /// The test script.
    pub test: PathBuf,
}

impl ScriptOwner {
    /// The sidecar paths under `root`.
    #[must_use]
    pub fn files(&self, root: &Path) -> ScriptFiles {
        let (dir, stem) = match self {
            Self::Collection(slug) => (collection_dir(root, slug), "collection"),
            Self::Folder(parent, slug) => (parent.dir(root).join(slug), "folder"),
            Self::Request(parent, slug) => (parent.dir(root), slug.as_str()),
        };
        script_files(&dir, stem)
    }

    pub(crate) fn validate(&self) -> Result<(), StoreError> {
        match self {
            Self::Collection(slug) => validate_slug(slug),
            Self::Folder(parent, slug) => {
                parent.validate()?;
                validate_slug(slug)
            }
            Self::Request(parent, slug) => {
                parent.validate()?;
                validate_request_slug(slug)
            }
        }
    }
}

pub(crate) fn script_files(dir: &Path, stem: &str) -> ScriptFiles {
    ScriptFiles {
        pre_request: dir.join(format!("{stem}{PRE_SUFFIX}")),
        test: dir.join(format!("{stem}{TEST_SUFFIX}")),
    }
}

/// `<root>/reqflux.toml`.
#[must_use]
pub fn manifest_file(root: &Path) -> PathBuf {
    root.join(MANIFEST)
}

/// `<root>/environments/<slug>.env.toml`. Raw: `slug` must already have been validated.
#[must_use]
pub(crate) fn environment_file(root: &Path, slug: &str) -> PathBuf {
    environment_file_in(&environments_dir(root), slug)
}

/// `<root>/environments`.
pub(crate) fn environments_dir(root: &Path) -> PathBuf {
    root.join(ENVIRONMENTS)
}

/// `<dir>/<slug>.env.toml`. Raw: `slug` must already have been validated.
pub(crate) fn environment_file_in(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{slug}{ENV_SUFFIX}"))
}

/// `<root>/environments/<slug>.env.toml` for a validated slug.
///
/// # Errors
///
/// [`StoreError::InvalidSlug`] when `slug` is not a single, visible path component.
pub fn checked_environment_file(root: &Path, slug: &str) -> Result<PathBuf, StoreError> {
    validate_slug(slug)?;
    Ok(environment_file(root, slug))
}

/// `<root>/collections/<slug>/collection.toml`. Raw: `slug` must already have been
/// validated.
#[must_use]
pub(crate) fn collection_file(root: &Path, slug: &str) -> PathBuf {
    collection_dir(root, slug).join(COLLECTION_FILE)
}

/// `<root>/collections/<slug>/collection.toml` for a validated slug.
///
/// # Errors
///
/// [`StoreError::InvalidSlug`] when `slug` is not a single, visible path component.
pub fn checked_collection_file(root: &Path, slug: &str) -> Result<PathBuf, StoreError> {
    validate_slug(slug)?;
    Ok(collection_file(root, slug))
}

/// `<parent dir>/<slug>/folder.toml`. Raw: the parent and `slug` must already have been
/// validated.
#[must_use]
pub(crate) fn folder_file(root: &Path, parent: &Parent, slug: &str) -> PathBuf {
    parent.dir(root).join(slug).join(FOLDER_FILE)
}

/// `<parent dir>/<slug>/folder.toml` for a validated parent and slug.
///
/// # Errors
///
/// [`StoreError::InvalidSlug`] when the collection slug, a folder slug or `slug` is not a
/// single, visible path component.
pub fn checked_folder_file(
    root: &Path,
    parent: &Parent,
    slug: &str,
) -> Result<PathBuf, StoreError> {
    validate_parent(parent)?;
    validate_slug(slug)?;
    Ok(folder_file(root, parent, slug))
}

/// `<parent dir>/<slug>.req.toml`. Raw: the parent and `slug` must already have been
/// validated.
#[must_use]
pub(crate) fn request_file(root: &Path, parent: &Parent, slug: &str) -> PathBuf {
    request_file_in(&parent.dir(root), slug)
}

/// `<parent dir>/<slug>.req.toml` for a validated parent and request slug.
///
/// # Errors
///
/// As [`checked_folder_file`], plus [`StoreError::ReservedSlug`] when `slug` is reserved
/// for the sidecar scripts of the containing collection or folder.
pub fn checked_request_file(
    root: &Path,
    parent: &Parent,
    slug: &str,
) -> Result<PathBuf, StoreError> {
    validate_parent(parent)?;
    validate_request_slug(slug)?;
    Ok(request_file(root, parent, slug))
}

pub(crate) fn collection_dir(root: &Path, slug: &str) -> PathBuf {
    collections_dir(root).join(slug)
}

/// `<root>/collections`.
pub(crate) fn collections_dir(root: &Path) -> PathBuf {
    root.join(COLLECTIONS)
}

/// `<dir>/<slug>.req.toml`. Raw: `slug` must already have been validated.
pub(crate) fn request_file_in(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{slug}{REQUEST_SUFFIX}"))
}

/// `<dir>/<slug>.examples`. Raw: `slug` must already have been validated.
pub(crate) fn examples_dir_in(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{slug}{EXAMPLES_SUFFIX}"))
}

/// A slug is one visible path component: non-empty, no separator, not hidden.
/// Request slugs that would collide with the sidecar scripts of the containing collection
/// (`collection.pre.js`) or folder (`folder.pre.js`).
pub const RESERVED_REQUEST_SLUGS: [&str; 2] = ["collection", "folder"];

/// Whether `slug` is reserved for requests; see [`RESERVED_REQUEST_SLUGS`].
#[must_use]
pub fn is_reserved_request_slug(slug: &str) -> bool {
    RESERVED_REQUEST_SLUGS.contains(&slug)
}

pub(crate) fn validate_request_slug(slug: &str) -> Result<(), StoreError> {
    validate_slug(slug)?;
    if is_reserved_request_slug(slug) {
        return Err(StoreError::ReservedSlug {
            slug: slug.to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn validate_slug(slug: &str) -> Result<(), StoreError> {
    let valid = !slug.is_empty() && !slug.starts_with('.') && !slug.contains(['/', '\\']);
    if valid {
        Ok(())
    } else {
        Err(StoreError::InvalidSlug {
            slug: slug.to_owned(),
        })
    }
}

pub(crate) fn validate_parent(parent: &Parent) -> Result<(), StoreError> {
    parent.validate()
}
