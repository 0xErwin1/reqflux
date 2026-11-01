//! Creating a workspace the app keeps from the workspace menu (standalone-workspaces S3, S4;
//! app-pending S6): `New workspace…` opens the name popover beside the menu,
//! `workspace-new-popover`, as workspace-menu/01 draws it: the title, the `Name` field
//! `workspace-new-name`, the hint, `Cancel` (`workspace-new-cancel`) and `Create`
//! (`workspace-new-create`). Enter or Create makes a managed workspace with that name and
//! opens it; Escape, Cancel or a click outside the popover closes it and creates nothing.
//! Recent rows of managed workspaces read `In ReqFlux` in place of their path.

use std::path::PathBuf;

use bolt::composites::menu_frame;
use bolt::controls::{Button, Input, InputEnter as Enter, InputEscape as Escape, InputState};
use bolt::modals::modal_field;
use bolt::primitives::{Chamfer, TextVariant};
use bolt::tokens::{ChamferCut, Fields, ModalMetrics, Spacing};
use gpui::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, Role, Styled, Window, div, prelude::*,
};
use reqflux_engine::state::RecentWorkspace;

use super::request::ui;
use super::{Shell, region, text_region};
use crate::tokens;

/// What a recent row shows in place of the path of a managed workspace.
pub(crate) const IN_REQFLUX: &str = "In ReqFlux";

/// The title of the name popover.
pub(crate) const POPOVER_TITLE: &str = "New workspace";

/// The label of the name field.
const NAME_LABEL: &str = "Name";

/// The placeholder of the name field.
const NAME_PLACEHOLDER: &str = "Workspace name";

/// The note under the name field.
pub(crate) const POPOVER_HINT: &str = "ReqFlux keeps it in its data folder. To keep requests \
                                       in a repository, use New workspace in folder.";

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

    /// Escape, Cancel or a click outside: closes the popover and keeps the menu open.
    /// Returns whether the popover was open.
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

    /// The name popover, `workspace-new-popover`, while it is open: `bolt`'s menu chrome
    /// holding the title, the name field, the hint and the buttons.
    // bolt gap: no form popover; the popover is `menu_frame` with dialog parts inside.
    pub(super) fn new_workspace_popover(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.new_workspace_name.as_ref()?;
        let colors = bolt::colors(cx);
        let field = region("workspace-new-name")
            .role(Role::TextInput)
            .relative()
            .h(Fields::HEIGHT)
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
            .text_color(colors.strong)
            .child(
                Input::new(input)
                    .appearance(false)
                    .font_weight(FontWeight::NORMAL)
                    .padding(Pixels::ZERO, Pixels::ZERO)
                    .text_size(Fields::TEXT),
            );
        let cancel = Button::new("workspace-new-cancel", "Cancel")
            .secondary()
            .on_click(cx.listener(|this, _event, window, cx| {
                this.cancel_new_workspace(window, cx);
            }));
        let create = Button::new("workspace-new-create", "Create")
            .primary()
            .kbd("↵")
            .on_click(cx.listener(|this, _event, window, cx| {
                this.commit_new_workspace(window, cx);
            }));
        Some(
            menu_frame(cx)
                .id("workspace-new-popover")
                .debug_selector(|| "workspace-new-popover".to_owned())
                .role(Role::Dialog)
                .aria_label(POPOVER_TITLE)
                .occlude()
                .w(tokens::NEW_WORKSPACE_POPOVER_WIDTH)
                .p(Spacing::MD)
                .gap(ModalMetrics::HEADER_GAP)
                .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                    this.cancel_new_workspace(window, cx);
                }))
                .child(
                    ui::role(text_region("workspace-new-title"), TextVariant::Heading, cx)
                        .aria_label(POPOVER_TITLE)
                        .child(POPOVER_TITLE),
                )
                .child(modal_field(NAME_LABEL, field, cx))
                .child(
                    ui::role(text_region("workspace-new-hint"), TextVariant::Caption, cx)
                        .aria_label(POPOVER_HINT)
                        .child(POPOVER_HINT),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(Spacing::XXS)
                        .child(cancel)
                        .child(create),
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
