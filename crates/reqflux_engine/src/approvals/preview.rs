//! What the window shows of a call it is asked to approve (mcp-approvals S3), read from its
//! own copy of the workspace: no argument value crosses the approval socket, so the window
//! prepares the request the call names with the environment it names, the way
//! `request_execute` would, without running its scripts and without sending anything.
//!
//! Every value in a [`CallPreview`] is masked with the masker of that preparation: the URL,
//! the headers (a credential header whole) and the body. A secret variable's value is never
//! read into it at all; it shows as [`MASK`].

use reqflux_core::model::{
    Auth, CollectionNode, EnvironmentNode, Item, KeyValue, Variable, Workspace,
};
use reqflux_core::template::{DynamicVariable, Segment, segments};

use crate::engine::Engine;
use crate::error::ExecuteError;
use crate::prepare::{ExecuteOptions, Inspection};
use crate::select::{self, Target};
use crate::sent::{MASK, Masker, SentBody};

/// Where a variable a request uses takes its value from, in the precedence the engine
/// resolves with: the environment, then the collection, then the globals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableSource {
    /// The environment with this name.
    Environment(String),
    /// The collection with this name.
    Collection(String),
    /// The workspace globals.
    Globals,
    /// The secret store: a secret variable, whose value never lives in a file.
    Keyring,
    /// A dynamic variable such as `$guid`, generated when the request is sent.
    Dynamic,
    /// No scope defines it: the reference is sent as written.
    Unresolved,
}

impl VariableSource {
    /// The source as the window names it: the environment or collection name, `globals`,
    /// `keyring`, `dynamic` or `unresolved`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Environment(name) | Self::Collection(name) => name.clone(),
            Self::Globals => "globals".to_owned(),
            Self::Keyring => "keyring".to_owned(),
            Self::Dynamic => "dynamic".to_owned(),
            Self::Unresolved => "unresolved".to_owned(),
        }
    }
}

/// One variable a request references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedVariable {
    /// The variable name, without braces.
    pub name: String,
    /// Its value, masked; [`MASK`] for a secret, empty when it does not resolve.
    pub value: String,
    /// Whether it is a secret variable.
    pub secret: bool,
    /// Where its value comes from.
    pub source: VariableSource,
}

/// A request as a call would send it, every secret masked.
#[derive(Debug, Clone, PartialEq)]
pub struct CallPreview {
    /// The collection, folder and request names, outermost first.
    pub breadcrumb: Vec<String>,
    /// The HTTP method.
    pub method: String,
    /// The request as written: its URL template in [`Inspection::url`] and the spans of its
    /// references in [`Inspection::spans`].
    pub inspection: Inspection,
    /// The resolved URL, masked.
    pub url: String,
    /// The name of the environment the URL was resolved in; `None` without one.
    pub environment: Option<String>,
    /// The headers in send order, masked.
    pub headers: Vec<(String, String)>,
    /// The body, masked.
    pub body: SentBody,
    /// The variables the request references, in first-use order.
    pub variables: Vec<UsedVariable>,
}

/// The environment of `workspace` whose slug, name or id is `given`, in that order.
#[must_use]
pub fn find_environment<'a>(workspace: &'a Workspace, given: &str) -> Option<&'a EnvironmentNode> {
    let environments = &workspace.environments;
    environments
        .iter()
        .find(|node| node.slug == given)
        .or_else(|| {
            environments
                .iter()
                .find(|node| node.environment.name == given)
        })
        .or_else(|| {
            environments
                .iter()
                .find(|node| node.environment.id.to_string() == given)
        })
}

/// The names from the collection down to the collection, folder or request `id`, or the
/// name of the environment `id`; `None` when nothing in `workspace` has that id.
#[must_use]
pub fn breadcrumb(workspace: &Workspace, id: &str) -> Option<Vec<String>> {
    if let Some(environment) = workspace
        .environments
        .iter()
        .find(|node| node.environment.id.to_string() == id)
    {
        return Some(vec![environment.environment.name.clone()]);
    }
    workspace.collections.iter().find_map(|collection| {
        let mut path = vec![collection.collection.name.clone()];
        if collection.collection.id.to_string() == id || item_path(&collection.items, id, &mut path)
        {
            Some(path)
        } else {
            None
        }
    })
}

/// Pushes onto `path` the names down to the folder or request `id` in `items`.
fn item_path(items: &[Item], id: &str, path: &mut Vec<String>) -> bool {
    for item in items {
        match item {
            Item::Request(request) if request.request.id.to_string() == id => {
                path.push(request.request.name.clone());
                return true;
            }
            Item::Request(_) => {}
            Item::Folder(folder) => {
                path.push(folder.folder.name.clone());
                if folder.folder.id.to_string() == id || item_path(&folder.items, id, path) {
                    return true;
                }
                path.pop();
            }
        }
    }
    false
}

/// Prepares the request `request_id` of `engine` with the environment `environment` (a
/// slug, name or id), its scripts not run and nothing sent, and returns it masked.
///
/// # Errors
///
/// No request has the id, the environment is unknown or not allowed for its collection, or
/// the request does not prepare.
pub fn preview(
    engine: &Engine,
    request_id: &str,
    environment: Option<&str>,
) -> Result<CallPreview, ExecuteError> {
    let workspace = engine.workspace();
    let target = select::find(workspace, request_id)?;
    let selected = environment.map(|given| find_environment(workspace, given));
    let slug = match (environment, selected) {
        (Some(_), Some(Some(node))) => Some(node.slug.clone()),
        (Some(given), _) => Some(given.to_owned()),
        (None, _) => None,
    };
    let options = ExecuteOptions {
        environment: slug.clone(),
        ..ExecuteOptions::default()
    };
    let prepared = engine.prepare(request_id, &options)?;
    let inspection = engine.inspect(request_id, slug.as_deref())?;
    let masker = prepared.masker();
    let sent = prepared.sent();
    let environment = selected.flatten();
    let mut breadcrumb = vec![target.collection.collection.name.clone()];
    breadcrumb.extend(
        target
            .folders
            .iter()
            .map(|folder| folder.folder.name.clone()),
    );
    breadcrumb.push(target.request.request.name.clone());
    Ok(CallPreview {
        breadcrumb,
        method: sent.method,
        variables: used_variables(workspace, &target, environment, &masker),
        inspection,
        url: sent.url,
        environment: environment.map(|node| node.environment.name.clone()),
        headers: sent.headers,
        body: sent.body,
    })
}

/// The variables the request of `target` references in its URL, enabled params and headers,
/// body and effective auth, each with its masked value and source.
fn used_variables(
    workspace: &Workspace,
    target: &Target<'_>,
    environment: Option<&EnvironmentNode>,
    masker: &Masker,
) -> Vec<UsedVariable> {
    let request = &target.request.request;
    let rows = |rows: &[KeyValue]| -> Vec<String> {
        rows.iter()
            .filter(|row| row.enabled)
            .flat_map(|row| [row.key.clone(), row.value.clone()])
            .collect()
    };
    let mut texts = vec![request.url.clone()];
    texts.extend(rows(&request.params));
    texts.extend(rows(&request.headers));
    texts.extend(serde_json::to_string(&request.body).ok());
    texts.extend(serde_json::to_string(effective_auth(target)).ok());
    let mut names: Vec<String> = Vec::new();
    for text in &texts {
        for segment in segments(text) {
            if let Segment::Reference { name, .. } = segment
                && !names.iter().any(|known| known == name)
            {
                names.push(name.to_owned());
            }
        }
    }
    names
        .into_iter()
        .map(|name| used_variable(workspace, target.collection, environment, masker, name))
        .collect()
}

/// The auth of the request, or of the nearest folder, then the collection, that does not
/// inherit; the same walk the engine's preparation makes.
fn effective_auth<'a>(target: &Target<'a>) -> &'a Auth {
    std::iter::once(&target.request.request.auth)
        .chain(
            target
                .folders
                .iter()
                .rev()
                .map(|folder| &folder.folder.auth),
        )
        .chain(std::iter::once(&target.collection.collection.auth))
        .find(|auth| !auth.is_inherit())
        .unwrap_or(&Auth::None)
}

/// `name` with its value and source: the first enabled definition in the environment, the
/// collection, then the globals.
fn used_variable(
    workspace: &Workspace,
    collection: &CollectionNode,
    environment: Option<&EnvironmentNode>,
    masker: &Masker,
    name: String,
) -> UsedVariable {
    let find = |variables: &[Variable]| -> Option<Variable> {
        variables
            .iter()
            .find(|variable| variable.enabled && variable.key == name)
            .cloned()
    };
    let found = environment
        .and_then(|node| {
            find(&node.environment.variables).map(|variable| {
                (
                    variable,
                    VariableSource::Environment(node.environment.name.clone()),
                )
            })
        })
        .or_else(|| {
            find(&collection.collection.variables).map(|variable| {
                (
                    variable,
                    VariableSource::Collection(collection.collection.name.clone()),
                )
            })
        })
        .or_else(|| {
            find(&workspace.manifest.globals).map(|variable| (variable, VariableSource::Globals))
        });
    match found {
        Some((variable, source)) if variable.secret => UsedVariable {
            name,
            value: MASK.to_owned(),
            secret: true,
            source: if variable.value.is_none() {
                VariableSource::Keyring
            } else {
                source
            },
        },
        Some((variable, source)) => UsedVariable {
            name,
            value: masker.mask(&variable.value.unwrap_or_default()),
            secret: false,
            source,
        },
        None if DynamicVariable::parse(&name).is_some() => UsedVariable {
            name,
            value: String::new(),
            secret: false,
            source: VariableSource::Dynamic,
        },
        None => UsedVariable {
            name,
            value: String::new(),
            secret: false,
            source: VariableSource::Unresolved,
        },
    }
}
