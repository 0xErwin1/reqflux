//! The editable cell of the Params, Headers and body grids: clicking a row's key or value cell
//! swaps it for a focused text field seeded with the cell's text as the draft has it, else
//! as the file does. Enter commits the text into that row of the draft; Escape drops what
//! was typed, since nothing was written while typing; clicking another cell commits the
//! open field first, like a spreadsheet. A value that references a secret is never seeded:
//! the field opens empty with a `Secret value` placeholder and an empty commit keeps the
//! stored reference. The Variables document's cells use the same field; see
//! [`super::variables_ops`].

use gpui::{AppContext as _, Context, Entity, Window};
use gpui_component::input::InputState;

use super::Shell;
use super::auth_edit::AUTH_TABLE;
use super::drafts::grid_cell;
use super::method_edit::METHOD_TABLE;
use super::setting_edit::SETTINGS_TABLE;
use super::variables_model::Column;

/// Which cell of a grid row the field edits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellField {
    /// The row's key.
    Key,
    /// The row's value.
    Value,
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
        let Some((key, value, _)) = grid_cell(&mut request, table, n) else {
            return;
        };
        let (text, secret) = match field {
            CellField::Key => (key.clone(), false),
            CellField::Value => {
                let secret = state.inspection.references_secret(value);
                if secret {
                    (String::new(), true)
                } else {
                    (value.clone(), false)
                }
            }
        };
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
        self.cell_edit = Some(CellEdit {
            request_id: id.to_owned(),
            table: table.to_owned(),
            n,
            field,
            input,
            secret,
        });
        cx.notify();
    }

    /// Enter in the field: commits its text into that cell of the draft and closes the
    /// field. An empty commit on a secret cell keeps the stored reference.
    pub(super) fn commit_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            return;
        };
        let value = input.read(cx).value().to_string();
        if Column::from_table(&table).is_some() {
            self.commit_variable_cell(&request_id, &table, n, secret, value, cx);
            self.close_cell_edit(window, cx);
            return;
        }
        if table == SETTINGS_TABLE {
            self.set_draft_timeout(&request_id, &value, cx);
            self.close_cell_edit(window, cx);
            return;
        }
        if table == METHOD_TABLE {
            self.commit_custom_method(&request_id, &value, cx);
            self.close_cell_edit(window, cx);
            return;
        }
        if table == AUTH_TABLE {
            self.set_draft_auth_text(&request_id, secret, value, cx);
            self.close_cell_edit(window, cx);
            return;
        }
        self.set_draft_cell(&request_id, &table, n, field, value, secret, cx);
        self.close_cell_edit(window, cx);
    }

    /// Escape in the field: closes it and drops what was typed. Nothing was written to the
    /// draft while typing, so the stored value is untouched.
    pub(super) fn discard_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell_edit(window, cx);
    }

    /// Closes the field and gives the focus back to the shell, so the window's keys
    /// (Ctrl+Enter, Escape) keep reaching the shell's key context.
    pub(super) fn close_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cell_edit.take().is_some() {
            self.focus.focus(window, cx);
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
