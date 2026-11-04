//! What a front end shows about a request before it is sent, against the active environment.

use std::path::Path;

use reqflux_core::model::{Request, RequestNode, Variable, Workspace};
use reqflux_core::template::{DynamicVariable, Scopes, Segment, SequentialDynamicSource, segments};

use super::{
    ExecuteOptions, StoredSecrets, enabled, prepare_target, scopes, select_environment, url,
};
use crate::current::CurrentValues;
use crate::error::ExecuteError;
use crate::secrets::SecretStore;
use crate::select;

/// A request as a front end shows it before sending it, against the active environment. Its
/// `Debug` output leaves out the variable values, secrets included.
#[derive(Clone, PartialEq)]
pub struct Inspection {
    /// The selected request with its scripts, as stored.
    pub request: RequestNode,
    /// The request URL as written, with the enabled params appended as written after the
    /// separator [`prepare`] uses.
    pub url: String,
    /// The names [`prepare`](super::prepare) reports unresolved with the environment, in its order. Empty
    /// when the request cannot be prepared, for example with an auth type the engine does
    /// not send yet.
    pub unresolved: Vec<String>,
    /// Why [`prepare`](super::prepare) fails with the environment, as its error text. The spans still resolve
    /// with the environment when the collection allows it, and without one otherwise.
    pub problem: Option<String>,
    /// Why secret variables were not read from the secret store, as in
    /// [`Prepared::secret_problem`](super::Prepared::secret_problem).
    pub secret_problem: Option<String>,
    scopes: Scopes,
    secrets: Vec<String>,
}

impl std::fmt::Debug for Inspection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Inspection")
            .field("request", &self.request)
            .field("url", &self.url)
            .field("unresolved", &self.unresolved)
            .field("problem", &self.problem)
            .field("secret_problem", &self.secret_problem)
            .field("secrets", &self.secrets)
            .finish_non_exhaustive()
    }
}

/// A run of a text split by [`Inspection::spans`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    /// Text that is not a reference.
    Literal(String),
    /// A `{{name}}` reference.
    Reference {
        /// The whole `{{...}}` as written.
        raw: String,
        /// The trimmed name.
        name: String,
        /// Whether the name resolves: a variable of the globals, the collection or the
        /// environment, or a dynamic variable such as `$guid`.
        resolves: bool,
        /// Whether a variable with this name is secret in the globals, the collection or an
        /// environment the collection allows.
        secret: bool,
    },
}

impl Inspection {
    /// Splits `text` into literal runs and references; concatenating the runs yields `text`.
    #[must_use]
    pub fn spans(&self, text: &str) -> Vec<Span> {
        segments(text)
            .map(|segment| match segment {
                Segment::Literal(literal) => Span::Literal(literal.to_owned()),
                Segment::Reference { raw, name } => Span::Reference {
                    raw: raw.to_owned(),
                    name: name.to_owned(),
                    resolves: self.scopes.lookup(name).is_some()
                        || DynamicVariable::parse(name).is_some(),
                    secret: self.secrets.iter().any(|secret| secret == name),
                },
            })
            .collect()
    }

    /// Whether `text` references a secret variable.
    #[must_use]
    pub fn references_secret(&self, text: &str) -> bool {
        self.spans(text)
            .iter()
            .any(|span| matches!(span, Span::Reference { secret: true, .. }))
    }
}

/// Inspects the request `selector` of `workspace`, whose file paths resolve against `root`,
/// against the environment slug `environment`, without any IO.
///
/// When [`prepare`](super::prepare) fails with that environment, the inspection carries the error text in
/// [`Inspection::problem`]; its spans resolve with the environment when the collection allows
/// it and without one otherwise.
///
/// # Errors
///
/// Fails when no request matches `selector`.
pub fn inspect(
    workspace: &Workspace,
    root: &Path,
    selector: &str,
    environment: Option<&str>,
) -> Result<Inspection, ExecuteError> {
    inspect_stored(
        workspace,
        root,
        selector,
        environment,
        &CurrentValues::default(),
        None,
        None,
    )
}

/// [`inspect`] with the `current` values of the workspace, and with the valueless secrets
/// read from `store` when there is one, so a stored secret reports resolved (keyring-secrets
/// S4). Its value stays out of the inspection.
///
/// With a `draft`, the unresolved names, the problem and the URL follow the draft, as a send
/// of it would resolve them; [`Inspection::request`] stays the stored request.
#[allow(
    clippy::too_many_arguments,
    reason = "the one internal path every public entry point funnels into"
)]
pub(crate) fn inspect_stored(
    workspace: &Workspace,
    root: &Path,
    selector: &str,
    environment: Option<&str>,
    current: &CurrentValues,
    store: Option<&dyn SecretStore>,
    draft: Option<&Request>,
) -> Result<Inspection, ExecuteError> {
    let target = select::find(workspace, selector)?;
    let request = draft.unwrap_or(&target.request.request);
    let collection = &target.collection.collection;

    let params: Vec<(String, String)> = enabled(&request.params)
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect();
    let options = ExecuteOptions {
        environment: environment.map(str::to_owned),
        current: current.clone(),
        ..ExecuteOptions::default()
    };
    // The spans follow the environment whenever the collection allows it, also when
    // something else, such as the auth, keeps the request from being prepared.
    let selected = select_environment(workspace, target.collection, &options)
        .ok()
        .flatten();
    let stored = store.map_or_else(StoredSecrets::default, |store| {
        StoredSecrets::read(store, workspace, target.collection, selected)
    });
    let prepared = prepare_target(
        workspace,
        &target,
        request,
        root,
        &options,
        &mut SequentialDynamicSource::default(),
        &[],
        &stored,
    );
    let (unresolved, problem) = match prepared {
        Ok(prepared) => (prepared.unresolved, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };

    let environments = workspace
        .environments
        .iter()
        .filter(|environment| collection.allows_environment(environment.environment.id))
        .map(|environment| environment.environment.variables.as_slice());
    let secrets = [
        workspace.manifest.globals.as_slice(),
        collection.variables.as_slice(),
    ]
    .into_iter()
    .chain(environments)
    .flatten()
    .filter(|variable| variable.secret)
    .map(|variable: &Variable| variable.key.clone())
    .collect();

    Ok(Inspection {
        request: target.request.clone(),
        url: url::append(request.url.clone(), &params),
        unresolved,
        problem,
        scopes: scopes(workspace, target.collection, selected, &options, &stored),
        secret_problem: stored.problem,
        secrets,
    })
}
