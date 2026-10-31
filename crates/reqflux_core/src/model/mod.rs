//! The workspace domain model.
//!
//! Two layers: per-file documents ([`WorkspaceManifest`], [`Environment`], [`Collection`],
//! [`Folder`], [`Request`]) that serialize to TOML exactly as stored, and the in-memory tree
//! ([`Workspace`], [`CollectionNode`], [`FolderNode`], [`RequestNode`]) that the store
//! assembles from them together with slugs and sidecar [`Scripts`].

mod auth;
mod body;
mod documents;
mod extensions;
mod id;
mod mcp;
mod method;
mod tree;
mod variable;

pub use auth::{ApiKeyLocation, Auth};
pub use body::{Body, FormPart, RawLanguage, Settings};
pub use documents::{Collection, Environment, Folder, Request, WorkspaceManifest};
pub use id::Id;
pub use mcp::{McpOverride, Rule};
pub use method::{Method, MethodError};
pub use tree::{
    CollectionNode, EnvironmentNode, FolderNode, Item, RequestNode, Scripts, Workspace,
};
pub use variable::{KeyValue, Variable, strip_secret_values};

#[cfg(test)]
mod tests;
