//! Running the sidebar's tree operations (tree-crud S6, S8, S9) through the engine, which
//! reloads the workspace after each, including the reorder and move of tree-crud S10. Tabs, drafts and the active request are keyed by id,
//! which a rename or move keeps, so they follow; a delete closes the tabs and drops the
//! drafts of every request it removed.

use std::sync::Arc;

use gpui::Context;
use reqflux_core::model::Id;
use reqflux_engine::{Engine, TreeError};

use super::Shell;
use super::sidebar_edit::TreeCommand;
use super::tree_model::{self, Node};
use crate::workspace::OpenedWorkspace;

/// The error a tree operation shows while a send's prepare task holds the engine.
pub(crate) const BUSY: &str = "Wait for the request being sent, then try again";

impl Shell {
    /// Runs `command` and reports the outcome to the sidebar: the new tree, with the entity
    /// the command created selected, or the error, leaving the tree as it was.
    pub(super) fn run_tree_command(&mut self, command: &TreeCommand, cx: &mut Context<Self>) {
        // A delete removes every request under the entity: collect them while the tree
        // still has them.
        let deleted = match command {
            TreeCommand::Delete(id) => tree_model::find(&self.roots, id)
                .map(Node::request_ids)
                .unwrap_or_default(),
            TreeCommand::NewCollection
            | TreeCommand::NewRequest(_)
            | TreeCommand::NewFolder(_)
            | TreeCommand::Rename { .. }
            | TreeCommand::Duplicate(_)
            | TreeCommand::Reorder { .. }
            | TreeCommand::Move { .. } => Vec::new(),
        };
        let outcome = self.with_engine(|engine| apply(engine, command));
        match outcome {
            Ok((created, roots)) => {
                self.roots = roots.clone();
                for id in &deleted {
                    self.forget_request(id, cx);
                }
                let rename = matches!(
                    command,
                    TreeCommand::NewCollection
                        | TreeCommand::NewRequest(_)
                        | TreeCommand::NewFolder(_)
                );
                let created = created.map(|id| id.to_string());
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.tree_changed(roots, created, rename, cx);
                });
                self.inspect_open_requests(cx);
            }
            Err(error) => {
                log::warn!("tree operation {command:?} failed: {error}");
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.show_error(error, cx));
            }
        }
    }

    /// Runs `operation` on the engine and returns its result with the reloaded trees.
    /// Fails without touching the workspace while a send's prepare task holds the other
    /// `Arc` clone: the engine is never dropped or replaced, because its runtime owns a
    /// send in flight.
    pub(super) fn with_engine(
        &mut self,
        operation: impl FnOnce(&mut Engine) -> Result<Option<Id>, String>,
    ) -> Result<(Option<Id>, Vec<Node>), String> {
        let OpenedWorkspace::Loaded(engine) = &mut self.workspace else {
            return Err("No workspace is open".to_owned());
        };
        let Some(engine) = Arc::get_mut(engine) else {
            return Err(BUSY.to_owned());
        };
        let created = operation(engine)?;
        Ok((created, tree_model::engine_nodes(engine)))
    }

    /// Closes the tab of the deleted request `id` and drops its draft, in memory and in
    /// the app state, and its send.
    fn forget_request(&mut self, id: &str, cx: &mut Context<Self>) {
        self.close_document(id, cx);
        if self.drafts.remove(id).is_some() {
            self.delete_stored_draft(id);
        }
    }
}

/// Runs `command` on `engine`; returns the id of the entity it created, or moved so the
/// sidebar keeps it selected and its new parent expanded.
fn apply(engine: &mut Engine, command: &TreeCommand) -> Result<Option<Id>, String> {
    let message = |error: TreeError| error.to_string();
    match command {
        TreeCommand::NewCollection => engine.create_collection().map(Some).map_err(message),
        TreeCommand::NewRequest(parent) => engine
            .create_request(parse(parent)?)
            .map(Some)
            .map_err(message),
        TreeCommand::NewFolder(parent) => engine
            .create_folder(parse(parent)?)
            .map(Some)
            .map_err(message),
        TreeCommand::Rename { id, name } => engine
            .rename(parse(id)?, name)
            .map(|_| None)
            .map_err(message),
        TreeCommand::Duplicate(id) => engine.duplicate(parse(id)?).map(Some).map_err(message),
        TreeCommand::Delete(id) => engine.delete(parse(id)?).map(|()| None).map_err(message),
        TreeCommand::Reorder { id, index } => engine
            .reorder(parse(id)?, *index)
            .map(Some)
            .map_err(message),
        TreeCommand::Move { id, parent, index } => engine
            .move_item(parse(id)?, parse(parent)?, *index)
            .map(Some)
            .map_err(message),
    }
}

fn parse(id: &str) -> Result<Id, String> {
    id.parse()
        .map_err(|error| format!("invalid entity id {id}: {error}"))
}
