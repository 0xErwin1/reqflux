//! Editing scope variables and environments (variables-editor S1-S3): each edit goes
//! through the store, hash-checked, and the workspace reloads after it.
//!
//! A secret's value lives in the [`SecretStore`](crate::SecretStore), never in a file, so
//! some edits touch both. The order keeps a refused edit from changing anything:
//!
//! - Adding a secret with a value, marking a variable secret and renaming a secret's key set
//!   the new store entry first and write the file after; a store failure refuses the edit
//!   before the file is touched. When the file write then fails, the new entry is deleted
//!   again; if that fails too, [`VariableError::Orphaned`] names the entry left behind.
//! - Setting a secret's value writes the store only.
//! - Unmarking a secret, deleting one and renaming its key write the file first and delete
//!   the old entry after. A delete failure at that point does not undo the file: the edit
//!   stands and [`VariableError::SecretCleanup`] names the entry left behind.
//!
//! Errors and logs name keys and keyring accounts, never a value.

use reqflux_core::model::{Id, Variable};
use reqflux_store::{ScopeFile, VariableChange, VariableWrite};

use crate::engine::Engine;
use crate::error::{TreeError, VariableError};
use crate::secrets::{SecretKey, SecretScope};

/// One change to an existing variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableEdit {
    /// The key; a secret's store entry moves with it.
    Key(String),
    /// The shared value; for a secret, the value in the secret store (`None` deletes it).
    Value(Option<String>),
    /// The description.
    Description(Option<String>),
    /// Whether the variable resolves.
    Enabled(bool),
    /// Whether the value is kept in the secret store instead of the file.
    Secret(bool),
}

/// What an edit does to the secret store around the file write.
#[derive(Debug, Default)]
struct SecretMoves {
    /// Set before the write, so a store failure refuses the edit.
    set: Option<(SecretKey, String)>,
    /// Deleted after a successful write.
    delete: Option<SecretKey>,
}

impl Engine {
    /// Appends `variable` to the variables of `scope`. A secret's value goes to the secret
    /// store, never to the file.
    ///
    /// # Errors
    ///
    /// The store error (an empty or taken key, an unknown scope, a conflict), a secret
    /// store error that refused the edit, or [`VariableError::Reload`].
    pub fn add_variable(
        &mut self,
        scope: SecretScope,
        variable: Variable,
    ) -> Result<(), VariableError> {
        let mut moves = SecretMoves::default();
        if variable.secret
            && let Some(value) = &variable.value
        {
            moves.set = Some((self.secret_key(scope, &variable.key), value.clone()));
        }
        let plan = self.plan(scope, VariableChange::Add(variable))?;
        self.commit(plan, moves)
    }

    /// Declares `key` in `scope`: appends an enabled, non-secret variable with no value, so
    /// the key is in the file while a current value for it stays on this machine.
    ///
    /// # Errors
    ///
    /// As [`Engine::add_variable`].
    pub fn declare_variable(&mut self, scope: SecretScope, key: &str) -> Result<(), VariableError> {
        let mut variable = Variable::new(key, "");
        variable.value = None;
        self.add_variable(scope, variable)
    }

    /// Applies `edit` to the variable at `index` in `scope`.
    ///
    /// # Errors
    ///
    /// As [`Engine::add_variable`], plus [`VariableError::SecretCleanup`] when the file was
    /// written but an old secret store entry could not be deleted.
    pub fn edit_variable(
        &mut self,
        scope: SecretScope,
        index: usize,
        edit: VariableEdit,
    ) -> Result<(), VariableError> {
        let old = reqflux_store::scope_variables(&self.loaded, scope_file(scope))?
            .get(index)
            .cloned()
            .ok_or(reqflux_store::StoreError::NoVariable { index })?;
        let old_key = self.secret_key(scope, &old.key);
        let mut new = old.clone();
        let mut moves = SecretMoves::default();
        match edit {
            VariableEdit::Value(value) if old.secret => {
                match value {
                    Some(value) => self.secret_store().set(&old_key, &value)?,
                    None => self.secret_store().delete(&old_key)?,
                }
                return Ok(());
            }
            VariableEdit::Key(key) => {
                if old.secret && key != old.key {
                    if let Some(value) = self.secret_store().get(&old_key)? {
                        moves.set = Some((self.secret_key(scope, &key), value));
                    }
                    moves.delete = Some(old_key);
                }
                new.key = key;
            }
            VariableEdit::Value(value) => new.value = value,
            VariableEdit::Description(description) => new.description = description,
            VariableEdit::Enabled(enabled) => new.enabled = enabled,
            VariableEdit::Secret(true) => {
                if !old.secret
                    && let Some(value) = old.value
                {
                    moves.set = Some((old_key, value));
                }
                new.secret = true;
                new.value = None;
            }
            VariableEdit::Secret(false) => {
                if old.secret {
                    new.value = None;
                    moves.delete = Some(old_key);
                }
                new.secret = false;
            }
        }
        let plan = self.plan(scope, VariableChange::Replace(index, new))?;
        self.commit(plan, moves)
    }

    /// Deletes the variable at `index` in `scope`, and a secret's store entry after it.
    ///
    /// # Errors
    ///
    /// As [`Engine::edit_variable`].
    pub fn delete_variable(
        &mut self,
        scope: SecretScope,
        index: usize,
    ) -> Result<(), VariableError> {
        let variables = reqflux_store::scope_variables(&self.loaded, scope_file(scope))?;
        let mut moves = SecretMoves::default();
        if let Some(variable) = variables.get(index)
            && variable.secret
        {
            moves.delete = Some(self.secret_key(scope, &variable.key));
        }
        let plan = self.plan(scope, VariableChange::Remove(index))?;
        self.commit(plan, moves)
    }

    /// Creates an environment named `New environment` (numbered when taken) and returns its
    /// id.
    ///
    /// # Errors
    ///
    /// The store error, or [`TreeError::Reload`] when the workspace no longer loads.
    pub fn create_environment(&mut self) -> Result<Id, TreeError> {
        let placed = reqflux_store::create_environment(&self.root)?;
        self.reload().map_err(TreeError::Reload)?;
        Ok(placed.id)
    }

    /// Renames the environment `id`, moving its file to the new slug, and returns `id`.
    ///
    /// # Errors
    ///
    /// The store error (an empty or taken name, an unknown environment, a conflict), or
    /// [`TreeError::Reload`].
    pub fn rename_environment(&mut self, id: Id, name: &str) -> Result<Id, TreeError> {
        self.apply(|root, loaded| {
            reqflux_store::rename_environment(root, loaded, id, name).map(|placed| placed.id)
        })
    }

    /// Deletes the environment `id`. The secret store entries of its secrets stay.
    ///
    /// # Errors
    ///
    /// The store error (an unknown environment, a conflict), or [`TreeError::Reload`].
    pub fn delete_environment(&mut self, id: Id) -> Result<(), TreeError> {
        self.apply(|root, loaded| reqflux_store::delete_environment(root, loaded, id))
    }

    fn plan(
        &self,
        scope: SecretScope,
        change: VariableChange,
    ) -> Result<VariableWrite, VariableError> {
        Ok(reqflux_store::plan_variables(
            &self.root,
            &self.loaded,
            scope_file(scope),
            change,
        )?)
    }

    fn secret_key(&self, scope: SecretScope, key: &str) -> SecretKey {
        SecretKey {
            workspace_id: self.loaded.workspace.manifest.id,
            scope,
            key: key.to_owned(),
        }
    }

    /// Sets the new secret entry, writes the file, deletes the old entry and reloads.
    fn commit(&mut self, plan: VariableWrite, moves: SecretMoves) -> Result<(), VariableError> {
        let store = self.secret_store().clone();
        if let Some((key, value)) = &moves.set {
            store.set(key, value)?;
        }
        if let Err(error) = plan.write() {
            if let Some((key, _)) = &moves.set
                && let Err(rollback) = store.delete(key)
            {
                return Err(VariableError::Orphaned {
                    source: Box::new(error),
                    account: key.account(),
                    rollback,
                });
            }
            return Err(error.into());
        }
        let cleanup = match moves.delete {
            Some(key) => store
                .delete(&key)
                .map_err(|source| VariableError::SecretCleanup {
                    account: key.account(),
                    source,
                }),
            None => Ok(()),
        };
        self.reload().map_err(VariableError::Reload)?;
        cleanup
    }
}

fn scope_file(scope: SecretScope) -> ScopeFile {
    match scope {
        SecretScope::Global => ScopeFile::Globals,
        SecretScope::Collection(id) => ScopeFile::Collection(id),
        SecretScope::Environment(id) => ScopeFile::Environment(id),
    }
}
