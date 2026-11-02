//! Tree operations on the loaded workspace (S8): each calls the store, then reloads.
//!
//! Ids are stable across rename, move and reorder, so anything a front end keys by id
//! (tabs, drafts, the active request) keeps resolving after the reload.

use reqflux_core::model::Id;
use reqflux_store::{Loaded, StoreError};

use crate::engine::Engine;
use crate::error::TreeError;

impl Engine {
    /// Creates a collection named `New collection` (numbered when taken) and returns its id.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn create_collection(&mut self) -> Result<Id, TreeError> {
        let placed = reqflux_store::create_collection(&self.root)?;
        self.reload_tree(placed.id)
    }

    /// Creates a folder at the end of the collection or folder `parent` and returns its id.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn create_folder(&mut self, parent: Id) -> Result<Id, TreeError> {
        self.apply(|root, loaded| reqflux_store::create_folder(root, loaded, parent).map(|p| p.id))
    }

    /// Creates a `GET` request with an empty URL at the end of the collection or folder
    /// `parent` and returns its id.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn create_request(&mut self, parent: Id) -> Result<Id, TreeError> {
        self.apply(|root, loaded| reqflux_store::create_request(root, loaded, parent).map(|p| p.id))
    }

    /// Renames the entity `id`, moving its file to the new slug, and returns `id`.
    ///
    /// # Errors
    ///
    /// The store error (an empty, reserved or taken name), or [`TreeError::Reload`].
    pub fn rename(&mut self, id: Id, name: &str) -> Result<Id, TreeError> {
        self.apply(|root, loaded| reqflux_store::rename(root, loaded, id, name).map(|p| p.id))
    }

    /// Duplicates the request or folder `id` right after it and returns the copy's new id.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn duplicate(&mut self, id: Id) -> Result<Id, TreeError> {
        self.apply(|root, loaded| reqflux_store::duplicate(root, loaded, id).map(|p| p.id))
    }

    /// Deletes the entity `id` with everything under it.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn delete(&mut self, id: Id) -> Result<(), TreeError> {
        self.apply(|root, loaded| reqflux_store::delete(root, loaded, id))
    }

    /// Moves the request or folder `id` into the collection or folder `parent` at `index`
    /// and returns `id`.
    ///
    /// # Errors
    ///
    /// The store error (a slug clash, or a folder into its own subtree), or
    /// [`TreeError::Reload`].
    pub fn move_item(&mut self, id: Id, parent: Id, index: usize) -> Result<Id, TreeError> {
        self.apply(|root, loaded| {
            reqflux_store::move_item(root, loaded, id, parent, index).map(|p| p.id)
        })
    }

    /// Moves the request or folder `id` to `index` among its siblings and returns `id`.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn reorder(&mut self, id: Id, index: usize) -> Result<Id, TreeError> {
        self.apply(|root, loaded| reqflux_store::reorder(root, loaded, id, index).map(|()| id))
    }

    /// Replaces the environment allowlist of the collection `id`: the ids of the workspace
    /// environments its requests may run with; empty means every one of them (v1-features S7).
    ///
    /// # Errors
    ///
    /// The store error (an unknown collection, a conflict), or [`TreeError::Reload`].
    pub fn set_collection_environments(
        &mut self,
        id: Id,
        environments: Vec<Id>,
    ) -> Result<(), TreeError> {
        self.apply(|root, loaded| {
            reqflux_store::set_collection_environments(root, loaded, id, environments)
        })
    }

    /// Runs a store operation against the loaded workspace and reloads after it succeeds.
    pub(crate) fn apply<T>(
        &mut self,
        operation: impl FnOnce(&std::path::Path, &Loaded) -> Result<T, StoreError>,
    ) -> Result<T, TreeError> {
        let value = operation(&self.root, &self.loaded)?;
        self.reload_tree(value)
    }

    fn reload_tree<T>(&mut self, value: T) -> Result<T, TreeError> {
        self.reload().map_err(TreeError::Reload)?;
        Ok(value)
    }
}
