//! The active environment of each workspace, kept across restarts and switches
//! (standalone-workspaces S8): the app state setting `active_environment/<workspace id>` holds
//! the environment id, so a rename keeps it; an empty value is No environment. An id that no
//! longer exists falls back to No environment.

use super::Shell;
use crate::workspace::OpenedWorkspace;

/// The app state setting that holds the active environment of `workspace_id`.
fn setting_key(workspace_id: &str) -> String {
    format!("active_environment/{workspace_id}")
}

impl Shell {
    /// Stores the active environment of the loaded workspace in the app state.
    pub(super) fn save_active_environment(&mut self) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let (Some(workspace_id), Some(state)) = (self.workspace_id.as_deref(), self.state.as_mut())
        else {
            return;
        };
        let id = self.active_environment.as_deref().and_then(|slug| {
            engine
                .workspace()
                .environments
                .iter()
                .find(|node| node.slug == slug)
                .map(|node| node.environment.id.to_string())
        });
        if let Err(error) =
            state.set_setting(&setting_key(workspace_id), id.as_deref().unwrap_or(""))
        {
            log::warn!("cannot save the active environment: {error}");
        }
    }

    /// Makes the stored environment of the loaded workspace active, or No environment when
    /// none is stored or it no longer exists.
    pub(super) fn restore_active_environment(&mut self) {
        self.active_environment = None;
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let (Some(workspace_id), Some(state)) = (self.workspace_id.as_deref(), self.state.as_ref())
        else {
            return;
        };
        let stored = match state.setting(&setting_key(workspace_id)) {
            Ok(stored) => stored,
            Err(error) => {
                log::warn!("cannot read the active environment: {error}");
                return;
            }
        };
        let Some(stored) = stored.filter(|id| !id.is_empty()) else {
            return;
        };
        self.active_environment = engine
            .workspace()
            .environments
            .iter()
            .find(|node| node.environment.id.to_string() == stored)
            .map(|node| node.slug.clone());
    }
}
