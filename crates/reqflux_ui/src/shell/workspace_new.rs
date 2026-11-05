//! Creating a workspace the app keeps from the workspace menu (standalone-workspaces S3, S4;
//! app-pending S6): `New workspace…` opens the name popover beside the menu,
//! `workspace-new-popover`, as workspace-menu/01 draws it: `bolt`'s form popover, titled
//! `New workspace`, with the `Name` field `workspace-new-name`, the hint, `Cancel`
//! (`workspace-new-cancel`) and `Create` (`workspace-new-create`). Enter or Create makes a
//! managed workspace with that name and opens it; Escape, Cancel or a click outside the
//! popover closes it and creates nothing. Recent rows of managed workspaces read `In ReqFlux`
//! in place of their path.

use std::path::PathBuf;

use bolt::composites::{Popover, PopoverState};
use bolt::controls::{Button, Input, InputEvent, InputState};
use bolt::modals::modal_field;
use bolt::primitives::TextVariant;
use bolt::tokens::PopoverMetrics;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement,
    Styled, Subscription, Window, div, prelude::*,
};
use reqflux_engine::state::RecentWorkspace;

use super::request::ui;
use super::{Shell, text_region};
use crate::tokens;

/// What a recent row shows in place of the path of a managed workspace.
pub(crate) const IN_REQFLUX: &str = "In ReqFlux";

/// The title of the name popover.
pub(crate) const POPOVER_TITLE: &str = "New workspace";

/// The element id of the name popover.
pub(crate) const POPOVER_ID: &str = "workspace-new-popover";

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

/// The surface of the name popover, `bolt`'s, and what the shell keeps of it: the field's
/// Enter and the surface closing itself on Escape or on a click away both reach the shell
/// through them.
pub(super) struct NamePopover {
    pub(super) surface: Entity<PopoverState>,
    _changes: Subscription,
    _events: Subscription,
}

impl Shell {
    /// Opens the workspace menu with a focused, empty name field.
    pub(super) fn start_new_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.environment_menu_open = false;
        self.workspace_menu_open = true;
        self.refresh_recents();
        self.new_workspace_surface = None;
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let focus = input.focus_handle(cx);
        let surface = cx.new(|cx| {
            let mut surface = PopoverState::new(cx);
            surface.set_initial_focus(Some(focus));
            surface
        });
        self.new_workspace_surface = Some(NamePopover {
            surface: surface.clone(),
            _changes: cx.observe(&surface, |_, _, cx| cx.notify()),
            _events: cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.commit_new_workspace(window, cx);
                }
            }),
        });
        self.new_workspace_name = Some(input);
        surface.update(cx, |surface, cx| surface.open(window, cx));
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
                self.new_workspace_surface = None;
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
        self.new_workspace_surface = None;
        self.workspace_error = None;
        self.workspace_menu_focus.focus(window, cx);
        cx.notify();
        true
    }

    /// Drops the name popover once `bolt`'s surface closed itself, or with the menu it sits
    /// beside.
    pub(super) fn dismiss_new_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = self.new_workspace_surface.as_ref() else {
            return;
        };
        let open = self.workspace_menu_open
            && self.new_workspace_name.is_some()
            && popover.surface.read(cx).is_open();
        if open {
            return;
        }
        self.cancel_new_workspace(window, cx);
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

    /// The name popover, `workspace-new-popover`, while it is open: `bolt`'s form popover
    /// beside the workspace menu, its bottom on the menu's bottom.
    pub(super) fn new_workspace_popover(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.new_workspace_name.as_ref()?;
        let surface = self.new_workspace_surface.as_ref()?;
        let name = Input::new(input)
            .id("workspace-new-name")
            .aria_label(NAME_LABEL)
            .w_full();
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
        let body = div()
            .flex()
            .flex_col()
            .gap(PopoverMetrics::GAP)
            .child(modal_field(NAME_LABEL, name, cx))
            .child(
                ui::role(text_region("workspace-new-hint"), TextVariant::Caption, cx)
                    .aria_label(POPOVER_HINT)
                    .child(POPOVER_HINT),
            );
        Some(
            // Beside the menu, its top at the menu's bottom: the surface places itself, so it
            // takes no room in the menu's row.
            div()
                .absolute()
                .top_full()
                .left_full()
                .child(
                    Popover::new(POPOVER_ID, surface.surface.clone())
                        .title(POPOVER_TITLE)
                        .body(body)
                        .footer(vec![cancel, create])
                        .width(tokens::NEW_WORKSPACE_POPOVER_WIDTH),
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
