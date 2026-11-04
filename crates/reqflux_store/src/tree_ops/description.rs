//! Setting the markdown description of a collection, folder or request.

use std::path::Path;

use reqflux_core::model::Id;

use super::files::Shape;
use super::locate::{Entity, find};
use crate::error::StoreError;
use crate::layout::{collection_file, validate_slug};
use crate::load::Loaded;
use crate::save::{check, save_document};

/// Writes `text` as the description of the collection, folder or request `id`; blank text
/// clears it. The copy an earlier Postman import kept in `extensions.postman` goes away in
/// the same write, so the edit is the only description left.
///
/// # Errors
///
/// [`StoreError::NotFound`], a conflict when the entity's file changed since `loaded` read
/// it, or an IO error.
pub fn set_description(root: &Path, loaded: &Loaded, id: Id, text: &str) -> Result<(), StoreError> {
    match find(&loaded.workspace, id)? {
        Entity::Collection(node) => {
            validate_slug(&node.slug)?;
            let file = collection_file(root, &node.slug);
            let expected = loaded.hash(&file);
            check(&file, expected)?;
            let mut collection = node.collection.clone();
            collection.set_description(text);
            save_document(&file, &collection, expected).map(|_| ())
        }
        Entity::Folder { parent, node } => {
            parent.validate()?;
            Shape::Folder.validate(&node.slug)?;
            let file = Shape::Folder.file(root, &parent.inner, &node.slug);
            let expected = loaded.hash(&file);
            check(&file, expected)?;
            let mut folder = node.folder.clone();
            folder.set_description(text);
            save_document(&file, &folder, expected).map(|_| ())
        }
        Entity::Request { parent, node } => {
            parent.validate()?;
            Shape::Request.validate(&node.slug)?;
            let file = Shape::Request.file(root, &parent.inner, &node.slug);
            let expected = loaded.hash(&file);
            check(&file, expected)?;
            let mut request = node.request.clone();
            request.set_description(text);
            save_document(&file, &request, expected).map(|_| ())
        }
    }
}
