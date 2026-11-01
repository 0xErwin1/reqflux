//! Writing a Postman import into a workspace (S9).
//!
//! [`postman`] maps the JSON documents with [`reqflux_postman`](crate) (no IO) and then
//! writes the entities through `reqflux_store`, atomically, after checking that nothing it
//! would write already exists: an import never overwrites a collection, folder, request
//! or environment that is already there. [`postman_preview`] runs the same mapping and
//! checks and lists what [`postman`] would write, writing nothing (app-pending S6).

mod preview;

use std::path::{Path, PathBuf};

use reqflux_core::model::{CollectionNode, EnvironmentNode, Id, Item, WorkspaceManifest};
use reqflux_store::{
    Parent, ScriptHashes, ScriptOwner, checked_collection_file, checked_environment_file,
    checked_folder_file, checked_request_file, manifest_file, save_collection, save_environment,
    save_folder, save_manifest, save_request, save_scripts,
};
use serde_json::Value;

use crate::error::ImportError;

pub use preview::{ImportPreview, postman_preview};

/// What an import wrote (S9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    /// The workspace root the import wrote to.
    pub root: PathBuf,
    /// The slug of the collection directory that was written.
    pub collection: String,
    /// Folders written, at any depth.
    pub folders: usize,
    /// Requests written, at any depth.
    pub requests: usize,
    /// Environments written.
    pub environments: usize,
    /// Non-fatal problems found while importing. Store saves are atomic and fail hard,
    /// so an import reports none today; the field keeps the summary stable when the
    /// store grows save-time warnings.
    pub warnings: Vec<String>,
}

/// Imports a Postman collection v2.1 document and any number of environment documents
/// into the workspace at `root` (S9).
///
/// When `root` has no `reqflux.toml`, a workspace manifest is created first, named after
/// the collection. An existing workspace keeps its manifest and gains the collection
/// beside what is there. Every target file is checked before anything is written — the
/// entities, the two sidecar scripts of every collection, folder and request, and the
/// request slugs against the reserved ones — so a collision fails with
/// [`ImportError::Exists`] (a reserved slug with [`ImportError::Store`]) and leaves the
/// workspace untouched.
///
/// # Errors
///
/// [`ImportError::Postman`] when a document does not map (S8), [`ImportError::Exists`]
/// when a target file already exists, [`ImportError::Store`] when the store cannot write.
pub fn postman(
    root: &Path,
    collection: &Value,
    environments: &[Value],
) -> Result<ImportSummary, ImportError> {
    let Plan {
        node,
        environments: environment_nodes,
        create_manifest,
    } = plan(root, collection, environments)?;
    let manifest_path = manifest_file(root);
    let collection_path = target_path(checked_collection_file(root, &node.slug), root)?;

    if create_manifest {
        let manifest = WorkspaceManifest {
            id: Id::new(),
            name: node.collection.name.clone(),
            format: reqflux_store::FORMAT,
            globals: Vec::new(),
            mcp: None,
            extensions: toml::Table::new(),
        };
        save_manifest(root, &manifest, None).map_err(|error| store_error(&manifest_path, error))?;
    }
    let mut summary = ImportSummary {
        root: root.to_owned(),
        collection: node.slug.clone(),
        folders: 0,
        requests: 0,
        environments: environment_nodes.len(),
        warnings: Vec::new(),
    };
    save_collection(root, &node.slug, &node.collection, None)
        .map_err(|error| store_error(&collection_path, error))?;
    save_scripts(
        root,
        &ScriptOwner::Collection(node.slug.clone()),
        &node.scripts,
        ScriptHashes::default(),
    )
    .map_err(|error| store_error(&collection_path, error))?;
    write_items(
        root,
        &Parent::collection(&node.slug),
        &node.items,
        &mut summary,
    )?;
    for environment in &environment_nodes {
        let file = target_path(checked_environment_file(root, &environment.slug), root)?;
        save_environment(root, &environment.slug, &environment.environment, None)
            .map_err(|error| store_error(&file, error))?;
    }
    Ok(summary)
}

/// A mapped import whose every target is free: what [`postman`] writes and
/// [`postman_preview`] lists.
struct Plan {
    node: CollectionNode,
    environments: Vec<EnvironmentNode>,
    /// Whether `root` has no manifest yet, so the import creates one.
    create_manifest: bool,
}

/// Maps the documents and checks that nothing the import would write exists.
///
/// # Errors
///
/// As [`postman`], before anything is written.
fn plan(root: &Path, collection: &Value, environments: &[Value]) -> Result<Plan, ImportError> {
    let imported = reqflux_postman::collection(collection)?;
    let mut taken = imported.slugs.clone();
    let mut environment_nodes = Vec::with_capacity(environments.len());
    for value in environments {
        environment_nodes.push(reqflux_postman::environment_in(value, &mut taken)?);
    }
    let node = imported.collection;
    let manifest_path = manifest_file(root);
    let create_manifest = !manifest_path.exists();

    let collection_path = target_path(checked_collection_file(root, &node.slug), root)?;
    let mut targets = Vec::new();
    if create_manifest {
        targets.push(manifest_path);
    }
    targets.push(collection_path);
    targets.extend(sidecars(&ScriptOwner::Collection(node.slug.clone()), root));
    collect_targets(
        root,
        &node.items,
        &Parent::collection(&node.slug),
        &mut targets,
    )?;
    for environment in &environment_nodes {
        let file = target_path(checked_environment_file(root, &environment.slug), root)?;
        targets.push(file);
    }
    for target in &targets {
        if target.exists() {
            return Err(ImportError::Exists {
                path: target.clone(),
            });
        }
    }
    Ok(Plan {
        node,
        environments: environment_nodes,
        create_manifest,
    })
}

fn write_items(
    root: &Path,
    parent: &Parent,
    items: &[Item],
    summary: &mut ImportSummary,
) -> Result<(), ImportError> {
    for item in items {
        match item {
            Item::Folder(folder) => {
                let file = target_path(checked_folder_file(root, parent, &folder.slug), root)?;
                save_folder(root, parent, &folder.slug, &folder.folder, None)
                    .map_err(|error| store_error(&file, error))?;
                save_scripts(
                    root,
                    &ScriptOwner::Folder(parent.clone(), folder.slug.clone()),
                    &folder.scripts,
                    ScriptHashes::default(),
                )
                .map_err(|error| store_error(&file, error))?;
                write_items(root, &parent.child(&folder.slug), &folder.items, summary)?;
                summary.folders += 1;
            }
            Item::Request(request) => {
                let file = target_path(checked_request_file(root, parent, &request.slug), root)?;
                save_request(root, parent, &request.slug, &request.request, None)
                    .map_err(|error| store_error(&file, error))?;
                save_scripts(
                    root,
                    &ScriptOwner::Request(parent.clone(), request.slug.clone()),
                    &request.scripts,
                    ScriptHashes::default(),
                )
                .map_err(|error| store_error(&file, error))?;
                summary.requests += 1;
            }
        }
    }
    Ok(())
}

/// Collects every file the import would write for `items` under `parent`: the entity
/// files and the two sidecar scripts of each folder and request. A request whose slug the
/// store rejects (not a path component, or reserved: `collection`, `folder`) fails here,
/// before anything is written.
fn collect_targets(
    root: &Path,
    items: &[Item],
    parent: &Parent,
    targets: &mut Vec<PathBuf>,
) -> Result<(), ImportError> {
    for item in items {
        match item {
            Item::Folder(folder) => {
                let file = target_path(checked_folder_file(root, parent, &folder.slug), root)?;
                targets.push(file);
                targets.extend(sidecars(
                    &ScriptOwner::Folder(parent.clone(), folder.slug.clone()),
                    root,
                ));
                collect_targets(root, &folder.items, &parent.child(&folder.slug), targets)?;
            }
            Item::Request(request) => {
                let file = target_path(checked_request_file(root, parent, &request.slug), root)?;
                targets.push(file);
                targets.extend(sidecars(
                    &ScriptOwner::Request(parent.clone(), request.slug.clone()),
                    root,
                ));
            }
        }
    }
    Ok(())
}

/// Unwraps a checked layout path, mapping a slug that fails validation to an import
/// error. Nothing has been written; the error names the workspace root and the slug.
fn target_path(
    path: Result<PathBuf, reqflux_store::StoreError>,
    root: &Path,
) -> Result<PathBuf, ImportError> {
    path.map_err(|error| ImportError::Store {
        path: root.to_owned(),
        message: error.to_string(),
    })
}

/// The two sidecar script paths of `owner`, in the order the store writes them.
fn sidecars(owner: &ScriptOwner, root: &Path) -> [PathBuf; 2] {
    let files = owner.files(root);
    [files.pre_request, files.test]
}

/// Maps a store failure on `target` to an [`ImportError`]. A conflict means the file
/// appeared between the preflight and the write, which is an existing file again.
fn store_error(target: &Path, error: reqflux_store::StoreError) -> ImportError {
    match error {
        reqflux_store::StoreError::Conflict { path } => ImportError::Exists { path },
        other => ImportError::Store {
            path: target.to_owned(),
            message: other.to_string(),
        },
    }
}
