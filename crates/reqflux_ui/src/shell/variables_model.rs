//! The workspace settings page without GPUI (variables-editor S6, design-catch-up T7): the
//! environments and collections it lists, the variables a scope declares, the file they live
//! in, the key a new variable gets, the variables left empty and the current values a scope
//! holds for keys it does not declare.

use reqflux_core::model::{Id, Variable, Workspace};
use reqflux_engine::{CurrentValues, SecretScope};

/// The key a new variable starts with, numbered when taken.
pub(crate) const NEW_KEY: &str = "new_variable";

/// An environment or a collection the page lists, with the number of variables it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeEntry {
    pub(crate) id: Id,
    pub(crate) name: String,
    /// The environment's file stem or the collection's directory.
    pub(crate) slug: String,
    pub(crate) count: usize,
}

/// The environments by name.
pub(crate) fn environments(workspace: &Workspace) -> Vec<ScopeEntry> {
    let mut environments: Vec<_> = workspace
        .environments
        .iter()
        .map(|node| ScopeEntry {
            id: node.environment.id,
            name: node.environment.name.clone(),
            slug: node.slug.clone(),
            count: node.environment.variables.len(),
        })
        .collect();
    environments.sort_by(|left, right| left.name.cmp(&right.name));
    environments
}

/// The collections in workspace order.
pub(crate) fn collections(workspace: &Workspace) -> Vec<ScopeEntry> {
    workspace
        .collections
        .iter()
        .map(|node| ScopeEntry {
            id: node.collection.id,
            name: node.collection.name.clone(),
            slug: node.slug.clone(),
            count: node.collection.variables.len(),
        })
        .collect()
}

/// The name of `scope`: `Globals`, or the collection's or the environment's name.
pub(crate) fn scope_name(workspace: &Workspace, scope: SecretScope) -> Option<String> {
    match scope {
        SecretScope::Global => Some("Globals".to_owned()),
        SecretScope::Collection(id) => collections(workspace)
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.name),
        SecretScope::Environment(id) => environments(workspace)
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.name),
    }
}

/// The file that declares the variables of `scope`, relative to the workspace.
pub(crate) fn scope_file(workspace: &Workspace, scope: SecretScope) -> Option<String> {
    match scope {
        SecretScope::Global => Some("reqflux.toml".to_owned()),
        SecretScope::Collection(id) => collections(workspace)
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| format!("{}/collection.toml", entry.slug)),
        SecretScope::Environment(id) => environments(workspace)
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| format!("{}.env.toml", entry.slug)),
    }
}

/// `1 variable` or `<n> variables`.
pub(crate) fn variable_count(count: usize) -> String {
    if count == 1 {
        "1 variable".to_owned()
    } else {
        format!("{count} variables")
    }
}

/// Whether `variable` resolves to nothing: enabled, not secret (a secret's value is in the
/// keyring, which the page never reads), with neither a shared nor a current value.
pub(crate) fn is_empty(variable: &Variable, current: Option<&str>) -> bool {
    variable.enabled
        && !variable.secret
        && variable.value.as_deref().unwrap_or_default().is_empty()
        && current.unwrap_or_default().is_empty()
}

/// How many variables of `scope` are [`is_empty`].
pub(crate) fn empty_count(
    current: &CurrentValues,
    scope: SecretScope,
    variables: &[Variable],
) -> usize {
    variables
        .iter()
        .filter(|variable| is_empty(variable, current.get(scope, &variable.key)))
        .count()
}

/// The keys `variables` declares that hold a current value in `scope`, which `Reset current
/// values` clears; a secret has none of its own.
pub(crate) fn current_keys(
    current: &CurrentValues,
    scope: SecretScope,
    variables: &[Variable],
) -> Vec<String> {
    variables
        .iter()
        .filter(|variable| !variable.secret && current.get(scope, &variable.key).is_some())
        .map(|variable| variable.key.clone())
        .collect()
}

/// The variables `scope` declares, in file order; `None` when the scope no longer exists.
pub(crate) fn scope_variables(workspace: &Workspace, scope: SecretScope) -> Option<&[Variable]> {
    match scope {
        SecretScope::Global => Some(&workspace.manifest.globals),
        SecretScope::Collection(id) => workspace
            .collections
            .iter()
            .find(|node| node.collection.id == id)
            .map(|node| node.collection.variables.as_slice()),
        SecretScope::Environment(id) => workspace
            .environments
            .iter()
            .find(|node| node.environment.id == id)
            .map(|node| node.environment.variables.as_slice()),
    }
}

/// [`NEW_KEY`], or `new_variable_<n>` with the first `n` from 2 that `variables` does not
/// use.
pub(crate) fn new_key(variables: &[Variable]) -> String {
    let taken = |key: &str| variables.iter().any(|variable| variable.key == key);
    if !taken(NEW_KEY) {
        return NEW_KEY.to_owned();
    }
    (2..)
        .map(|n| format!("{NEW_KEY}_{n}"))
        .find(|key| !taken(key))
        .unwrap_or_default()
}

/// The current values of `scope` whose key `variables` does not declare, as `(key, value)`
/// sorted by key: values a script set for a variable no file declares.
pub(crate) fn undeclared(
    current: &CurrentValues,
    scope: SecretScope,
    variables: &[Variable],
) -> Vec<(String, String)> {
    let mut found: Vec<_> = current
        .iter()
        .filter(|(at, key, _)| {
            *at == scope && !variables.iter().any(|variable| variable.key == *key)
        })
        .map(|(_, key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    found.sort();
    found
}

/// The cell of a Variables document row that a field edits. Each is a distinct table name
/// in the shell's cell field slot, and `<table>-<n>` is the cell's element id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Key,
    Value,
    Current,
    Description,
    /// The current value of a key the scope does not declare.
    Undeclared,
}

impl Column {
    const ALL: [Self; 5] = [
        Self::Key,
        Self::Value,
        Self::Current,
        Self::Description,
        Self::Undeclared,
    ];

    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Key => "variables-key",
            Self::Value => "variables-value",
            Self::Current => "variables-current",
            Self::Description => "variables-description",
            Self::Undeclared => "variables-undeclared-current",
        }
    }

    pub(crate) fn from_table(table: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.table() == table)
    }

    /// The element id of the cell in the 1-based row `n`.
    pub(crate) fn element_id(self, n: usize) -> String {
        format!("{}-{n}", self.table())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_key_is_numbered_when_taken() {
        assert_eq!(new_key(&[]), "new_variable");
        let taken = [
            Variable::new("new_variable", ""),
            Variable::new("new_variable_2", ""),
        ];
        assert_eq!(new_key(&taken), "new_variable_3");
    }

    #[test]
    fn undeclared_values_skip_declared_keys_and_other_scopes() {
        let mut current = CurrentValues::default();
        current.insert(SecretScope::Global, "token", "a");
        current.insert(SecretScope::Global, "base", "b");
        let declared = [Variable::new("base", "")];
        assert_eq!(
            undeclared(&current, SecretScope::Global, &declared),
            vec![("token".to_owned(), "a".to_owned())]
        );
    }

    #[test]
    fn empty_counts_enabled_plain_variables_with_no_value() {
        let mut current = CurrentValues::default();
        current.insert(SecretScope::Global, "filled", "x");
        let mut disabled = Variable::new("disabled", "");
        disabled.enabled = false;
        let mut secret = Variable::new("secret", "");
        secret.secret = true;
        let mut none = Variable::new("none", "");
        none.value = None;
        let variables = [
            none,
            Variable::new("blank", ""),
            Variable::new("filled", ""),
            Variable::new("set", "v"),
            disabled,
            secret,
        ];
        assert_eq!(empty_count(&current, SecretScope::Global, &variables), 2);
        assert_eq!(
            current_keys(&current, SecretScope::Global, &variables),
            vec!["filled".to_owned()]
        );
        assert_eq!(variable_count(1), "1 variable");
        assert_eq!(variable_count(0), "0 variables");
    }

    #[test]
    fn columns_round_trip_through_their_table() {
        for column in Column::ALL {
            assert_eq!(Column::from_table(column.table()), Some(column));
        }
        assert_eq!(Column::from_table("params"), None);
    }
}
