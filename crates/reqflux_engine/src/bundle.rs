//! The native bundle (v1-features S6): a workspace, a collection, a folder, a request or an
//! environment in one versioned JSON file (see [`format`] for the layout), and its import.
//!
//! The entity files go in as the store writes them, scripts and saved examples included, so
//! a bundle reads like the workspace. Secret variable values never live in workspace files;
//! a bundle carries them only age-encrypted with a passphrase ([`BundleSecrets::Encrypt`]),
//! read from the secret store when the bundle is made and written to the target's store
//! when it is imported with the same passphrase. Otherwise they are left out and named in
//! [`Bundle::secrets_omitted`].
//!
//! [`import_bundle`] follows the Postman import's rule: every target file is checked before
//! anything is written, and one that exists fails the import with
//! [`BundleError::Exists`] and leaves the workspace untouched.

mod crypt;
mod format;
mod import;

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqflux_core::model::{CollectionNode, FolderNode, Item, RequestNode, Scripts, Variable};
use reqflux_store::StoreError;
use serde::Serialize;

use crate::engine::Engine;
use crate::error::BundleError;
use crate::export::unit::{self, Unit};
use crate::secrets::{SecretKey, SecretScope};
use format::{
    CIPHER, CollectionEntry, Document, EnvironmentEntry, ExampleEntry, FORMAT, FORMAT_VERSION,
    ItemEntry, RequestEntry, SecretList, SecretValue, SecretsEntry,
};

pub use format::BundleKind;
pub use import::{BundleImport, BundleSummary, import_bundle};

/// What a bundle does with secret values.
#[derive(Clone, Copy)]
pub enum BundleSecrets<'a> {
    /// Leave them out.
    Omit,
    /// Read them from the secret store and encrypt them with this passphrase.
    Encrypt {
        /// The passphrase; it is never stored or shown.
        passphrase: &'a str,
    },
}

impl std::fmt::Debug for BundleSecrets<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Omit => formatter.write_str("Omit"),
            Self::Encrypt { .. } => formatter.write_str("Encrypt { .. }"),
        }
    }
}

/// A bundle made from a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// What it holds.
    pub kind: BundleKind,
    /// The name of the workspace, collection, folder, request or environment.
    pub name: String,
    /// The bundle file.
    pub text: String,
    /// How many secret values it carries, encrypted.
    pub secrets_included: usize,
    /// The secret variables whose value it does not carry, as (scope, key): the scope as
    /// `reqflux secret` names it (`global`, a collection slug or `env:<slug>`). All of them
    /// with [`BundleSecrets::Omit`]; the ones without a value in the store otherwise.
    pub secrets_omitted: Vec<(String, String)>,
}

/// The secret variables of one scope a bundle covers.
struct SecretScopeOf<'a> {
    scope: SecretScope,
    name: String,
    variables: &'a [Variable],
}

impl Engine {
    /// Bundles the unit `selector` names, or the whole workspace when it is `None`:
    /// `env:<slug>` or an environment id, or the id or slug path of a collection, folder or
    /// request (a request wins over a folder at the same path).
    ///
    /// # Errors
    ///
    /// Nothing matches `selector`; an empty passphrase; the secret store could not be read
    /// when [`BundleSecrets::Encrypt`] asked for it; an example could not be read.
    pub fn bundle(
        &self,
        selector: Option<&str>,
        secrets: BundleSecrets<'_>,
    ) -> Result<Bundle, BundleError> {
        if let BundleSecrets::Encrypt { passphrase: "" } = secrets {
            return Err(BundleError::EmptyPassphrase);
        }
        let workspace = self.workspace();
        let mut document = Document {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            reqflux_version: env!("CARGO_PKG_VERSION").to_owned(),
            kind: BundleKind::Workspace,
            name: workspace.manifest.name.clone(),
            manifest: None,
            environments: Vec::new(),
            collections: Vec::new(),
            items: Vec::new(),
            secrets: None,
        };
        let mut scopes = Vec::new();
        match selector {
            None => {
                document.manifest = Some(toml_text(&workspace.manifest, "reqflux.toml")?);
                scopes.push(SecretScopeOf {
                    scope: SecretScope::Global,
                    name: "global".to_owned(),
                    variables: &workspace.manifest.globals,
                });
                for environment in &workspace.environments {
                    document.environments.push(environment_entry(environment)?);
                    scopes.push(environment_scope(environment));
                }
                for collection in &workspace.collections {
                    document
                        .collections
                        .push(self.collection_entry(collection)?);
                    scopes.push(collection_scope(collection));
                }
            }
            Some(selector) => {
                match unit::select(workspace, selector)
                    .ok_or_else(|| BundleError::UnknownTarget(selector.to_owned()))?
                {
                    Unit::Collection(collection) => {
                        document.kind = BundleKind::Collection;
                        document.name.clone_from(&collection.collection.name);
                        document
                            .collections
                            .push(self.collection_entry(collection)?);
                        scopes.push(collection_scope(collection));
                    }
                    Unit::Folder { folders, .. } => {
                        let Some(folder) = folders.last() else {
                            return Err(BundleError::UnknownTarget(selector.to_owned()));
                        };
                        document.kind = BundleKind::Folder;
                        document.name.clone_from(&folder.folder.name);
                        document
                            .items
                            .push(ItemEntry::Folder(self.folder_entry(folder)?));
                    }
                    Unit::Request { request, .. } => {
                        document.kind = BundleKind::Request;
                        document.name.clone_from(&request.request.name);
                        document
                            .items
                            .push(ItemEntry::Request(self.request_entry(request)?));
                    }
                    Unit::Environment(environment) => {
                        document.kind = BundleKind::Environment;
                        document.name.clone_from(&environment.environment.name);
                        document.environments.push(environment_entry(environment)?);
                        scopes.push(environment_scope(environment));
                    }
                }
            }
        }
        let (secrets_included, secrets_omitted) =
            self.seal_secrets(&scopes, secrets, &mut document)?;
        let mut text = serde_json::to_string_pretty(&document)
            .map_err(|error| BundleError::Invalid(error.to_string()))?;
        text.push('\n');
        Ok(Bundle {
            kind: document.kind,
            name: document.name,
            text,
            secrets_included,
            secrets_omitted,
        })
    }

    /// Reads the secret values of `scopes` and puts them, encrypted, in `document`, unless
    /// `secrets` leaves them out. Returns how many went in and the ones that did not.
    fn seal_secrets(
        &self,
        scopes: &[SecretScopeOf<'_>],
        secrets: BundleSecrets<'_>,
        document: &mut Document,
    ) -> Result<(usize, Vec<(String, String)>), BundleError> {
        let workspace_id = self.workspace().manifest.id;
        let mut values = Vec::new();
        let mut omitted = Vec::new();
        for of in scopes {
            for variable in of.variables.iter().filter(|variable| variable.secret) {
                let value = match secrets {
                    BundleSecrets::Omit => None,
                    BundleSecrets::Encrypt { .. } => self.secrets.get(&SecretKey {
                        workspace_id,
                        scope: of.scope,
                        key: variable.key.clone(),
                    })?,
                };
                match value {
                    Some(value) => values.push(SecretValue {
                        scope: of.scope.path(),
                        key: variable.key.clone(),
                        value,
                    }),
                    None => omitted.push((of.name.clone(), variable.key.clone())),
                }
            }
        }
        let BundleSecrets::Encrypt { passphrase } = secrets else {
            return Ok((0, omitted));
        };
        if values.is_empty() {
            return Ok((0, omitted));
        }
        let count = values.len();
        let plaintext = serde_json::to_vec(&SecretList { secrets: values })
            .map_err(|error| BundleError::Encrypt(error.to_string()))?;
        document.secrets = Some(SecretsEntry {
            cipher: CIPHER.to_owned(),
            armor: crypt::encrypt(passphrase, &plaintext)?,
        });
        Ok((count, omitted))
    }

    fn collection_entry(&self, node: &CollectionNode) -> Result<CollectionEntry, BundleError> {
        Ok(CollectionEntry {
            slug: node.slug.clone(),
            file: toml_text(&node.collection, &node.slug)?,
            pre: script(&node.scripts.pre_request),
            test: script(&node.scripts.test),
            items: self.item_entries(&node.items)?,
        })
    }

    fn folder_entry(&self, node: &FolderNode) -> Result<CollectionEntry, BundleError> {
        Ok(CollectionEntry {
            slug: node.slug.clone(),
            file: toml_text(&node.folder, &node.slug)?,
            pre: script(&node.scripts.pre_request),
            test: script(&node.scripts.test),
            items: self.item_entries(&node.items)?,
        })
    }

    fn item_entries(&self, items: &[Item]) -> Result<Vec<ItemEntry>, BundleError> {
        items
            .iter()
            .map(|item| match item {
                Item::Folder(folder) => self.folder_entry(folder).map(ItemEntry::Folder),
                Item::Request(request) => self.request_entry(request).map(ItemEntry::Request),
            })
            .collect()
    }

    fn request_entry(&self, node: &RequestNode) -> Result<RequestEntry, BundleError> {
        let examples = reqflux_store::examples(&self.root, &self.loaded, node.request.id)?
            .into_iter()
            .map(|stored| {
                let body_base64 = match stored.example.response.body {
                    reqflux_core::model::ExampleBody::File(_) => {
                        Some(STANDARD.encode(&stored.body))
                    }
                    _ => None,
                };
                Ok(ExampleEntry {
                    file: toml_text(&stored.example, &stored.slug)?,
                    body_base64,
                })
            })
            .collect::<Result<Vec<_>, BundleError>>()?;
        Ok(RequestEntry {
            slug: node.slug.clone(),
            file: toml_text(&node.request, &node.slug)?,
            pre: script(&node.scripts.pre_request),
            test: script(&node.scripts.test),
            examples,
        })
    }
}

fn collection_scope(node: &CollectionNode) -> SecretScopeOf<'_> {
    SecretScopeOf {
        scope: SecretScope::Collection(node.collection.id),
        name: node.slug.clone(),
        variables: &node.collection.variables,
    }
}

fn environment_scope(node: &reqflux_core::model::EnvironmentNode) -> SecretScopeOf<'_> {
    SecretScopeOf {
        scope: SecretScope::Environment(node.environment.id),
        name: format!("{}{}", crate::export::ENVIRONMENT_PREFIX, node.slug),
        variables: &node.environment.variables,
    }
}

fn environment_entry(
    node: &reqflux_core::model::EnvironmentNode,
) -> Result<EnvironmentEntry, BundleError> {
    Ok(EnvironmentEntry {
        slug: node.slug.clone(),
        file: toml_text(&node.environment, &node.slug)?,
    })
}

/// A non-empty script, as its sidecar holds it.
fn script(text: &Option<String>) -> Option<String> {
    text.clone().filter(|text| !text.is_empty())
}

/// An entity as the store writes its file; a secret variable's value is never serialized.
fn toml_text<T: Serialize>(document: &T, name: &str) -> Result<String, BundleError> {
    toml::to_string(document).map_err(|source| {
        BundleError::Store(StoreError::Serialize {
            path: Path::new(name).to_owned(),
            source,
        })
    })
}

/// The scripts of an entry.
fn scripts(pre: &Option<String>, test: &Option<String>) -> Scripts {
    Scripts {
        pre_request: pre.clone(),
        test: test.clone(),
    }
}
