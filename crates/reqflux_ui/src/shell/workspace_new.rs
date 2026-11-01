//! Creating a workspace the app keeps from the workspace menu (standalone-workspaces S3, S4):
//! `New workspace…` opens an inline name field in the menu, `workspace-new-name`; Enter, or a
//! click outside the field, creates a managed workspace with that name and opens it, Escape
//! closes the field. A click outside an empty field closes it. Recent
//! rows of managed workspaces read `In ReqFlux` in place of their path.

use std::path::PathBuf;

use bolt::controls::{GpuiInput, InputEnter as Enter, InputEscape as Escape, InputState};
use bolt::primitives::Chamfer;
use bolt::tokens::{ChamferCut, Fields, MenuMetrics, Spacing};
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement, IntoElement, ParentElement, Role,
    Styled, Window, prelude::*,
};
use reqflux_engine::state::RecentWorkspace;

use super::{Shell, region};

/// What a recent row shows in place of the path of a managed workspace.
pub(crate) const IN_REQFLUX: &str = "In ReqFlux";

/// The placeholder of the name field.
const NAME_PLACEHOLDER: &str = "Workspace name";

/// The directory managed workspaces live in: the per-user one in the app, none in tests, so
/// no test writes the user's data directory; tests set their own.
pub(super) fn default_managed_dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    match reqflux_engine::managed_workspaces_dir() {
        Ok(dir) => Some(dir),
        Err(error) => {
            log::warn!("cannot locate the app's workspaces: {error}");
            None
        }
    }
}

impl Shell {
    /// Opens the workspace menu with a focused, empty name field.
    pub(super) fn start_new_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.environment_menu_open = false;
        self.workspace_menu_open = true;
        self.refresh_recents();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        input.update(cx, |state, cx| state.focus(window, cx));
        self.new_workspace_name = Some(input);
        cx.notify();
    }

    /// Enter in the name field: creates a managed workspace with the name and opens it. A
    /// failure, an empty name too, keeps the field open over the workspace error.
    fn commit_new_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.new_workspace_name.as_ref() else {
            return;
        };
        let name = input.read(cx).value().trim().to_owned();
        let created = match self.managed_dir.as_deref() {
            Some(root) => reqflux_engine::create_managed_workspace_in(root, &name),
            None => Err(reqflux_engine::CreateWorkspaceError::NoDataDirectory),
        };
        match created {
            Ok((_, dir)) => {
                self.new_workspace_name = None;
                self.switch_workspace(Some(dir), window, cx);
            }
            Err(error) => {
                self.show_workspace_error(format!("Could not create the workspace: {error}"), cx);
            }
        }
    }

    /// A click outside the name field: creates the workspace as Enter does, or closes an
    /// empty field without the error an empty name gets on Enter.
    fn blur_new_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let empty = self
            .new_workspace_name
            .as_ref()
            .is_some_and(|input| input.read(cx).value().trim().is_empty());
        if empty {
            self.cancel_new_workspace(window, cx);
        } else {
            self.commit_new_workspace(window, cx);
        }
    }

    /// Escape in the name field: closes it and keeps the menu open. Returns whether a field
    /// was open.
    pub(super) fn cancel_new_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.new_workspace_name.take().is_none() {
            return false;
        }
        self.workspace_error = None;
        self.workspace_menu_focus.focus(window, cx);
        cx.notify();
        true
    }

    /// The text under the name of `recent`: `In ReqFlux` for a managed workspace, else its
    /// path.
    pub(super) fn recent_detail(&self, recent: &RecentWorkspace) -> String {
        let managed = self
            .managed_dir
            .as_deref()
            .is_some_and(|root| reqflux_engine::is_managed_in(root, &recent.path));
        if managed {
            IN_REQFLUX.to_owned()
        } else {
            recent.path.display().to_string()
        }
    }

    /// The name field, when it is open.
    pub(super) fn new_name_field(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.new_workspace_name.as_ref()?;
        let colors = bolt::colors(cx);
        Some(
            region("workspace-new-name")
                .role(Role::TextInput)
                .relative()
                .h(Fields::HEIGHT)
                .mx(MenuMetrics::ROW_INSET)
                .my(Spacing::XS)
                .flex()
                .items_center()
                .pl(Fields::PADDING_X)
                .pr(Fields::PADDING_X)
                .child(
                    Chamfer::new(ChamferCut::CONTROL)
                        .fill(colors.background)
                        .border(colors.line),
                )
                .on_action(cx.listener(|this, _: &Enter, window, cx| {
                    this.commit_new_workspace(window, cx);
                }))
                .on_action(cx.listener(|this, _: &Escape, window, cx| {
                    this.cancel_new_workspace(window, cx);
                }))
                .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                    this.blur_new_workspace(window, cx);
                }))
                // bolt gap: bolt's frameless Input draws its text in the medium weight with no
                // option.
                .child(
                    GpuiInput::new(input)
                        .appearance(false)
                        .bordered(false)
                        .p_0()
                        .flex_1()
                        .min_w_0()
                        .text_size(Fields::TEXT)
                        .text_color(colors.strong),
                )
                .into_any_element(),
        )
    }

    /// The name field's text, when it is open.
    #[cfg(test)]
    pub(super) fn new_name_text(&self, cx: &gpui::App) -> Option<String> {
        let input = self.new_workspace_name.as_ref()?;
        Some(input.read(cx).value().to_string())
    }
}
