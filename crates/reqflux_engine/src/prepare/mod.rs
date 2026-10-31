//! Turning a workspace request into a [`PreparedRequest`] (S4-S9).

mod auth;
mod body;
mod inspect;
mod resolver;
mod stored;
mod url;

pub(crate) use stored::workspace_secrets;

use std::path::Path;
use std::time::Duration;

use reqflux_core::model::{
    CollectionNode, EnvironmentNode, KeyValue, Request, Settings, Variable, Workspace,
};
use reqflux_core::template::{DynamicSource, Scope, ScopeEntry, ScopeKind, Scopes};
use reqflux_http::{PreparedRequest, RequestOptions};
use reqflux_script::VariableWrite;

use crate::current::CurrentValues;
use crate::error::ExecuteError;
use crate::guard::HostGuard;
use crate::secrets::SecretScope;
use crate::select::{self, Target};
use crate::sent::{self, Masker, Secrets, SentRequest};
use resolver::Resolver;

pub(crate) use inspect::inspect_stored;
pub use inspect::{Inspection, Span, inspect};
pub(crate) use stored::StoredSecrets;

/// The timeout used when a request does not set one, in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// What the caller adds to a request execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecuteOptions {
    /// The slug of the workspace environment to use, if any.
    pub environment: Option<String>,
    /// Local variables, the highest-precedence scope, in order; a later key wins.
    pub vars: Vec<(String, String)>,
    /// The per-user current values of the workspace (variables-editor S4): each overrides
    /// the shared value of its variable within its scope, below local variables. Empty by
    /// default, which is what `reqflux run` passes.
    pub current: CurrentValues,
    /// The check every request of the execution passes before it is sent: the main request
    /// and each `pm.sendRequest` of its scripts (user-config-mcp-policy S3). `None`, the
    /// default and what the window and `reqflux run` pass, sends anywhere.
    pub host_guard: Option<HostGuard>,
}

/// A request ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The resolved request.
    pub request: PreparedRequest,
    /// Variable names that did not resolve, unique, in first-occurrence order. Their
    /// references are sent as literal text.
    pub unresolved: Vec<String>,
    /// Why secret variables were not read from the secret store, such as an OS keyring that
    /// is not available. Those secrets stay unresolved; the request still prepares.
    pub secret_problem: Option<String>,
    /// The values history must mask; see [`Prepared::sent`].
    pub(crate) secrets: Secrets,
}

impl Prepared {
    /// The request as it will be sent, with every secret replaced by
    /// [`MASK`](crate::MASK): every value of every secret variable in scope, and the basic
    /// password, bearer token or API key value, wherever they appear in the URL, the
    /// headers and the body.
    #[must_use]
    pub fn sent(&self) -> SentRequest {
        sent::redact(&self.request, &self.masker())
    }

    /// What masks this request's secrets in any other text, such as the error of a failed
    /// send, the same way [`Prepared::sent`] masks them.
    #[must_use]
    pub fn masker(&self) -> Masker {
        self.secrets.masker()
    }
}

/// Percent-encodes `text` the way the query string carries it.
pub(crate) fn percent_encode(text: &str) -> String {
    url::encode(text)
}

/// Prepares the request `selector` of `workspace` without any IO. Secret variables with no
/// value in the files stay unresolved; [`Engine::prepare`](crate::Engine::prepare) also reads
/// them from its secret store.
///
/// Variables resolve in the URL, then enabled params, enabled headers, the effective auth and
/// the body, which is the order unresolved names are reported in. Disabled rows are skipped
/// without resolving them.
///
/// `root` is the workspace root that relative file paths of bodies resolve against, and
/// `dynamic` produces the values of dynamic variables such as `{{$guid}}`.
///
/// # Errors
///
/// Fails when no request matches, when the environment is unknown or not allowed for the
/// request's collection, when the effective auth is not supported, or when GraphQL variables
/// are not valid JSON.
pub fn prepare(
    workspace: &Workspace,
    root: &Path,
    selector: &str,
    options: &ExecuteOptions,
    dynamic: &mut impl DynamicSource,
) -> Result<Prepared, ExecuteError> {
    let target = select::find(workspace, selector)?;
    prepare_target(
        workspace,
        &target,
        &target.request.request,
        root,
        options,
        dynamic,
        &[],
        &StoredSecrets::default(),
    )
}

/// Prepares `draft` in the place of the request `selector` selects, without any IO
/// (request-editing S5).
///
/// The draft replaces only the request document. Its parents are the ones the workspace has
/// for the selected request, so the collection variables, the inherited folder and
/// collection auth, the environment allowlist and the settings defaults behave exactly as
/// for the stored request. Resolution works as in [`prepare`].
///
/// # Errors
///
/// The errors of [`prepare`] for the same `workspace`, `selector`, `options` and `draft`.
pub fn prepare_draft(
    workspace: &Workspace,
    root: &Path,
    selector: &str,
    draft: &Request,
    options: &ExecuteOptions,
    dynamic: &mut impl DynamicSource,
) -> Result<Prepared, ExecuteError> {
    let target = select::find(workspace, selector)?;
    prepare_target(
        workspace,
        &target,
        draft,
        root,
        options,
        dynamic,
        &[],
        &StoredSecrets::default(),
    )
}

/// Prepares the request `target` selects with `request` as its document, applying the
/// variable writes a pre-request script made to this run's scopes (`S7`). The writes are
/// never persisted; they exist for this preparation only. Valueless secrets resolve from
/// `stored`.
///
/// The one resolution path; `request` is the document to prepare, stored or draft, and
/// `target` supplies the parents the scopes, allowlist and inherited auth come from.
///
/// # Errors
///
/// The errors of [`prepare`] for the same `workspace`, `target`, `options` and `request`.
#[allow(
    clippy::too_many_arguments,
    reason = "the one internal path every public entry point funnels into"
)]
pub(crate) fn prepare_target(
    workspace: &Workspace,
    target: &Target<'_>,
    request: &Request,
    root: &Path,
    options: &ExecuteOptions,
    dynamic: &mut impl DynamicSource,
    script_writes: &[VariableWrite],
    stored: &StoredSecrets,
) -> Result<Prepared, ExecuteError> {
    let environment = select_environment(workspace, target.collection, options)?;
    let names = secret_names(workspace, target.collection, environment);
    let mut scope_map = scopes(workspace, target.collection, environment, options, stored);
    crate::scripts::apply_writes(&mut scope_map, script_writes);
    let mut resolver = Resolver::new(scope_map, dynamic);

    let base = resolver.text(&request.url);
    let mut query = resolver.pairs(&request.params);
    let mut headers = resolver.pairs(&request.headers);
    let auth = auth::effective(request, target);
    let mut secrets = Secrets::default();
    auth::apply(auth, &mut resolver, &mut headers, &mut query, &mut secrets)?;
    let body = body::prepare(&request.body, root, &headers, &mut resolver)?;
    resolver.add_secret_values(&names, &mut secrets);

    Ok(Prepared {
        request: PreparedRequest {
            method: request.method.as_str().to_owned(),
            url: url::with_query(base, &query),
            headers,
            body,
            options: request_options(request.settings),
        },
        unresolved: resolver.into_unresolved(),
        secret_problem: stored.problem.clone(),
        secrets,
    })
}

/// The names of the secret variables in the scopes a request resolves with: the globals,
/// its collection and the selected environment.
fn secret_names<'a>(
    workspace: &'a Workspace,
    collection: &'a CollectionNode,
    environment: Option<&'a EnvironmentNode>,
) -> Vec<&'a str> {
    [
        workspace.manifest.globals.as_slice(),
        collection.collection.variables.as_slice(),
        environment.map_or(&[][..], |environment| {
            environment.environment.variables.as_slice()
        }),
    ]
    .into_iter()
    .flatten()
    .filter(|variable| variable.secret)
    .map(|variable| variable.key.as_str())
    .collect()
}

/// The environment `options` selects, checked against the collection allowlist (S5).
pub(crate) fn select_environment<'a>(
    workspace: &'a Workspace,
    collection: &CollectionNode,
    options: &ExecuteOptions,
) -> Result<Option<&'a EnvironmentNode>, ExecuteError> {
    let Some(slug) = options.environment.as_deref() else {
        return Ok(None);
    };
    let environment = workspace
        .environments
        .iter()
        .find(|environment| environment.slug == slug)
        .ok_or_else(|| ExecuteError::UnknownEnvironment(slug.to_owned()))?;
    if !collection
        .collection
        .allows_environment(environment.environment.id)
    {
        return Err(ExecuteError::EnvironmentNotAllowed {
            environment: slug.to_owned(),
            collection: collection.collection.name.clone(),
        });
    }
    Ok(Some(environment))
}

/// Globals < collection < environment < local `--var`s (S4). Within each scope a current
/// value from `options` overrides the shared value (variables-editor S4), and a current value
/// of a key the scope does not declare resolves too. A secret with no value in the files
/// takes its value from `stored`, in its own scope, so every override still wins; a secret
/// never takes a current value.
pub(crate) fn scopes(
    workspace: &Workspace,
    collection: &CollectionNode,
    environment: Option<&EnvironmentNode>,
    options: &ExecuteOptions,
    stored: &StoredSecrets,
) -> Scopes {
    let mut scopes = Scopes::default();
    let current = &options.current;
    *scopes.scope_mut(ScopeKind::Globals) = scope(
        &workspace.manifest.globals,
        stored,
        current,
        SecretScope::Global,
    );
    *scopes.scope_mut(ScopeKind::Collection) = scope(
        &collection.collection.variables,
        stored,
        current,
        SecretScope::Collection(collection.collection.id),
    );
    if let Some(environment) = environment {
        *scopes.scope_mut(ScopeKind::Environment) = scope(
            &environment.environment.variables,
            stored,
            current,
            SecretScope::Environment(environment.environment.id),
        );
    }
    *scopes.scope_mut(ScopeKind::Local) = options
        .vars
        .iter()
        .map(|(key, value)| ScopeEntry::new(key.clone(), value.clone(), true))
        .collect();
    scopes
}

/// A secret with no value takes its stored value, and is left out when none is stored so it
/// does not resolve; any other variable takes its current value, else its shared value, else
/// empty text. Current values of undeclared keys follow, enabled.
fn scope(
    variables: &[Variable],
    stored: &StoredSecrets,
    current: &CurrentValues,
    secret_scope: SecretScope,
) -> Scope {
    let declared =
        variables
            .iter()
            .filter_map(|variable| match (&variable.value, variable.secret) {
                (None, true) => stored
                    .value(secret_scope, &variable.key)
                    .map(|value| ScopeEntry::new(variable.key.clone(), value, variable.enabled)),
                (value, true) => Some(ScopeEntry::new(
                    variable.key.clone(),
                    value.clone().unwrap_or_default(),
                    variable.enabled,
                )),
                (value, false) => {
                    let value = current
                        .get(secret_scope, &variable.key)
                        .map(str::to_owned)
                        .or_else(|| value.clone())
                        .unwrap_or_default();
                    Some(ScopeEntry::new(
                        variable.key.clone(),
                        value,
                        variable.enabled,
                    ))
                }
            });
    let undeclared = current
        .undeclared(secret_scope, variables)
        .map(|(key, value)| ScopeEntry::new(key, value, true));
    declared.chain(undeclared).collect()
}

/// The transport settings of one request: the request's own, with the workspace defaults
/// filled in. Shared so a script's `pm.sendRequest` respects the same settings (S13).
pub(crate) fn request_options(settings: Settings) -> RequestOptions {
    RequestOptions {
        timeout: Duration::from_millis(settings.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)),
        follow_redirects: settings.follow_redirects.unwrap_or(true),
        verify_tls: settings.verify_tls.unwrap_or(true),
    }
}

/// The enabled rows of a key/value table.
pub(super) fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter().filter(|row| row.enabled)
}

/// Whether `headers` has one named `name`, compared case-insensitively.
fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case(name))
}
