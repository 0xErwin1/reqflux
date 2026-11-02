//! What an export, a bundle or the docs take (v1-features S6): a collection, a folder, a
//! request or an environment, selected by id or slug path.

use reqflux_core::model::{
    CollectionNode, EnvironmentNode, FolderNode, Item, RequestNode, Workspace,
};

/// The prefix of an environment selector: `env:<slug>`.
pub const ENVIRONMENT_PREFIX: &str = "env:";

/// A selected unit of a workspace.
#[derive(Debug, Clone)]
pub(crate) enum Unit<'a> {
    /// A collection with everything under it.
    Collection(&'a CollectionNode),
    /// A folder; `folders` holds its enclosing folders, outermost first, then the folder.
    Folder {
        collection: &'a CollectionNode,
        folders: Vec<&'a FolderNode>,
    },
    /// A request and its enclosing folders, outermost first.
    Request {
        collection: &'a CollectionNode,
        folders: Vec<&'a FolderNode>,
        request: &'a RequestNode,
    },
    /// A workspace environment.
    Environment(&'a EnvironmentNode),
}

/// The unit `selector` names: `env:<slug>` or an environment id, a request's id or slug path
/// (a request wins over a folder at the same path), or a collection's or folder's id or slug
/// path (`<collection>/<folder>/...`). `None` when nothing matches.
pub(crate) fn select<'a>(workspace: &'a Workspace, selector: &str) -> Option<Unit<'a>> {
    if let Some(slug) = selector.strip_prefix(ENVIRONMENT_PREFIX) {
        return workspace
            .environments
            .iter()
            .find(|environment| environment.slug == slug)
            .map(Unit::Environment);
    }
    if let Some(environment) = workspace
        .environments
        .iter()
        .find(|environment| environment.environment.id.to_string() == selector)
    {
        return Some(Unit::Environment(environment));
    }
    if let Ok(target) = crate::select::find(workspace, selector) {
        return Some(Unit::Request {
            collection: target.collection,
            folders: target.folders,
            request: target.request,
        });
    }
    by_id(workspace, selector).or_else(|| by_path(workspace, selector))
}

fn by_id<'a>(workspace: &'a Workspace, id: &str) -> Option<Unit<'a>> {
    workspace.collections.iter().find_map(|collection| {
        if collection.collection.id.to_string() == id {
            return Some(Unit::Collection(collection));
        }
        let mut folders = Vec::new();
        folder_by_id(&collection.items, id, &mut folders).then_some(Unit::Folder {
            collection,
            folders,
        })
    })
}

/// Whether the folder `id` is under `items`; `folders` then holds the path to it, the
/// folder last.
fn folder_by_id<'a>(items: &'a [Item], id: &str, folders: &mut Vec<&'a FolderNode>) -> bool {
    for item in items {
        if let Item::Folder(folder) = item {
            folders.push(folder);
            if folder.folder.id.to_string() == id || folder_by_id(&folder.items, id, folders) {
                return true;
            }
            folders.pop();
        }
    }
    false
}

fn by_path<'a>(workspace: &'a Workspace, path: &str) -> Option<Unit<'a>> {
    let mut segments = path.split('/');
    let collection_slug = segments.next()?;
    let collection = workspace
        .collections
        .iter()
        .find(|collection| collection.slug == collection_slug)?;
    let mut folders = Vec::new();
    let mut items = &collection.items;
    for slug in segments {
        let folder = items.iter().find_map(|item| match item {
            Item::Folder(folder) if folder.slug == slug => Some(folder),
            _ => None,
        })?;
        folders.push(folder);
        items = &folder.items;
    }
    if folders.is_empty() {
        Some(Unit::Collection(collection))
    } else {
        Some(Unit::Folder {
            collection,
            folders,
        })
    }
}

/// The collection holding only the path to `leaf`: each of `folders` (outermost first)
/// keeps its own fields with `leaf`'s branch as its only child, so the auth and scripts the
/// leaf inherits come with it.
pub(crate) fn pruned(
    collection: &CollectionNode,
    folders: &[&FolderNode],
    leaf: Item,
) -> CollectionNode {
    let mut item = leaf;
    for folder in folders.iter().rev() {
        let mut node = (*folder).clone();
        node.folder.order = vec![item.slug().to_owned()];
        node.items = vec![item];
        item = Item::Folder(node);
    }
    let mut node = collection.clone();
    node.collection.order = vec![item.slug().to_owned()];
    node.items = vec![item];
    node
}

/// Every request under `items`, depth first.
pub(crate) fn requests(items: &[Item]) -> Vec<&RequestNode> {
    let mut out = Vec::new();
    collect(items, &mut out);
    out
}

fn collect<'a>(items: &'a [Item], out: &mut Vec<&'a RequestNode>) {
    for item in items {
        match item {
            Item::Folder(folder) => collect(&folder.items, out),
            Item::Request(request) => out.push(request),
        }
    }
}
