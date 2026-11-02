//! The window's current values (variables-editor S4, S5): read from the app state when the
//! shell opens, passed to every send and inspection, and updated by the script writes of
//! each completed send.

use gpui::Context;
use reqflux_engine::{ExecuteOptions, VariableWrite};

use super::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

impl Shell {
    /// Reads the workspace's current values from the app state. A failure is logged and
    /// leaves them empty, so sends resolve the shared values.
    pub(super) fn load_current_values(&mut self) {
        let (Some(state), Some(workspace_id)) = (self.state.as_ref(), self.workspace_id.as_deref())
        else {
            return;
        };
        match state.current_values(workspace_id) {
            Ok(values) => self.current = values,
            Err(error) => {
                log::warn!("cannot read the current values of workspace {workspace_id}: {error}");
            }
        }
    }

    /// Persists the script writes of a completed send of the request `id` with `options` as
    /// current values, then inspects the open requests again so they show the new values.
    /// A failure shows in an error toast without values and never reaches the send; without
    /// the app state nothing is persisted.
    pub(super) fn persist_script_writes(
        &mut self,
        id: &str,
        options: &ExecuteOptions,
        writes: &[VariableWrite],
        cx: &mut Context<Self>,
    ) {
        if writes.is_empty() {
            return;
        }
        let (OpenedWorkspace::Loaded(engine), Some(state)) = (&self.workspace, self.state.as_mut())
        else {
            return;
        };
        if let Err(error) =
            engine.persist_script_writes(state, &mut self.current, id, options, writes)
        {
            log::warn!("cannot persist the script writes of the send of {id}");
            toasts::failure(
                "Cannot remember the values the scripts set",
                error.to_string(),
                cx,
            );
        }
        self.inspect_open_requests(cx);
    }
}
