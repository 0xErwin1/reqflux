//! The bundle file (v1-features S6): one JSON document holding workspace files as the store
//! writes them, so a bundle reads like the workspace it came from.
//!
//! ```json
//! {
//!   "format": "reqflux-bundle",
//!   "format_version": 1,
//!   "reqflux_version": "0.1.0",
//!   "kind": "workspace | collection | folder | request | environment",
//!   "name": "<display name>",
//!   "manifest": "<reqflux.toml>",                       // workspace only
//!   "environments": [{ "slug": "...", "file": "<.env.toml>" }],
//!   "collections": [{ "slug", "file": "<collection.toml>", "pre"?, "test"?, "items": [...] }],
//!   "items": [{ "folder": { "slug", "file", "pre"?, "test"?, "items": [...] } }
//!            | { "request": { "slug", "file", "pre"?, "test"?,
//!                             "examples": [{ "file": "<.example.toml>", "body_base64"? }] } }],
//!   "secrets": { "cipher": "age-scrypt", "armor": "-----BEGIN AGE ENCRYPTED FILE-----..." }
//! }
//! ```
//!
//! `pre` and `test` are the sidecar scripts. `body_base64` is an example body kept in a file
//! beside it. The secrets, when present, are the age encryption (scrypt passphrase
//! recipient, ASCII armor) of `{"secrets": [{"scope", "key", "value"}]}`, `scope` being
//! `global`, `collection/<id>` or `environment/<id>` with the ids of the files in the
//! bundle. A secret value is never anywhere else in the file.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The `format` of every bundle.
pub(crate) const FORMAT: &str = "reqflux-bundle";

/// The newest `format_version` this build writes and reads.
pub(crate) const FORMAT_VERSION: u64 = 1;

/// The cipher of an encrypted secrets section.
pub(crate) const CIPHER: &str = "age-scrypt";

/// What a bundle holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleKind {
    /// A whole workspace: its manifest, environments and collections.
    Workspace,
    /// A collection with everything under it.
    Collection,
    /// A folder with everything under it.
    Folder,
    /// One request with its scripts and examples.
    Request,
    /// One environment.
    Environment,
}

impl BundleKind {
    /// The kind as the file spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Collection => "collection",
            Self::Folder => "folder",
            Self::Request => "request",
            Self::Environment => "environment",
        }
    }
}

/// The whole bundle file.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Document {
    pub(crate) format: String,
    pub(crate) format_version: u64,
    #[serde(default)]
    pub(crate) reqflux_version: String,
    pub(crate) kind: BundleKind,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) manifest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) environments: Vec<EnvironmentEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) collections: Vec<CollectionEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) items: Vec<ItemEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) secrets: Option<SecretsEntry>,
}

/// Only the header, read first so a newer format is refused before the rest is parsed.
#[derive(Debug, Deserialize)]
pub(crate) struct Header {
    pub(crate) format: String,
    pub(crate) format_version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EnvironmentEntry {
    pub(crate) slug: String,
    pub(crate) file: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CollectionEntry {
    pub(crate) slug: String,
    pub(crate) file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) test: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) items: Vec<ItemEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ItemEntry {
    Folder(CollectionEntry),
    Request(RequestEntry),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RequestEntry {
    pub(crate) slug: String,
    pub(crate) file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) test: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) examples: Vec<ExampleEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExampleEntry {
    pub(crate) file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body_base64: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SecretsEntry {
    pub(crate) cipher: String,
    pub(crate) armor: String,
}

/// The plaintext of the secrets section, only ever held in memory.
#[derive(Serialize, Deserialize)]
pub(crate) struct SecretList {
    pub(crate) secrets: Vec<SecretValue>,
}

/// One secret value. Its `Debug` leaves the value out.
#[derive(Serialize, Deserialize)]
pub(crate) struct SecretValue {
    pub(crate) scope: String,
    pub(crate) key: String,
    pub(crate) value: String,
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretValue")
            .field("scope", &self.scope)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for SecretList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(&self.secrets).finish()
    }
}
