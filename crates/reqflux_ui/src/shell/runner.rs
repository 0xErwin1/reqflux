//! The run document (v1-features S1, design/runner/01-run-document): a collection or folder
//! run in a document tab of its own beside the request tabs, opened from the tree's `Run…`,
//! the `Run collection…` command and the collection settings.
//!
//! - The setup names the target and its run order, and sets the environment (the window's,
//!   for this run only) and Stop on first failure. The engine has no delay between
//!   requests, so the setup has none.
//! - Run sends the requests through [`reqflux_engine::Engine::run_collection`] on a thread
//!   of its own, with a second connection to the app state so every send is recorded in
//!   history; its events reach the document as the run goes. Stop cancels the run's token:
//!   the send in flight is cancelled and nothing more is sent.
//! - The results grid is `bolt`'s grid, one row per step, then the requests not reached,
//!   queued or skipped. Selecting a finished row shows its tests beside its recorded
//!   response under the grid.
//! - When the run ends, the current values its scripts set are persisted, as a single send
//!   persists its script writes, and the History panel is read again.
//!
//! Element ids start with `runner-`. Nothing here shows a secret: the engine masks the URL,
//! errors and test text, and the response is the masked one history stored.

mod grid;
mod model;
mod result;
mod run;
mod setup;
mod view;

use std::collections::HashMap;

use gpui::{Context, EventEmitter};

use super::Shell;
use super::sidebar::Sidebar;
use super::tree_model::{self, NodeKind};
use crate::workspace::OpenedWorkspace;

pub(super) use model::RunModel;
pub(super) use view::{render, settings_run_button, tab_chip};

/// Emitted by the sidebar when its row menu's `Run…` is picked on the collection or folder
/// with this id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRequested(pub(crate) String);

impl EventEmitter<RunRequested> for Sidebar {}

/// The run document: its model, whether it is the active document, and the run in flight.
pub(crate) struct RunTab {
    pub(crate) model: RunModel,
    /// Whether the tab is the active document.
    pub(crate) active: bool,
    /// Whether the setup's environment menu is open.
    pub(crate) environment_menu: bool,
    /// The key of each finished step's recorded response in the shell's sends.
    pub(crate) responses: HashMap<usize, String>,
    running: Option<run::Running>,
}

impl RunTab {
    /// Whether a run is in flight.
    pub(crate) fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

impl Shell {
    /// Whether the run document is the active document.
    pub(super) fn run_tab_active(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.active)
    }

    /// Leaves the run document in the tab row, inactive.
    pub(super) fn leave_run_tab(&mut self) {
        if let Some(run) = self.run.as_mut() {
            run.active = false;
            run.environment_menu = false;
        }
    }

    /// Activates the run document, leaving the views and the history tab it replaces.
    pub(super) fn activate_run_tab(&mut self, cx: &mut Context<Self>) {
        self.leave_approvals(cx);
        self.leave_audit(cx);
        self.leave_history_tab();
        if let Some(run) = self.run.as_mut() {
            run.active = true;
        }
        cx.notify();
    }

    /// Opens the setup of a run over the collection or folder `target` and activates it. An
    /// open run document over the same target is activated as it is; one over another
    /// target is replaced, cancelling its run in flight.
    pub(super) fn open_run(&mut self, target: &str, cx: &mut Context<Self>) {
        if self
            .run
            .as_ref()
            .is_some_and(|run| run.model.target == target)
        {
            self.activate_run_tab(cx);
            return;
        }
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let items = match engine.run_plan(target) {
            Ok(items) => items,
            Err(error) => {
                log::warn!("cannot plan a run of {target}: {error}");
                return;
            }
        };
        let environment = self.active_environment.clone().zip(self.environment_name());
        let Some(model) = RunModel::new(&self.roots, target, &items, environment) else {
            return;
        };
        self.close_run_tab(cx);
        self.run = Some(RunTab {
            model,
            active: false,
            environment_menu: false,
            responses: HashMap::new(),
            running: None,
        });
        self.activate_run_tab(cx);
    }

    /// `Run collection…`: opens the setup of the collection or folder selected in the tree,
    /// the parent of a selected request, else the first collection.
    pub(super) fn open_run_for_selection(&mut self, cx: &mut Context<Self>) {
        let target = {
            let sidebar = self.sidebar.read(cx);
            sidebar
                .selected()
                .and_then(
                    |id| match tree_model::find(&self.roots, id).map(|node| &node.kind) {
                        Some(NodeKind::Request { .. }) => sidebar.parent(id),
                        Some(NodeKind::Collection | NodeKind::Folder) => Some(id.to_owned()),
                        None => None,
                    },
                )
                .or_else(|| self.roots.first().map(|root| root.id.clone()))
        };
        if let Some(target) = target {
            self.open_run(&target, cx);
        }
    }

    /// Closes the run document, cancelling its run in flight, and drops its responses.
    pub(super) fn close_run_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.run.take() {
            for key in run.responses.values() {
                self.sends.remove(key);
            }
        }
        cx.notify();
    }

    /// Selects the finished step `step`, or clears the selection when it is selected.
    pub(super) fn select_run_step(&mut self, step: usize, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut()
            && run.model.result(step).is_some()
        {
            run.model.selected = (run.model.selected != Some(step)).then_some(step);
            cx.notify();
        }
    }

    /// Clears the selected step, closing its result.
    pub(super) fn close_run_result(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut() {
            run.model.selected = None;
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
