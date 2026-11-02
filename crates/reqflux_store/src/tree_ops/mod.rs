//! Tree operations: create, rename, duplicate, delete, move and reorder collections,
//! folders and requests (tree-crud S1-S7), and create, rename and delete environments
//! (variables-editor S3).
//!
//! Every operation takes the [`Loaded`] workspace the caller holds and names entities by
//! id. It validates names, slugs and targets and checks the hash of every existing file it
//! rewrites or removes before its first write, so a refused operation changes nothing on
//! disk. Order lives in the parent: an operation that adds, removes or moves a child
//! rewrites only the affected parents' `order`, as the children's display order with the
//! change applied. The caller reloads afterwards.
//!
//! An operation is several filesystem steps, each atomic on its own. A failure partway
//! through (an IO error, or a concurrent write landing after the checks) can leave the
//! files moved or written but the name or a parent's `order` not yet updated. The order is
//! always written last, and a stale or missing `order` entry is harmless on load: unlisted
//! children append and unmatched entries are skipped. A duplicated folder is built in a
//! hidden staging directory and a deleted one is renamed into one first, so either appears
//! or disappears in a single rename.
//!
//! Before deleting or duplicating a folder or collection, every entity file and sidecar
//! script under it is compared with what the load read, so a file added or changed since
//! is never removed or copied stale. Known gaps:
//!
//! - A write landing between the checks and the staging rename is still lost.
//! - When removing a staged folder fails, it is renamed back before the error returns, but
//!   the files removed before the failure stay removed; if the rename back fails too, the
//!   staging directory is kept and logged.
//! - A move is not rolled back: a failure after the files moved leaves them in the target
//!   with a stale `order` in one or both parents.
//! - Slugs are ASCII only: non-ASCII letters become `-`, so a name without ASCII
//!   alphanumerics slugs to `item`.

mod environments;
mod examples;
mod files;
mod locate;
mod naming;
mod restructure;

use std::path::{Path, PathBuf};

use reqflux_core::model::{Auth, Collection, Folder, Id, Method, Request};
use serde::Serialize;

use crate::error::StoreError;
use crate::layout::{
    collection_dir, collection_file, collections_dir, folder_file, request_file, validate_slug,
};
use crate::load::Loaded;
use crate::save::{check, save_document};
use files::Shape;
use locate::{Container, Entity, find, find_container};
use naming::{SlugFor, ensure_free, named, numbered};

pub use environments::{create_environment, delete_environment, rename_environment};
pub use examples::{
    StoredExample, create_example, create_example_at, delete_example, example_files_at, examples,
    rename_example,
};
pub use restructure::{delete, duplicate, move_item, reorder};

/// Where an operation put an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The entity id: new for a created or duplicated entity, kept otherwise.
    pub id: Id,
    /// Its entity file: `collection.toml`, `folder.toml`, `<slug>.req.toml` or
    /// `<slug>.env.toml`.
    pub path: PathBuf,
}

/// Creates the collection `New collection` (then `New collection 2`, ...).
///
/// # Errors
///
/// An IO error naming the path.
pub fn create_collection(root: &Path) -> Result<Placed, StoreError> {
    let (name, slug) = numbered(&collections_dir(root), "New collection", SlugFor::Container)?;
    let collection = Collection {
        id: Id::new(),
        name,
        environments: Vec::new(),
        order: Vec::new(),
        auth: Auth::Inherit,
        variables: Vec::new(),
        extensions: toml::Table::new(),
    };
    let path = collection_file(root, &slug);
    save_document(&path, &collection, None)?;
    Ok(Placed {
        id: collection.id,
        path,
    })
}

/// Creates the folder `New folder` (then `New folder 2`, ...) at the end of the collection
/// or folder `parent`.
///
/// # Errors
///
/// [`StoreError::NotFound`], [`StoreError::NotAContainer`], a conflict when the parent
/// file changed since `loaded` read it, or an IO error.
pub fn create_folder(root: &Path, loaded: &Loaded, parent: Id) -> Result<Placed, StoreError> {
    let container = find_container(&loaded.workspace, parent)?;
    container.validate()?;
    container.check(root, loaded)?;
    let (name, slug) = numbered(&container.dir(root), "New folder", SlugFor::Container)?;
    let folder = Folder {
        id: Id::new(),
        name,
        order: Vec::new(),
        auth: Auth::Inherit,
        extensions: toml::Table::new(),
    };
    let path = folder_file(root, &container.inner, &slug);
    save_document(&path, &folder, None)?;
    append(root, loaded, &container, slug)?;
    Ok(Placed {
        id: folder.id,
        path,
    })
}

/// Creates the request `New request` (then `New request 2`, ...), `GET` with an empty
/// URL, at the end of the collection or folder `parent`.
///
/// # Errors
///
/// As [`create_folder`].
pub fn create_request(root: &Path, loaded: &Loaded, parent: Id) -> Result<Placed, StoreError> {
    let container = find_container(&loaded.workspace, parent)?;
    container.validate()?;
    container.check(root, loaded)?;
    let (name, slug) = numbered(&container.dir(root), "New request", SlugFor::Request)?;
    let request = Request::new(Id::new(), name, Method::GET, "");
    let path = request_file(root, &container.inner, &slug);
    save_document(&path, &request, None)?;
    append(root, loaded, &container, slug)?;
    Ok(Placed {
        id: request.id,
        path,
    })
}

fn append(
    root: &Path,
    loaded: &Loaded,
    container: &Container<'_>,
    slug: String,
) -> Result<(), StoreError> {
    let mut order = container.order();
    order.push(slug);
    container.save_order(root, loaded, order).map(drop)
}

/// Renames the entity `id`: its display name and its slug, so its file or directory (with
/// a request's sidecars and examples) follows the name. The id is kept and the parent's
/// `order` entry is renamed in place.
///
/// # Errors
///
/// [`StoreError::EmptyName`], [`StoreError::ReservedSlug`] for a request,
/// [`StoreError::SlugTaken`] when a sibling holds the new slug, [`StoreError::NotFound`],
/// a conflict when a file to rewrite changed since `loaded` read it, or an IO error.
pub fn rename(root: &Path, loaded: &Loaded, id: Id, name: &str) -> Result<Placed, StoreError> {
    let path = match find(&loaded.workspace, id)? {
        Entity::Collection(node) => {
            let (name, slug) = named(name, SlugFor::Container)?;
            validate_slug(&node.slug)?;
            let old = collection_file(root, &node.slug);
            let expected = loaded.hash(&old);
            check(&old, expected)?;
            if slug != node.slug {
                ensure_free(&collections_dir(root), &slug, SlugFor::Container)?;
                files::rename(
                    &collection_dir(root, &node.slug),
                    &collection_dir(root, &slug),
                )?;
            }
            let mut collection = node.collection.clone();
            collection.name = name;
            let path = collection_file(root, &slug);
            save_document(&path, &collection, expected)?;
            path
        }
        Entity::Folder { parent, node } => {
            let (name, slug) = named(name, SlugFor::Container)?;
            let mut folder = node.folder.clone();
            folder.name = name;
            rename_child(
                root,
                loaded,
                &parent,
                Shape::Folder,
                &node.slug,
                &slug,
                &folder,
            )?
        }
        Entity::Request { parent, node } => {
            let (name, slug) = named(name, SlugFor::Request)?;
            let mut request = node.request.clone();
            request.name = name;
            rename_child(
                root,
                loaded,
                &parent,
                Shape::Request,
                &node.slug,
                &slug,
                &request,
            )?
        }
    };
    Ok(Placed { id, path })
}

/// Moves a folder or request from `old` to `slug` in `parent` and writes `document` (the
/// renamed entity) over its moved file, then renames the `order` entry in place.
fn rename_child<T: Serialize>(
    root: &Path,
    loaded: &Loaded,
    parent: &Container<'_>,
    shape: Shape,
    old: &str,
    slug: &str,
    document: &T,
) -> Result<PathBuf, StoreError> {
    parent.validate()?;
    shape.validate(old)?;
    let old_file = shape.file(root, &parent.inner, old);
    let expected = loaded.hash(&old_file);
    check(&old_file, expected)?;
    let moved = slug != old;
    if moved {
        parent.check(root, loaded)?;
        let dir = parent.dir(root);
        ensure_free(&dir, slug, shape.slug_for())?;
        files::relocate(shape, &dir, old, &dir, slug)?;
    }
    let path = shape.file(root, &parent.inner, slug);
    save_document(&path, document, expected)?;
    if moved {
        let order = parent
            .order()
            .into_iter()
            .map(|entry| if entry == old { slug.to_owned() } else { entry })
            .collect();
        parent.save_order(root, loaded, order)?;
    }
    Ok(path)
}
