//! The run order of a collection runner target: the requests under a collection or folder,
//! selected by id or slug path, in tree order, depth first.

use reqflux_core::model::{CollectionNode, Item, Workspace};

use super::RunItem;
use crate::error::ExecuteError;

/// The collection `selector` belongs to and the requests it runs, in order: a collection or
/// folder runs every request under it, recursively; a request runs alone. An id wins over a
/// slug path, and a request over a folder at the same path.
pub(crate) fn plan<'a>(
    workspace: &'a Workspace,
    selector: &str,
) -> Result<(&'a CollectionNode, Vec<RunItem>), ExecuteError> {
    if let Ok(target) = crate::select::find(workspace, selector) {
        let parent = target.parent();
        let mut path = parent.collection.clone();
        for folder in &parent.folders {
            path.push('/');
            path.push_str(folder);
        }
        let item = RunItem {
            id: target.request.request.id.to_string(),
            name: target.request.request.name.clone(),
            path: format!("{path}/{}", target.request.slug),
        };
        return Ok((target.collection, vec![item]));
    }
    by_id(workspace, selector)
        .or_else(|| by_path(workspace, selector))
        .ok_or_else(|| ExecuteError::UnknownTarget(selector.to_owned()))
}

/// The collection or folder whose id is `id`.
fn by_id<'a>(workspace: &'a Workspace, id: &str) -> Option<(&'a CollectionNode, Vec<RunItem>)> {
    workspace.collections.iter().find_map(|collection| {
        if collection.collection.id.to_string() == id {
            let mut items = Vec::new();
            collect(&collection.items, &collection.slug, &mut items);
            return Some((collection, items));
        }
        folder_by_id(&collection.items, &collection.slug, id).map(|items| (collection, items))
    })
}

fn folder_by_id(items: &[Item], prefix: &str, id: &str) -> Option<Vec<RunItem>> {
    items.iter().find_map(|item| match item {
        Item::Folder(folder) => {
            let path = format!("{prefix}/{}", folder.slug);
            if folder.folder.id.to_string() == id {
                let mut found = Vec::new();
                collect(&folder.items, &path, &mut found);
                Some(found)
            } else {
                folder_by_id(&folder.items, &path, id)
            }
        }
        Item::Request(_) => None,
    })
}

/// The collection or folder at `<collection>/<folder>/...`.
fn by_path<'a>(workspace: &'a Workspace, path: &str) -> Option<(&'a CollectionNode, Vec<RunItem>)> {
    let mut segments = path.split('/');
    let collection_slug = segments.next()?;
    let collection = workspace
        .collections
        .iter()
        .find(|collection| collection.slug == collection_slug)?;
    let mut items = &collection.items;
    let mut prefix = collection.slug.clone();
    for slug in segments {
        let folder = items.iter().find_map(|item| match item {
            Item::Folder(folder) if folder.slug == slug => Some(folder),
            _ => None,
        })?;
        prefix = format!("{prefix}/{}", folder.slug);
        items = &folder.items;
    }
    let mut found = Vec::new();
    collect(items, &prefix, &mut found);
    Some((collection, found))
}

/// Appends every request under `items` in tree order, depth first.
fn collect(items: &[Item], prefix: &str, out: &mut Vec<RunItem>) {
    for item in items {
        match item {
            Item::Request(request) => out.push(RunItem {
                id: request.request.id.to_string(),
                name: request.request.name.clone(),
                path: format!("{prefix}/{}", request.slug),
            }),
            Item::Folder(folder) => {
                collect(&folder.items, &format!("{prefix}/{}", folder.slug), out);
            }
        }
    }
}
