//! What a Postman import would write, without writing it (app-pending S6).

use std::path::{Path, PathBuf};

use reqflux_core::model::{Item, Scripts};
use reqflux_store::{
    Parent, ScriptOwner, checked_collection_file, checked_environment_file, checked_folder_file,
    checked_request_file, manifest_file,
};
use serde_json::Value;

use super::{plan, target_path};
use crate::error::ImportError;

/// What [`postman`](super::postman) would write into a workspace: the counts and the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    /// The name of the collection.
    pub collection: String,
    /// Folders, at any depth.
    pub folders: usize,
    /// Requests, at any depth.
    pub requests: usize,
    /// Script files, the pre-request and test sidecars that are not empty.
    pub scripts: usize,
    /// Environments.
    pub environments: usize,
    /// Every file the import writes, relative to the workspace root, in write order.
    pub files: Vec<PathBuf>,
}

/// Maps the documents and checks the targets as [`postman`](super::postman) does, and
/// returns what it would write. Nothing is written.
///
/// # Errors
///
/// The errors of [`postman`](super::postman) that happen before it writes:
/// [`ImportError::Postman`] when a document does not map, [`ImportError::Exists`] when a
/// target file exists, [`ImportError::Store`] when a slug is not a valid file name.
pub fn postman_preview(
    root: &Path,
    collection: &Value,
    environments: &[Value],
) -> Result<ImportPreview, ImportError> {
    let plan = plan(root, collection, environments)?;
    let node = &plan.node;
    let mut preview = ImportPreview {
        collection: node.collection.name.clone(),
        folders: 0,
        requests: 0,
        scripts: 0,
        environments: plan.environments.len(),
        files: Vec::new(),
    };
    let mut files = Vec::new();
    if plan.create_manifest {
        files.push(manifest_file(root));
    }
    files.push(target_path(
        checked_collection_file(root, &node.slug),
        root,
    )?);
    push_scripts(
        root,
        &ScriptOwner::Collection(node.slug.clone()),
        &node.scripts,
        &mut files,
        &mut preview.scripts,
    );
    walk(
        root,
        &Parent::collection(&node.slug),
        &node.items,
        &mut files,
        &mut preview,
    )?;
    for environment in &plan.environments {
        files.push(target_path(
            checked_environment_file(root, &environment.slug),
            root,
        )?);
    }
    preview.files = files
        .into_iter()
        .map(|file| {
            file.strip_prefix(root)
                .map_or(file.clone(), Path::to_path_buf)
        })
        .collect();
    Ok(preview)
}

/// The files of `items` under `parent`, in the order the import writes them, counting the
/// folders, requests and scripts into `preview`.
fn walk(
    root: &Path,
    parent: &Parent,
    items: &[Item],
    files: &mut Vec<PathBuf>,
    preview: &mut ImportPreview,
) -> Result<(), ImportError> {
    for item in items {
        match item {
            Item::Folder(folder) => {
                files.push(target_path(
                    checked_folder_file(root, parent, &folder.slug),
                    root,
                )?);
                push_scripts(
                    root,
                    &ScriptOwner::Folder(parent.clone(), folder.slug.clone()),
                    &folder.scripts,
                    files,
                    &mut preview.scripts,
                );
                walk(
                    root,
                    &parent.child(&folder.slug),
                    &folder.items,
                    files,
                    preview,
                )?;
                preview.folders += 1;
            }
            Item::Request(request) => {
                files.push(target_path(
                    checked_request_file(root, parent, &request.slug),
                    root,
                )?);
                push_scripts(
                    root,
                    &ScriptOwner::Request(parent.clone(), request.slug.clone()),
                    &request.scripts,
                    files,
                    &mut preview.scripts,
                );
                preview.requests += 1;
            }
        }
    }
    Ok(())
}

/// The sidecar scripts of `owner` the store writes: those that are not empty.
fn push_scripts(
    root: &Path,
    owner: &ScriptOwner,
    scripts: &Scripts,
    files: &mut Vec<PathBuf>,
    count: &mut usize,
) {
    let paths = owner.files(root);
    for (path, script) in [
        (paths.pre_request, &scripts.pre_request),
        (paths.test, &scripts.test),
    ] {
        if script.as_deref().is_some_and(|text| !text.is_empty()) {
            files.push(path);
            *count += 1;
        }
    }
}
