//! The variable popover (T12, S10, design/variable-popover/01-inline-popover.html): a
//! `{{name}}` mark in the URL, a grid value cell or an auth field opens it after a 400 ms
//! hover or on a click, and while the field is edited Alt+click or F2 on a reference opens it.
//! It edits the value of the scope the name resolves from, as the engine reports it
//! (`Engine::variable_info`):
//!
//! - Enter saves the field as the current value of that scope, or a secret's value in the
//!   keyring; the workspace file is never written. Reset to initial clears the current value.
//! - An undefined name offers a quick add through an "Add to" dropdown listing every
//!   environment the collection allows (the active one first, marked, and preselected), then
//!   the collection and the globals, with the typed value as its initial value; a secret keeps
//!   only its key in the file.
//! - With no active environment, a name only environments declare lists them to activate.
//!
//! The surface is `bolt`'s form popover ([`VariablePopover::state`]): it owns Escape, a click
//! away, the focus in and back, the hover delay and the grace that keeps it open while the
//! pointer travels into it. Escape is two-level while the "Add to" list is open (the first
//! closes the list). A field under edit whose blur arrived while the popover was open commits
//! when the popover closes with the focus elsewhere. After a write the open requests are
//! inspected again, so their marks show the new state. No value reaches a log.

use bolt::composites::PopoverState;
use bolt::controls::{Dropdown, DropdownItem};
use gpui::{App, AppContext as _, ClipboardItem, Context, Entity, Focusable as _, Window};
use gpui_component::input::InputState;
use reqflux_engine::{SecretScope, Span, VariableInfo, VariableScopeRef};

use super::cell_edit::Refocus;
use super::settings_page::Section;
use super::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

/// The element id of the popover.
pub(crate) const POPOVER_ID: &str = "variable-popover";

/// The element id of the "Add to" dropdown of the quick add.
pub(crate) const TARGET_ID: &str = "variable-popover-target";

/// The open popover over the reference `info.name`, anchored to the element `anchor`: a
/// mark, or the field under edit.
pub(crate) struct VariablePopover {
    pub(crate) anchor: String,
    /// `bolt`'s surface: the open flag, the rectangle it points at, the focus and the hover
    /// timers.
    pub(crate) state: Entity<PopoverState>,
    pub(crate) info: VariableInfo,
    /// The value field: the current value, a secret's value once revealed, or a new value.
    pub(crate) input: Entity<InputState>,
    /// The "Add to" dropdown of the quick add; its selection is the scope.
    pub(crate) target_select: Entity<Dropdown>,
    /// The quick-add Secret box.
    pub(crate) secret: bool,
    /// Whether a secret's value was read into the field.
    pub(crate) revealed: bool,
    /// Whether "Add to collection or globals instead" replaced the environment list.
    pub(crate) adding: bool,
    /// Whether the surface was opened for this draft. A draft a hover prepared, still waiting
    /// out `bolt`'s delay, is not a surface that was dismissed.
    pub(crate) opened: bool,
}

/// What the popover of `info` shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The winning scope's value, editable.
    Edit,
    /// A secret's keyring value, masked until revealed.
    Secret,
    /// Not defined: quick add.
    Add,
    /// No active environment and only environments declare it.
    NoEnvironment,
}

/// What the popover of `info` shows, with `adding` set once the environment list was replaced
/// by the quick add.
pub(crate) fn mode(info: &VariableInfo, adding: bool) -> Mode {
    match &info.winner {
        Some(winner) if winner.secret => Mode::Secret,
        Some(_) => Mode::Edit,
        None if !info.environments.is_empty() && !adding => Mode::NoEnvironment,
        None => Mode::Add,
    }
}

impl VariablePopover {
    pub(crate) fn mode(&self) -> Mode {
        mode(&self.info, self.adding)
    }

    /// The quick-add scope the "Add to" dropdown selects, an index into `info.targets`.
    pub(crate) fn target_index(&self, cx: &App) -> Option<usize> {
        self.target_select.read(cx).selected()
    }

    /// The scopes a quick add can pick: every environment the collection allows, the
    /// collection and the globals.
    pub(crate) fn add_targets(&self) -> &[VariableScopeRef] {
        &self.info.targets
    }
}

/// The quick-add scope picked when the popover opens: the active environment, else the
/// collection.
fn default_target(info: &VariableInfo) -> usize {
    info.active_target
        .and_then(|scope| info.targets.iter().position(|target| target.scope == scope))
        .or_else(|| {
            info.targets
                .iter()
                .position(|target| matches!(target.scope, SecretScope::Collection(_)))
        })
        .unwrap_or(0)
}

/// The label of the quick-add scope `target`: an environment by name, marked when it is the
/// active one, the collection as `Collection · <name>`, or `Globals`.
fn target_label(target: &VariableScopeRef, info: &VariableInfo) -> String {
    match target.scope {
        SecretScope::Collection(_) => format!("Collection · {}", target.name),
        SecretScope::Global => target.name.clone(),
        SecretScope::Environment(_) if info.active_target == Some(target.scope) => {
            format!("{} · active", target.name)
        }
        SecretScope::Environment(_) => target.name.clone(),
    }
}

/// The rows of the "Add to" dropdown, labelled as the popover labels its scopes.
fn target_items(info: &VariableInfo) -> Vec<DropdownItem> {
    info.targets
        .iter()
        .map(|target| DropdownItem::new(target_label(target, info)))
        .collect()
}

/// Where a reference sits in a text: its name and byte range, `{{` to `}}`.
fn references(inspection: &reqflux_engine::Inspection, text: &str) -> Vec<(String, usize, usize)> {
    let mut start = 0;
    let mut found = Vec::new();
    for span in inspection.spans(text) {
        let length = match &span {
            Span::Literal(literal) => literal.len(),
            Span::Reference { raw, .. } => raw.len(),
        };
        if let Span::Reference { name, .. } = span {
            found.push((name, start, start + length));
        }
        start += length;
    }
    found
}

impl Shell {
    /// The pointer entered (`hovered`) or left the mark `anchor` of `name` in the request
    /// `request_id`: `bolt`'s surface opens it after its hover delay, and closes it once the
    /// pointer has left both the mark and the surface.
    pub(super) fn hover_variable_mark(
        &mut self,
        request_id: &str,
        anchor: &str,
        name: &str,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.variable_popover_state.clone();
        if !hovered {
            state.update(cx, |state, cx| state.hover_anchor(false, window, cx));
            let prepared = self
                .variable_popover
                .as_ref()
                .is_some_and(|popover| popover.anchor == anchor && !popover.opened);
            if prepared && !state.read(cx).is_open() {
                self.variable_popover = None;
                cx.notify();
            }
            return;
        }
        if self
            .variable_popover
            .as_ref()
            .is_some_and(|popover| popover.anchor == anchor && popover.opened)
        {
            state.update(cx, |state, cx| state.hover_anchor(true, window, cx));
            return;
        }
        // The surface opens itself when the delay elapses, so its draft and the rectangle it
        // points at are ready before the pointer rests that long.
        if self.prepare_variable_popover(request_id, anchor, name, window, cx) {
            state.update(cx, |state, cx| state.hover_anchor(true, window, cx));
        }
    }

    /// Opens the popover of `name` in the request `request_id`, anchored to `anchor`. A click
    /// opens it at once; the value field takes the focus with its value selected, so typing
    /// replaces it. A dynamic variable such as `$guid` has no popover.
    pub(super) fn open_variable_popover(
        &mut self,
        request_id: &str,
        anchor: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.prepare_variable_popover(request_id, anchor, name, window, cx) {
            return;
        }
        let state = self.variable_popover_state.clone();
        if let Some(popover) = self.variable_popover.as_mut() {
            popover.opened = true;
        }
        state.update(cx, |state, cx| state.open(window, cx));
        cx.notify();
    }

    /// Builds what the popover of `name` shows and points the surface at the rectangle of
    /// `anchor`, without opening it. Answers whether the name has a popover at all.
    fn prepare_variable_popover(
        &mut self,
        request_id: &str,
        anchor: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return false;
        };
        let info = match engine.variable_info(
            request_id,
            self.active_environment.as_deref(),
            &self.current,
            name,
        ) {
            Ok(info) => info,
            Err(error) => {
                log::warn!("cannot read the variable {name} of request {request_id}: {error}");
                return false;
            }
        };
        if info.dynamic {
            return false;
        }
        let (seed, placeholder) = match &info.winner {
            Some(winner) if winner.secret && winner.stored => {
                (String::new(), "Stored in the OS keyring")
            }
            Some(winner) if winner.secret => (String::new(), "No value stored"),
            Some(winner) => (winner.value().unwrap_or_default().to_owned(), ""),
            None => (String::new(), "Value"),
        };
        let secret = info.winner.as_ref().is_some_and(|winner| winner.secret);
        let input = cx.new(|cx| InputState::new(window, cx).masked(secret));
        input.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
            state.set_value(seed, window, cx);
            state.select_all(window, cx);
        });
        let shown = mode(&info, false);
        let target = default_target(&info);
        let items = target_items(&info);
        let target_select = cx.new(|_| {
            Dropdown::new(TARGET_ID)
                .items(items)
                .selected_index(Some(target))
                .aria_label("Add to")
        });
        let initial_focus = (shown != Mode::NoEnvironment).then(|| input.focus_handle(cx));
        let bounds = self.variable_anchors.borrow().get(anchor).copied();
        let state = self.variable_popover_state.clone();
        state.update(cx, |state, _cx| {
            state.set_initial_focus(initial_focus);
            state.set_anchor(bounds);
        });
        self.variable_popover = Some(VariablePopover {
            anchor: anchor.to_owned(),
            state,
            info,
            input,
            target_select,
            secret: false,
            revealed: false,
            adding: false,
            opened: false,
        });
        true
    }

    /// F2, or Alt+click, in the field `anchor` under edit holding `text` of the request
    /// `request_id`: opens the popover of the reference under the caret, if any.
    pub(super) fn open_variable_popover_at(
        &mut self,
        request_id: &str,
        anchor: &str,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, caret) = {
            let state = input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let Some(inspection) = self.requests.get(request_id).map(|state| &state.inspection) else {
            return;
        };
        let Some((name, _, _)) = references(inspection, &text)
            .into_iter()
            .find(|(_, start, end)| (*start..=*end).contains(&caret))
        else {
            return;
        };
        self.open_variable_popover(request_id, anchor, &name, window, cx);
    }

    /// Closes the popover without writing.
    pub(super) fn close_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.variable_popover_state.clone();
        state.update(cx, |state, cx| state.close(window, cx));
        self.dismiss_variable_popover(window, cx);
    }

    /// Drops the draft of a surface that is closed, once: `bolt` closes the surface itself on
    /// Escape, on a click away and when the pointer leaves a hovered mark, and the shell
    /// settles the focus after the event that closed it.
    pub(super) fn dismiss_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.variable_popover_state.read(cx).is_open() {
            return;
        }
        let dismissed = self
            .variable_popover
            .as_ref()
            .is_some_and(|popover| popover.opened);
        if !dismissed {
            return;
        }
        let Some(popover) = self.variable_popover.take() else {
            return;
        };
        cx.notify();
        cx.defer_in(window, move |this, window, cx| {
            // A surface opened over the closed one owns the focus now.
            if this.variable_popover.is_some() {
                return;
            }
            let held = popover.input.focus_handle(cx).is_focused(window)
                || this
                    .variable_popover_state
                    .read(cx)
                    .focus_handle()
                    .is_focused(window);
            if window.is_window_active() && !held {
                this.settle_fields_after_popover(window, cx);
                if window.focused(cx).is_none() {
                    this.focus.focus(window, cx);
                }
            } else if let Some(edit) = &this.url_edit {
                edit.input.update(cx, |state, cx| state.focus(window, cx));
            } else if let Some(edit) = &this.cell_edit {
                edit.input.update(cx, |state, cx| state.focus(window, cx));
            } else {
                this.focus.focus(window, cx);
            }
            cx.notify();
        });
    }

    /// Commits the open URL and cell fields that do not have the focus.
    fn settle_fields_after_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let blurred = |input: &Entity<InputState>, window: &Window, cx: &gpui::App| {
            !input.focus_handle(cx).is_focused(window)
        };
        if self
            .url_edit
            .as_ref()
            .is_some_and(|edit| blurred(&edit.input, window, cx))
        {
            self.commit_url_edit_as(Refocus::KeepIfTaken, window, cx);
        }
        if self
            .cell_edit
            .as_ref()
            .is_some_and(|edit| blurred(&edit.input, window, cx))
        {
            self.commit_cell_edit_as(Refocus::KeepIfTaken, window, cx);
        }
    }

    /// Enter: saves the field to the winning scope, or adds the variable.
    pub(super) fn save_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = &self.variable_popover else {
            return;
        };
        let value = popover.input.read(cx).value().to_string();
        let name = popover.info.name.clone();
        let saved = match popover.mode() {
            Mode::Edit | Mode::Secret => {
                let Some(winner) = &popover.info.winner else {
                    return;
                };
                if winner.secret && value.is_empty() {
                    return;
                }
                let scope = winner.at.scope;
                self.write_popover_value(scope, &name, &value)
            }
            Mode::Add => {
                let Some(index) = popover.target_index(cx) else {
                    return;
                };
                let Some(target) = popover.add_targets().get(index) else {
                    return;
                };
                let (scope, secret) = (target.scope, popover.secret);
                self.with_engine(|engine| {
                    engine
                        .quick_add_variable(scope, &name, &value, secret)
                        .map(|()| None)
                        .map_err(|error| error.to_string())
                })
                .map(|_| ())
            }
            Mode::NoEnvironment => return,
        };
        match saved {
            Ok(()) => {
                self.close_variable_popover(window, cx);
                self.inspect_open_requests(cx);
            }
            Err(error) => {
                log::warn!("cannot save the variable {name}");
                toasts::failure(format!("Cannot save {name}"), error, cx);
            }
        }
    }

    fn write_popover_value(
        &mut self,
        scope: SecretScope,
        name: &str,
        value: &str,
    ) -> Result<(), String> {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return Err("No workspace is open".to_owned());
        };
        let secret = self
            .variable_popover
            .as_ref()
            .and_then(|popover| popover.info.winner.as_ref())
            .is_some_and(|winner| winner.secret);
        let result = if value.is_empty() && !secret {
            engine
                .reset_variable_value(self.state.as_mut(), &mut self.current, scope, name)
                .map(|_| ())
        } else {
            engine.set_variable_value(self.state.as_mut(), &mut self.current, scope, name, value)
        };
        result.map_err(|error| error.to_string())
    }

    /// Reset to initial: clears the current value and puts the initial value in the field.
    pub(super) fn reset_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = &self.variable_popover else {
            return;
        };
        let Some(winner) = &popover.info.winner else {
            return;
        };
        let (scope, name, initial, input) = (
            winner.at.scope,
            popover.info.name.clone(),
            winner.initial.clone().unwrap_or_default(),
            popover.input.clone(),
        );
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        if let Err(error) =
            engine.reset_variable_value(self.state.as_mut(), &mut self.current, scope, &name)
        {
            log::warn!("cannot reset the variable {name}");
            toasts::failure(format!("Cannot reset {name}"), error.to_string(), cx);
            return;
        }
        input.update(cx, |state, cx| state.set_value(initial, window, cx));
        if let Some(winner) = self
            .variable_popover
            .as_mut()
            .and_then(|popover| popover.info.winner.as_mut())
        {
            winner.current = None;
        }
        self.inspect_open_requests(cx);
    }

    /// The eye of a secret: reads its keyring value into the field, unmasked, until the
    /// popover closes.
    pub(super) fn reveal_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = &self.variable_popover else {
            return;
        };
        let Some(winner) = popover.info.winner.as_ref().filter(|winner| winner.secret) else {
            return;
        };
        let (scope, name, input) = (
            winner.at.scope,
            popover.info.name.clone(),
            popover.input.clone(),
        );
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        match engine.reveal_variable(scope, &name) {
            Ok(value) => {
                input.update(cx, |state, cx| {
                    state.set_masked(false, window, cx);
                    state.set_value(value.unwrap_or_default(), window, cx);
                    state.focus(window, cx);
                });
                if let Some(popover) = self.variable_popover.as_mut() {
                    popover.revealed = true;
                }
            }
            Err(error) => {
                log::warn!("cannot read the secret {name}: {error}");
                toasts::failure(format!("Cannot reveal {name}"), error.to_string(), cx);
            }
        }
        cx.notify();
    }

    /// Copies the value that wins to the clipboard.
    pub(super) fn copy_variable_popover(&mut self, cx: &mut Context<Self>) {
        if let Some(value) = self
            .variable_popover
            .as_ref()
            .and_then(|popover| popover.info.winner.as_ref())
            .and_then(|winner| winner.value())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(value.to_owned()));
        }
    }

    /// Escape in the popover: closes the "Add to" list when it is open, so the first Escape
    /// closes the list and the next one the surface.
    pub(super) fn escape_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.close_variable_target(cx) {
            self.close_variable_popover(window, cx);
        }
    }

    /// Closes the "Add to" list when it is open. Answers whether it closed it.
    pub(super) fn close_variable_target(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(select) = self
            .variable_popover
            .as_ref()
            .map(|popover| popover.target_select.clone())
        else {
            return false;
        };
        if !select.read(cx).is_open() {
            return false;
        }
        select.update(cx, |select, cx| select.close(cx));
        true
    }

    /// Toggles the quick-add Secret box.
    pub(super) fn toggle_popover_secret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(popover) = self.variable_popover.as_mut() {
            popover.secret = !popover.secret;
            let masked = popover.secret;
            popover
                .input
                .update(cx, |state, cx| state.set_masked(masked, window, cx));
        }
        cx.notify();
    }

    /// "Add to collection or globals instead": the quick add, without the environments.
    pub(super) fn add_variable_instead(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(popover) = self.variable_popover.as_mut() {
            popover.adding = true;
            popover
                .input
                .update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    /// An environment of the list: makes it active and closes the popover.
    pub(super) fn activate_variable_environment(
        &mut self,
        slug: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_variable_popover(window, cx);
        self.set_environment(Some(slug.to_owned()), cx);
    }

    /// "Open in Environments": the settings page at the winning scope, else the collection.
    pub(super) fn open_variable_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = &self.variable_popover else {
            return;
        };
        let scope = popover
            .info
            .winner
            .as_ref()
            .map(|winner| winner.at.scope)
            .or_else(|| {
                popover
                    .info
                    .environments
                    .first()
                    .map(|choice| choice.at.scope)
            })
            .or_else(|| {
                popover
                    .target_index(cx)
                    .and_then(|index| popover.info.targets.get(index))
                    .map(|target| target.scope)
            });
        self.close_variable_popover(window, cx);
        let section = match scope {
            Some(SecretScope::Environment(id)) => Section::Environments(Some(id)),
            Some(SecretScope::Collection(id)) => Section::Collection(id),
            Some(SecretScope::Global) | None => Section::Globals,
        };
        self.open_settings(section, cx);
    }
}
