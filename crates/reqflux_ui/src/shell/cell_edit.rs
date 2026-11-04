//! The editable cell of the Params, Headers and body grids: clicking a row's key, value or
//! description cell swaps it for a focused text field seeded with the cell's text as the
//! draft has it, else as the file does. Enter commits the text into that row of the draft; Escape drops what
//! was typed, since nothing was written while typing; clicking another cell commits the
//! open field first, like a spreadsheet. A value that references a secret is never seeded:
//! the field opens empty with a `Secret value` placeholder and an empty commit keeps the
//! stored reference. The Variables document's cells use the same field; see
//! [`super::variables_ops`].
//!
//! Clicking outside the field, or the field losing focus, commits it like Enter through the
//! same close path, and the focus goes back to the shell unless something took it; the
//! variable popover that hangs under a value field is not outside, and while it is open the
//! field keeps its text. Closing the popover with the focus elsewhere commits the field then
//! (see [`Shell::close_variable_popover`]).

use gpui::{AppContext as _, Context, Entity, FocusHandle, Window};
use gpui_component::input::{InputEvent, InputState};

use super::Shell;
use super::auth_edit::{AUTH_TABLE, TYPED_PREFIX};
use super::drafts::grid_text;
use super::input_marks::InputMarks;
use super::method_edit::METHOD_TABLE;
use super::setting_edit::SETTINGS_TABLE;
use super::variables_model::Column;

/// Where the focus goes when a field closes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Refocus {
    /// To the shell, so its keys work: Enter and Escape in the field.
    Shell,
    /// To the shell only when nothing has the focus: the field lost it to something else.
    KeepIfTaken,
}

impl Refocus {
    pub(super) fn settle(self, shell: &FocusHandle, window: &mut Window, cx: &mut gpui::App) {
        if self == Self::Shell || window.focused(cx).is_none() {
            shell.focus(window, cx);
        }
    }
}

/// Which cell of a grid row the field edits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellField {
    /// The row's key.
    Key,
    /// The row's value.
    Value,
    /// The row's description.
    Description,
}

/// The open cell field: the cell it edits — the request, the table, the 1-based row and
/// the field — the input, and whether the cell's value references a secret.
pub(crate) struct CellEdit {
    pub(crate) request_id: String,
    pub(crate) table: String,
    pub(crate) n: usize,
    pub(crate) field: CellField,
    pub(crate) input: Entity<InputState>,
    /// The cell's value references a secret, so the field opened empty and an empty
    /// commit keeps the stored reference.
    pub(crate) secret: bool,
    /// The `{{name}}` marks of the field while it is edited; none on a secret field.
    pub(crate) marks: Option<InputMarks>,
}

impl CellEdit {
    /// Whether this edit is over the cell (`table`, 1-based row `n`, `field`) of `id`.
    fn over(&self, id: &str, table: &str, n: usize, field: CellField) -> bool {
        self.request_id == id && self.table == table && self.n == n && self.field == field
    }
}

impl Shell {
    /// Opens a focused field over the cell (`table`, 1-based row `n`, `field`) of the
    /// request `id`, seeded with the cell's text as the draft has it, else as the file
    /// does. A field over another cell is committed first, like a spreadsheet, so no edit
    /// is lost. A value that references a secret opens empty with a placeholder.
    pub(super) fn start_cell_edit(
        &mut self,
        id: &str,
        table: &str,
        n: usize,
        field: CellField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .cell_edit
            .as_ref()
            .is_some_and(|edit| edit.over(id, table, n, field))
        {
            return;
        }
        if self.cell_edit.is_some() {
            self.commit_cell_edit(window, cx);
        }
        let Some(state) = self.requests.get(id) else {
            return;
        };
        let mut request = self
            .displayed_request(id, &state.inspection.request.request)
            .clone();
        let Some(stored) = grid_text(&mut request, table, n, field) else {
            return;
        };
        // A value that references a secret opens empty; keys and descriptions are text.
        let secret = field == CellField::Value && state.inspection.references_secret(&stored);
        let text = if secret { String::new() } else { stored };
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if secret {
                state.set_placeholder("Secret value", window, cx);
            }
            state
        });

        input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.focus(window, cx);
        });
        // Keys and descriptions are plain text at rest too, so only a value shows its
        // references.
        let marks = (field == CellField::Value && !secret).then(|| self.mark_input(id, &input, cx));
        self.cell_edit = Some(CellEdit {
            request_id: id.to_owned(),
            table: table.to_owned(),
            n,
            field,
            input,
            secret,
            marks,
        });
        cx.notify();
    }

    /// Enter in the field: commits its text into that cell of the draft and closes the
    /// field. An empty commit on a secret cell keeps the stored reference.
    pub(super) fn commit_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit_as(Refocus::Shell, window, cx);
    }

    /// Commits the field like Enter, closing it with `refocus`. Enter and the field losing
    /// focus share this path, so whatever closing does happens for both.
    pub(super) fn commit_cell_edit_as(
        &mut self,
        refocus: Refocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.write_cell_edit(cx) {
            self.close_cell_edit_as(refocus, window, cx);
        }
    }

    /// Writes the open field's text into its cell, without closing the field; reports
    /// whether a field was open.
    fn write_cell_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((request_id, table, n, field, secret, input)) =
            self.cell_edit.as_ref().map(|edit| {
                (
                    edit.request_id.clone(),
                    edit.table.clone(),
                    edit.n,
                    edit.field,
                    edit.secret,
                    edit.input.clone(),
                )
            })
        else {
            return false;
        };
        let value = input.read(cx).value().to_string();
        if Column::from_table(&table).is_some() {
            self.commit_variable_cell(&request_id, &table, n, secret, value, cx);
            return true;
        }
        if table == SETTINGS_TABLE {
            self.set_draft_timeout(&request_id, &value, cx);
            return true;
        }
        if table == METHOD_TABLE {
            self.commit_custom_method(&request_id, &value, cx);
            return true;
        }
        if table == AUTH_TABLE {
            self.set_draft_auth_text(&request_id, secret, value, cx);
            return true;
        }
        if let Some(key) = table.strip_prefix(TYPED_PREFIX) {
            self.set_draft_typed_text(&request_id, key, secret, value, cx);
            return true;
        }
        self.set_draft_cell(&request_id, &table, n, field, value, secret, cx);
        true
    }

    /// The field lost focus: commits it, unless the variable popover is open, whose field
    /// legitimately holds the focus (closing the popover settles the field then), or the
    /// window was deactivated, which is not the user leaving the field. The focus is not
    /// taken back from whatever got it; when nothing has it the shell does, so its keys
    /// work.
    fn blur_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.variable_popover.is_some() || !window.is_window_active() {
            return;
        }
        self.commit_cell_edit_as(Refocus::KeepIfTaken, window, cx);
    }

    /// Follows the open cell field's focus, so that every cell field (the grids', the
    /// Variables document's, the auth fields', the custom verb's) commits when it loses it:
    /// subscribes to the field's input when a field opened since the last call.
    pub(super) fn watch_cell_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.cell_edit.as_ref().map(|edit| edit.input.clone());
        if self.cell_blur.as_ref().map(|(input, _)| input) == open.as_ref() {
            return;
        }
        self.cell_blur = open.map(|input| {
            let watch = cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur)
                        && this
                            .cell_edit
                            .as_ref()
                            .is_some_and(|edit| edit.input == *input)
                    {
                        this.blur_cell_edit(window, cx);
                    }
                },
            );
            (input, watch)
        });
    }

    /// Escape in the field: closes it and drops what was typed. Nothing was written to the
    /// draft while typing, so the stored value is untouched.
    pub(super) fn discard_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell_edit(window, cx);
    }

    /// Closes the field and gives the focus back to the shell, so the window's keys
    /// (Ctrl+Enter, Escape) keep reaching the shell's key context.
    pub(super) fn close_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell_edit_as(Refocus::Shell, window, cx);
    }

    /// Drops the field and settles the focus as `refocus` says.
    fn close_cell_edit_as(
        &mut self,
        refocus: Refocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cell_edit.take().is_some() {
            refocus.settle(&self.focus, window, cx);
            cx.notify();
        }
    }

    /// Drops the field when it no longer belongs to the active request `id`.
    pub(super) fn drop_stale_cell_edit(&mut self, id: &str) {
        if self
            .cell_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id != id)
        {
            self.cell_edit = None;
        }
    }

    /// Drops the field of the request `id` without committing: its tab closed, or the
    /// draft is being saved or reverted.
    pub(super) fn drop_cell_edit_of(&mut self, id: &str) {
        if self
            .cell_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
        {
            self.cell_edit = None;
        }
    }
}

#[cfg(test)]
#[path = "cell_edit_tests.rs"]
mod tests;
