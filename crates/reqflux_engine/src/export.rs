//! Export to Postman v2.1 (v1-features S6): a collection, a folder, a request or an
//! environment, mapped by [`reqflux_postman`](reqflux_postman) with no IO of its own.
//!
//! A folder or a request is exported as a collection holding only the path to it, so the
//! auth and scripts it inherits from its folders and collection come with it. Each exported
//! request takes its saved examples as `response[]` entries; a body kept in a file goes as
//! text when it is UTF-8 and is left out otherwise.
//!
//! Secret variable values never live in workspace files. An export writes every secret
//! variable with an empty value and lists its key in [`PostmanExport::empty_secrets`],
//! unless the caller asks for the values in clear, which are then read from the secret
//! store; one that is not set is still written empty and listed.

pub(crate) mod unit;

use std::collections::BTreeMap;

use reqflux_core::model::{CollectionNode, Example, ExampleBody, Id, Item, Variable};
use reqflux_store::StoreError;
use serde_json::Value;

use crate::engine::Engine;
use crate::error::ExportError;
use crate::secrets::{SecretKey, SecretScope};
use unit::{Unit, pruned};

pub use unit::ENVIRONMENT_PREFIX;

/// Which Postman document an export is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostmanKind {
    /// A collection v2.1, also for a folder or a request.
    Collection,
    /// An environment.
    Environment,
}

/// A Postman document made from a workspace unit.
#[derive(Debug, Clone, PartialEq)]
pub struct PostmanExport {
    /// Which document it is.
    pub kind: PostmanKind,
    /// The name of the collection or environment.
    pub name: String,
    /// The document.
    pub document: Value,
    /// The keys of the secret variables written with an empty value, in document order.
    pub empty_secrets: Vec<String>,
}

impl Engine {
    /// Exports the unit `selector` names as a Postman document: `env:<slug>` or an
    /// environment id, or the id or slug path of a collection, folder or request (a request
    /// wins over a folder at the same path). With `include_secrets`, secret variable values
    /// are read from the secret store and written in clear.
    ///
    /// # Errors
    ///
    /// Nothing matches `selector`; the secret store could not be read when
    /// `include_secrets` asked for it; a saved example could not be read.
    pub fn export_postman(
        &self,
        selector: &str,
        include_secrets: bool,
    ) -> Result<PostmanExport, ExportError> {
        let unit = unit::select(self.workspace(), selector)
            .ok_or_else(|| ExportError::UnknownTarget(selector.to_owned()))?;
        let mut empty_secrets = Vec::new();
        let node = match unit {
            Unit::Environment(node) => {
                let mut environment = node.environment.clone();
                self.fill_secrets(
                    SecretScope::Environment(environment.id),
                    &mut environment.variables,
                    include_secrets,
                    &mut empty_secrets,
                )?;
                return Ok(PostmanExport {
                    kind: PostmanKind::Environment,
                    name: environment.name.clone(),
                    document: reqflux_postman::export_environment(&environment),
                    empty_secrets,
                });
            }
            Unit::Collection(collection) => collection.clone(),
            Unit::Folder {
                collection,
                folders,
            } => match folders.split_last() {
                Some((folder, parents)) => {
                    pruned(collection, parents, Item::Folder((*folder).clone()))
                }
                None => collection.clone(),
            },
            Unit::Request {
                collection,
                folders,
                request,
            } => pruned(collection, &folders, Item::Request(request.clone())),
        };
        let mut node = node;
        self.fill_secrets(
            SecretScope::Collection(node.collection.id),
            &mut node.collection.variables,
            include_secrets,
            &mut empty_secrets,
        )?;
        let examples = self.examples_of(&node)?;
        Ok(PostmanExport {
            kind: PostmanKind::Collection,
            name: node.collection.name.clone(),
            document: reqflux_postman::export_collection(&node, &examples),
            empty_secrets,
        })
    }

    /// Gives every secret of `variables` its value from the secret store when `include` asks
    /// for it, else none; the key of each one left without a value goes to `empty`.
    fn fill_secrets(
        &self,
        scope: SecretScope,
        variables: &mut [Variable],
        include: bool,
        empty: &mut Vec<String>,
    ) -> Result<(), ExportError> {
        let workspace_id = self.workspace().manifest.id;
        for variable in variables.iter_mut().filter(|variable| variable.secret) {
            variable.value = if include {
                self.secrets.get(&SecretKey {
                    workspace_id,
                    scope,
                    key: variable.key.clone(),
                })?
            } else {
                None
            };
            if variable.value.is_none() {
                empty.push(variable.key.clone());
            }
        }
        Ok(())
    }

    /// The saved examples of every request under `node`, by request id, a body kept in a
    /// file as text when it is UTF-8.
    pub(crate) fn examples_of(
        &self,
        node: &CollectionNode,
    ) -> Result<BTreeMap<Id, Vec<Example>>, StoreError> {
        let mut out = BTreeMap::new();
        for request in unit::requests(&node.items) {
            let id = request.request.id;
            let stored = reqflux_store::examples(&self.root, &self.loaded, id)?;
            if stored.is_empty() {
                continue;
            }
            let examples = stored
                .into_iter()
                .map(|stored| {
                    let mut example = stored.example;
                    if let ExampleBody::File(_) = example.response.body {
                        example.response.body = match String::from_utf8(stored.body) {
                            Ok(text) => ExampleBody::Text(text),
                            Err(_) => ExampleBody::None,
                        };
                    }
                    example
                })
                .collect();
            out.insert(id, examples);
        }
        Ok(out)
    }
}
