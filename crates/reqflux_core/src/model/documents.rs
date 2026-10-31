//! Per-file documents: one struct per workspace file, serialized to TOML as is.
//!
//! Field order is the on-disk key order (S5). Defaults are omitted on write: empty lists,
//! `auth = inherit`, `body = none`, unset settings and options. Unknown keys live in
//! `extensions` (S9).

use serde::{Deserialize, Serialize};

use super::auth::Auth;
use super::body::{Body, Settings};
use super::extensions;
use super::id::Id;
use super::mcp::McpOverride;
use super::method::Method;
use super::variable::{KeyValue, Variable, strip_secret_values};

/// `reqflux.toml`: the workspace manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    /// The workspace id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// The on-disk format version. The store checks it.
    #[serde(default)]
    pub format: u32,
    /// Workspace globals, the lowest variable scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub globals: Vec<Variable>,
    /// `[mcp]`: the MCP restrictions of this workspace, which can only narrow the user's
    /// policy (user-config-mcp-policy S4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpOverride>,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

/// `environments/<slug>.env.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    /// The environment id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// The environment variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

/// `collections/<slug>/collection.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    /// The collection id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// Workspace environments that apply to this collection. Empty means all of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<Id>,
    /// Child slugs in display order. Unlisted children append sorted by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// The auth inherited by folders and requests.
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// Collection variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

/// `<folder-slug>/folder.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    /// The folder id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// Child slugs in display order. Unlisted children append sorted by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// The auth inherited by children.
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

/// `<request-slug>.req.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The request id.
    pub id: Id,
    /// The display name.
    pub name: String,
    /// The HTTP method.
    pub method: Method,
    /// The URL without its query string; the query lives in `params`.
    #[serde(default)]
    pub url: String,
    /// Query parameters, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    /// Headers, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// The request auth.
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// The request body.
    #[serde(default, skip_serializing_if = "Body::is_none")]
    pub body: Body,
    /// Per-request settings.
    #[serde(default, skip_serializing_if = "Settings::is_default")]
    pub settings: Settings,
    /// Keys this version of ReqFlux does not know.
    #[serde(flatten, with = "extensions")]
    pub extensions: toml::Table,
}

impl Request {
    /// Builds a request with no params, headers, body or settings that inherits auth.
    #[must_use]
    pub fn new(id: Id, name: impl Into<String>, method: Method, url: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            method,
            url: url.into(),
            params: Vec::new(),
            headers: Vec::new(),
            auth: Auth::Inherit,
            body: Body::None,
            settings: Settings::default(),
            extensions: toml::Table::new(),
        }
    }
}

impl WorkspaceManifest {
    /// Drops the values of secret globals and returns the keys that had one (S10).
    pub fn strip_secret_values(&mut self) -> Vec<String> {
        strip_secret_values(&mut self.globals)
    }
}

impl Environment {
    /// Drops the values of secret variables and returns the keys that had one (S10).
    pub fn strip_secret_values(&mut self) -> Vec<String> {
        strip_secret_values(&mut self.variables)
    }
}

impl Collection {
    /// Drops the values of secret variables and returns the keys that had one (S10).
    pub fn strip_secret_values(&mut self) -> Vec<String> {
        strip_secret_values(&mut self.variables)
    }

    /// Whether the workspace environment `id` applies to this collection (S2).
    #[must_use]
    pub fn allows_environment(&self, id: Id) -> bool {
        self.environments.is_empty() || self.environments.contains(&id)
    }
}
