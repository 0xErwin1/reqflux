//! Running the variable edits of the workspace settings page (variables-editor S3, S6, S7;
//! design-catch-up T7) through the engine. Variables are not drafts: every commit writes at
//! once and the open requests are inspected again. A refused write shows its reason on the
//! page and changes nothing.
//!
//! The grid's cells edit through the shell's cell field slot, under the table names of
//! [`Column`] and the scope's path as the owner, so they commit, discard and give the focus
//! back as a grid cell does. A secret has no value in the file: its current value cell edits
//! the keyring through a field that opens empty, and an empty commit keeps the value; the
//! value is never seeded or logged.

use gpui::{AppContext as _, Context, Window};
use gpui_component::input::InputState;
use reqflux_core::model::Variable;
use reqflux_engine::{Engine, SecretScope, VariableEdit, VariableError};

use super::Shell;
use super::cell_edit::{CellEdit, CellField};
use super::variables_model::{Column, current_keys, new_key, scope_variables, undeclared};
use crate::workspace::OpenedWorkspace;

/// The warning shown after a variable stops being secret.
pub(crate) const UNMARKED: &str =
    "The secret value was not copied to the file; set a shared value if one is needed";

impl Shell {
    /// The variables `scope` declares, cloned.
    pub(super) fn variables_of(&self, scope: SecretScope) -> Vec<Variable> {
        match &self.workspace {
            OpenedWorkspace::Loaded(engine) => scope_variables(engine.workspace(), scope)
                .map(<[Variable]>::to_vec)
                .unwrap_or_default(),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
        }
    }

    /// Runs the variable write `write`; on success the error clears, `warning` shows and the
    /// open requests are inspected again. Returns whether it succeeded.
    fn write_variables(
        &mut self,
        write: impl FnOnce(&mut Engine) -> Result<(), String>,
        warning: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcome = self.with_engine(|engine| write(engine).map(|()| None));
        match outcome {
            Ok(_) => {
                self.show_variables_warning(warning, cx);
                self.inspect_open_requests(cx);
                true
            }
            Err(error) => {
                self.show_variables_error(error, cx);
                false
            }
        }
    }

    fn edit_variable(
        &mut self,
        scope: SecretScope,
        n: usize,
        edit: VariableEdit,
        warning: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = n.checked_sub(1) else {
            return;
        };
        self.write_variables(
            |engine| engine.edit_variable(scope, index, edit).map_err(message),
            warning,
            cx,
        );
    }

    /// The `Add` row: appends an enabled variable named [`new_key`] to `scope` and opens
    /// its key cell.
    pub(super) fn add_variable(
        &mut self,
        scope: SecretScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let variables = self.variables_of(scope);
        let mut variable = Variable::new(new_key(&variables), "");
        variable.value = None;
        if self.write_variables(
            |engine| engine.add_variable(scope, variable).map_err(message),
            None,
            cx,
        ) {
            let n = variables.len() + 1;
            self.start_variable_edit(scope, Column::Key, n, window, cx);
        }
    }

    /// The enable box of the 1-based row `n`.
    pub(super) fn toggle_variable_enabled(
        &mut self,
        scope: SecretScope,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(enabled) = self.variable(scope, n).map(|variable| variable.enabled) else {
            return;
        };
        self.edit_variable(scope, n, VariableEdit::Enabled(!enabled), None, cx);
    }

    /// The `stored in` toggle of the 1-based row `n`, between the file and the keyring.
    /// Moving a secret back to the file warns that the value was not copied there.
    pub(super) fn toggle_variable_secret(
        &mut self,
        scope: SecretScope,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(secret) = self.variable(scope, n).map(|variable| variable.secret) else {
            return;
        };
        let warning = secret.then(|| UNMARKED.to_owned());
        self.edit_variable(scope, n, VariableEdit::Secret(!secret), warning, cx);
    }

    /// The delete control of the 1-based row `n`.
    pub(super) fn delete_variable(
        &mut self,
        scope: SecretScope,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(index) = n.checked_sub(1) else {
            return;
        };
        self.write_variables(
            |engine| engine.delete_variable(scope, index).map_err(message),
            None,
            cx,
        );
    }

    /// `Declare` on the 1-based undeclared row `n`: the key goes into the scope's file with
    /// no initial value, and its current value stays on this machine.
    pub(super) fn declare_undeclared(
        &mut self,
        scope: SecretScope,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(key) = self.undeclared_key(scope, n) else {
            return;
        };
        self.write_variables(
            |engine| engine.declare_variable(scope, &key).map_err(message),
            None,
            cx,
        );
    }

    /// The discard control of the 1-based undeclared row `n`: clears its current value.
    pub(super) fn discard_undeclared(
        &mut self,
        scope: SecretScope,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if let Some(key) = self.undeclared_key(scope, n) {
            self.set_current_value(scope, &key, String::new(), cx);
        }
    }

    /// `Reset current values`: clears the current value of every variable `scope` declares,
    /// so the initial values resolve again. Undeclared values stay.
    pub(super) fn reset_current_values(
        &mut self,
        scope: SecretScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let variables = self.variables_of(scope);
        for key in current_keys(&self.current, scope, &variables) {
            self.set_current_value(scope, &key, String::new(), cx);
        }
    }

    fn undeclared_key(&self, scope: SecretScope, n: usize) -> Option<String> {
        let variables = self.variables_of(scope);
        undeclared(&self.current, scope, &variables)
            .into_iter()
            .nth(n.checked_sub(1)?)
            .map(|(key, _)| key)
    }

    fn variable(&self, scope: SecretScope, n: usize) -> Option<Variable> {
        let index = n.checked_sub(1)?;
        self.variables_of(scope).get(index).cloned()
    }

    /// Opens a focused field over the cell `column` of the 1-based row `n` of `scope`,
    /// seeded with its text; a secret's current value opens empty with a placeholder, and
    /// its initial value, which is not in the file, does not open. An open field elsewhere
    /// commits first.
    pub(super) fn start_variable_edit(
        &mut self,
        scope: SecretScope,
        column: Column,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = scope.path();
        if self.cell_edit.as_ref().is_some_and(|edit| {
            edit.request_id == owner && edit.table == column.table() && edit.n == n
        }) {
            return;
        }
        self.commit_cell_edit(window, cx);
        let index = n.saturating_sub(1);
        let (seed, secret) = if column == Column::Undeclared {
            let variables = self.variables_of(scope);
            let Some((_, value)) = undeclared(&self.current, scope, &variables)
                .into_iter()
                .nth(index)
            else {
                return;
            };
            (value, false)
        } else {
            let Some(variable) = self.variable(scope, n) else {
                return;
            };
            match column {
                Column::Key => (variable.key, false),
                Column::Value if variable.secret => return,
                Column::Value => (variable.value.unwrap_or_default(), false),
                Column::Current if variable.secret => (String::new(), true),
                Column::Current | Column::Undeclared => (
                    self.current
                        .get(scope, &variable.key)
                        .map(str::to_owned)
                        .unwrap_or_default(),
                    false,
                ),
                Column::Description => (variable.description.unwrap_or_default(), false),
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
            state.set_value(seed, window, cx);
            state.focus(window, cx);
        });
        self.cell_edit = Some(CellEdit {
            request_id: owner,
            table: column.table().to_owned(),
            n,
            field: CellField::Value,
            input,
            secret,
        });
        cx.notify();
    }

    /// Commits and drops an open variables field without a window, when the page changes
    /// section or closes.
    pub(super) fn commit_variable_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self
            .cell_edit
            .take_if(|edit| Column::from_table(&edit.table).is_some())
        else {
            return;
        };
        let value = edit.input.read(cx).value().to_string();
        self.commit_variable_cell(
            &edit.request_id,
            &edit.table,
            edit.n,
            edit.secret,
            value,
            cx,
        );
    }

    /// Writes `value` into the cell (`table`, 1-based row `n`) of the scope at `owner`;
    /// does nothing when `table` is not a variables grid column.
    pub(super) fn commit_variable_cell(
        &mut self,
        owner: &str,
        table: &str,
        n: usize,
        secret: bool,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let (Some(column), Some(scope)) =
            (Column::from_table(table), SecretScope::from_path(owner))
        else {
            return;
        };
        if column == Column::Undeclared {
            if let Some(key) = self.undeclared_key(scope, n) {
                self.set_current_value(scope, &key, value, cx);
            }
            return;
        }
        let Some(variable) = self.variable(scope, n) else {
            return;
        };
        let optional = |value: String| (!value.is_empty()).then_some(value);
        let edit = match column {
            Column::Key if value != variable.key => VariableEdit::Key(value),
            Column::Current if secret => match optional(value) {
                Some(value) => VariableEdit::Value(Some(value)),
                None => return,
            },
            Column::Value if !variable.secret && optional(value.clone()) != variable.value => {
                VariableEdit::Value(optional(value))
            }
            Column::Description if optional(value.clone()) != variable.description => {
                VariableEdit::Description(optional(value))
            }
            Column::Current if !variable.secret => {
                self.set_current_value(scope, &variable.key, value, cx);
                return;
            }
            Column::Key
            | Column::Value
            | Column::Description
            | Column::Current
            | Column::Undeclared => return,
        };
        self.edit_variable(scope, n, edit, None, cx);
    }

    /// Sets the current value of `key` in `scope` in the app state and the shell's copy; an
    /// empty `value` clears it, so the shared value resolves again.
    fn set_current_value(
        &mut self,
        scope: SecretScope,
        key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let (Some(state), Some(workspace_id)) =
            (self.state.as_mut(), self.workspace_id.as_deref())
        {
            let stored = if value.is_empty() {
                state
                    .clear_current_value(workspace_id, scope, key)
                    .map(|_| ())
            } else {
                state.set_current_value(workspace_id, scope, key, &value)
            };
            if let Err(error) = stored {
                self.show_variables_error(error.to_string(), cx);
                return;
            }
        }
        if value.is_empty() {
            self.current.remove(scope, key);
        } else {
            self.current.insert(scope, key, value);
        }
        self.show_variables_warning(None, cx);
        self.inspect_open_requests(cx);
    }
}

/// The user-facing text of a refused variable write.
fn message(error: VariableError) -> String {
    error.to_string()
}
