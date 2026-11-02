//! Request selection by id or slug path (S10).

use reqflux_core::model::{Auth, CollectionNode, FolderNode, Item, RequestNode, Workspace};
use reqflux_store::Parent;

use crate::error::ExecuteError;

/// A request together with the collection and folders that contain it.
#[derive(Debug, Clone)]
pub(crate) struct Target<'a> {
    /// The collection.
    pub(crate) collection: &'a CollectionNode,
    /// The enclosing folders, outermost first.
    pub(crate) folders: Vec<&'a FolderNode>,
    /// The request.
    pub(crate) request: &'a RequestNode,
}

/// Finds the request whose id is `selector`, or whose path is
/// `<collection slug>/<folder slug>/.../<request slug>`.
pub(crate) fn find<'a>(
    workspace: &'a Workspace,
    selector: &str,
) -> Result<Target<'a>, ExecuteError> {
    by_id(workspace, selector)
        .or_else(|| by_path(workspace, selector))
        .ok_or_else(|| ExecuteError::UnknownRequest(selector.to_owned()))
}

fn by_id<'a>(workspace: &'a Workspace, id: &str) -> Option<Target<'a>> {
    workspace.collections.iter().find_map(|collection| {
        let mut folders = Vec::new();
        find_id(&collection.items, id, &mut folders).map(|request| Target {
            collection,
            folders,
            request,
        })
    })
}

fn find_id<'a>(
    items: &'a [Item],
    id: &str,
    folders: &mut Vec<&'a FolderNode>,
) -> Option<&'a RequestNode> {
    for item in items {
        match item {
            Item::Request(request) if request.request.id.to_string() == id => {
                return Some(request);
            }
            Item::Request(_) => {}
            Item::Folder(folder) => {
                folders.push(folder);
                if let Some(request) = find_id(&folder.items, id, folders) {
                    return Some(request);
                }
                folders.pop();
            }
        }
    }
    None
}

fn by_path<'a>(workspace: &'a Workspace, path: &str) -> Option<Target<'a>> {
    let segments: Vec<&str> = path.split('/').collect();
    let (request_slug, directories) = segments.split_last()?;
    let (collection_slug, folder_slugs) = directories.split_first()?;
    let collection = workspace
        .collections
        .iter()
        .find(|collection| collection.slug == *collection_slug)?;
    let mut folders = Vec::new();
    let mut items = &collection.items;
    for slug in folder_slugs {
        let folder = items.iter().find_map(|item| match item {
            Item::Folder(folder) if folder.slug == *slug => Some(folder),
            _ => None,
        })?;
        folders.push(folder);
        items = &folder.items;
    }
    let request = items.iter().find_map(|item| match item {
        Item::Request(request) if request.slug == *request_slug => Some(request),
        _ => None,
    })?;
    Some(Target {
        collection,
        folders,
        request,
    })
}

impl<'a> Target<'a> {
    /// The auth of the request, or of the nearest folder, then the collection, that does not
    /// inherit; the same walk the engine's preparation makes. [`Auth::None`] when all inherit.
    pub(crate) fn effective_auth(&self) -> &'a Auth {
        std::iter::once(&self.request.request.auth)
            .chain(self.folders.iter().rev().map(|folder| &folder.folder.auth))
            .chain(std::iter::once(&self.collection.collection.auth))
            .find(|auth| !auth.is_inherit())
            .unwrap_or(&Auth::None)
    }

    /// The store parent of the request: its collection and folder slugs, outermost first.
    pub(crate) fn parent(&self) -> Parent {
        Parent {
            collection: self.collection.slug.clone(),
            folders: self
                .folders
                .iter()
                .map(|folder| folder.slug.clone())
                .collect(),
        }
    }
}
