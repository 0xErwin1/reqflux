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
//! Escape or a click outside closes it without writing; Escape is two-level while the "Add to"
//! list is open (the first closes the list). A field under edit whose blur arrived while the
//! popover was open commits when the popover closes with the focus elsewhere. After a write
//! the open requests are inspected again, so their marks show the new state. No value reaches
//! a log.

use std::time::Duration;

use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, FocusHandle, Focusable as _, Task, Window,
};
use gpui_component::input::InputState;
use reqflux_engine::{SecretScope, Span, VariableInfo};

use super::cell_edit::Refocus;
use super::settings_page::Section;
use super::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

/// How long the pointer rests on a mark before the popover opens.
// bolt gap: Popover has no hover-intent delay; the shell times it.
pub(crate) const HOVER_DELAY: Duration = Duration::from_millis(400);

/// The element id of the popover.
pub(crate) const POPOVER_ID: &str = "variable-popover";

/// The open popover over the reference `info.name`, anchored to the element `anchor`: a
/// mark, or the field under edit.
pub(crate) struct VariablePopover {
    pub(crate) anchor: String,
    pub(crate) info: VariableInfo,
    /// The value field: the current value, a secret's value once revealed, or a new value.
    pub(crate) input: Entity<InputState>,
    /// The popover's own focus, held when no value field shows, so Escape reaches it.
    pub(crate) focus: FocusHandle,
    /// The quick-add scope, an index into `info.targets`.
    pub(crate) target: usize,
    /// Whether the "Add to" list is open under its trigger.
    pub(crate) target_menu: bool,
    /// The quick-add Secret box.
    pub(crate) secret: bool,
    /// Whether a secret's value was read into the field.
    pub(crate) revealed: bool,
    /// Whether "Add to collection or globals instead" replaced the environment list.
    pub(crate) adding: bool,
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

impl VariablePopover {
    pub(crate) fn mode(&self) -> Mode {
        match &self.info.winner {
            Some(winner) if winner.secret => Mode::Secret,
            Some(_) => Mode::Edit,
            None if !self.info.environments.is_empty() && !self.adding => Mode::NoEnvironment,
            None => Mode::Add,
        }
    }

    /// The scopes a quick add can pick: every environment the collection allows, the
    /// collection and the globals.
    pub(crate) fn add_targets(&self) -> &[reqflux_engine::VariableScopeRef] {
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

/// The pending hover of the mark `anchor`, which opens its popover when the timer fires.
pub(crate) struct PendingHover {
    anchor: String,
    _timer: Task<()>,
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
    /// `request_id`: entering starts the hover delay, leaving cancels it.
    pub(super) fn hover_variable_mark(
        &mut self,
        request_id: &str,
        anchor: &str,
        name: &str,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            if self
                .variable_hover
                .as_ref()
                .is_some_and(|hover| hover.anchor == anchor)
            {
                self.variable_hover = None;
            }
            return;
        }
        if self
            .variable_popover
            .as_ref()
            .is_some_and(|popover| popover.anchor == anchor)
        {
            return;
        }
        let (request, mark, reference) =
            (request_id.to_owned(), anchor.to_owned(), name.to_owned());
        let timer = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let opened = this.update_in(cx, |this, window, cx| {
                this.variable_hover = None;
                this.open_variable_popover(&request, &mark, &reference, window, cx);
            });
            if let Err(error) = opened {
                log::debug!("the shell closed before the variable popover opened: {error}");
            }
        });
        self.variable_hover = Some(PendingHover {
            anchor: anchor.to_owned(),
            _timer: timer,
        });
    }

    /// Opens the popover of `name` in the request `request_id`, anchored to `anchor`, and
    /// focuses its value field with the value selected, so typing replaces it. A dynamic variable such as `$guid` has none.
    pub(super) fn open_variable_popover(
        &mut self,
        request_id: &str,
        anchor: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.variable_hover = None;
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
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
                return;
            }
        };
        if info.dynamic {
            return;
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
        let target = default_target(&info);
        let popover = VariablePopover {
            anchor: anchor.to_owned(),
            info,
            input,
            focus: cx.focus_handle(),
            target,
            target_menu: false,
            secret: false,
            revealed: false,
            adding: false,
        };
        if popover.mode() == Mode::NoEnvironment {
            popover.focus.focus(window, cx);
        } else {
            popover
                .input
                .update(cx, |state, cx| state.focus(window, cx));
        }
        self.variable_popover = Some(popover);
        cx.notify();
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

    /// Closes the popover without writing, and settles the focus once the event that closed
    /// it was handled, because a press outside the popover closes it first and then moves
    /// the focus to what it landed on. While the focus is still in the popover (or the window
    /// is inactive) it goes back to a field under edit, else to the shell. When the focus
    /// had left the popover, a blur the open field ignored is settled now: a field that no
    /// longer has the focus commits like Enter, and nothing is stranded open.
    pub(super) fn close_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popover) = self.variable_popover.take() else {
            return;
        };
        cx.notify();
        cx.defer_in(window, move |this, window, cx| {
            let held = popover.input.focus_handle(cx).is_focused(window)
                || popover.focus.contains_focused(window, cx);
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
                let Some(target) = popover.add_targets().get(popover.target) else {
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

    /// Picks the quick-add scope `index` of the popover and closes the "Add to" list.
    pub(super) fn pick_variable_target(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(popover) = self.variable_popover.as_mut() {
            popover.target = index;
            popover.target_menu = false;
        }
        cx.notify();
    }

    /// Opens the "Add to" list, or closes it when open.
    pub(super) fn toggle_variable_target_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(popover) = self.variable_popover.as_mut() {
            popover.target_menu = !popover.target_menu;
        }
        cx.notify();
    }

    /// Escape in the popover: closes the "Add to" list when it is open, else the popover.
    pub(super) fn escape_variable_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.variable_popover.as_mut() {
            Some(popover) if popover.target_menu => {
                popover.target_menu = false;
                cx.notify();
            }
            _ => self.close_variable_popover(window, cx),
        }
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
                    .info
                    .targets
                    .get(popover.target)
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
