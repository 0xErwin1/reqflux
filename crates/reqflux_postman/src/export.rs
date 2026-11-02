//! Export to Postman collection v2.1 and environment JSON (v1-features S6), the reverse of
//! the import: what the import keeps comes back, so importing an export gives the same
//! tree.
//!
//! The keys the import kept as text under `extensions.postman` are written back where
//! Postman reads them: `_postman_id` into `info`, the rest of a request item split between
//! the item (`id`, `response`, `protocolProfileBehavior`, ...) and its `request`. Saved
//! examples become `response[]` entries, before the entries the import could not map.
//! Secret variables are written with the value the caller left in them: the engine clears
//! them unless the user asked for them in clear.

mod auth;
mod request;

use std::collections::BTreeMap;

use reqflux_core::model::{
    CollectionNode, Environment, Example, FolderNode, Id, Item, RequestNode, Scripts, Variable,
};
use serde_json::{Map, Value, json};

/// The schema URL of a Postman collection v2.1 document.
pub const COLLECTION_SCHEMA: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// The keys of a request item Postman reads on the item, not on its `request`; the import
/// keeps both in one table.
const ITEM_KEYS: &[&str] = &[
    "id",
    "uid",
    "response",
    "protocolProfileBehavior",
    "variable",
];

/// Maps a collection tree to a Postman collection v2.1 document. `examples` holds the saved
/// examples of each request by id, written as its `response` entries.
#[must_use]
pub fn export_collection(node: &CollectionNode, examples: &BTreeMap<Id, Vec<Example>>) -> Value {
    let collection = &node.collection;
    let mut rest = postman_rest(&collection.extensions);
    let mut info = Map::new();
    if let Some(id) = rest.remove("_postman_id") {
        info.insert("_postman_id".to_owned(), id);
    }
    info.insert("name".to_owned(), Value::String(collection.name.clone()));
    info.insert(
        "schema".to_owned(),
        Value::String(COLLECTION_SCHEMA.to_owned()),
    );
    let mut out = Map::new();
    out.insert("info".to_owned(), Value::Object(info));
    out.insert("item".to_owned(), items(&node.items, examples));
    insert_some(&mut out, "auth", auth::export(&collection.auth));
    insert_some(&mut out, "event", events(&node.scripts));
    if !collection.variables.is_empty() {
        out.insert(
            "variable".to_owned(),
            Value::Array(
                collection
                    .variables
                    .iter()
                    .map(collection_variable)
                    .collect(),
            ),
        );
    }
    extend_missing(&mut out, rest);
    Value::Object(out)
}

/// Maps an environment to a Postman environment document. One the import did not bring in
/// gets `_postman_variable_scope = "environment"`, which Postman reads it by.
#[must_use]
pub fn export_environment(environment: &Environment) -> Value {
    let rest = postman_rest(&environment.extensions);
    let mut out = Map::new();
    out.insert("name".to_owned(), Value::String(environment.name.clone()));
    out.insert(
        "values".to_owned(),
        Value::Array(
            environment
                .variables
                .iter()
                .map(environment_variable)
                .collect(),
        ),
    );
    if rest.is_empty() {
        out.insert(
            "_postman_variable_scope".to_owned(),
            Value::String("environment".to_owned()),
        );
    }
    extend_missing(&mut out, rest);
    Value::Object(out)
}

fn items(items: &[Item], examples: &BTreeMap<Id, Vec<Example>>) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| match item {
                Item::Folder(folder) => folder_item(folder, examples),
                Item::Request(request) => request_item(request, examples.get(&request.request.id)),
            })
            .collect(),
    )
}

fn folder_item(node: &FolderNode, examples: &BTreeMap<Id, Vec<Example>>) -> Value {
    let mut out = Map::new();
    out.insert("name".to_owned(), Value::String(node.folder.name.clone()));
    out.insert("item".to_owned(), items(&node.items, examples));
    insert_some(&mut out, "auth", auth::export(&node.folder.auth));
    insert_some(&mut out, "event", events(&node.scripts));
    extend_missing(&mut out, postman_rest(&node.folder.extensions));
    Value::Object(out)
}

/// A request item: the request, its scripts, its examples then the `response` entries the
/// import could not map, and the kept keys on the item or the request as Postman reads
/// them.
fn request_item(node: &RequestNode, examples: Option<&Vec<Example>>) -> Value {
    let request = &node.request;
    let rest = postman_rest(&request.extensions);
    let mut responses: Vec<Value> = examples
        .into_iter()
        .flatten()
        .map(request::example)
        .collect();
    let mut document = request::request(request);
    let mut out = Map::new();
    out.insert("name".to_owned(), Value::String(request.name.clone()));
    insert_some(&mut out, "event", events(&node.scripts));
    // The kept keys go back in the order they were kept, `response` in its place.
    for (key, value) in rest {
        match (key.as_str(), value) {
            ("response", Value::Array(unmapped)) => {
                responses.extend(unmapped);
                out.insert(key, Value::Array(std::mem::take(&mut responses)));
            }
            // A `response` that is not an array never mapped: it goes back as it was
            // unless examples take its place.
            ("response", other) => {
                if responses.is_empty() {
                    out.insert(key, other);
                }
            }
            (name, value) if ITEM_KEYS.contains(&name) => {
                out.entry(key).or_insert(value);
            }
            (_, value) => {
                document.entry(key).or_insert(value);
            }
        }
    }
    if !responses.is_empty() {
        out.insert("response".to_owned(), Value::Array(responses));
    }
    request::settings(&request.settings, &mut out);
    out.insert("request".to_owned(), Value::Object(document));
    Value::Object(out)
}

/// The `event` array of a collection, folder or request: `prerequest` then `test`, each
/// script split into its lines.
fn events(scripts: &Scripts) -> Option<Value> {
    let mut out = Vec::new();
    for (listen, script) in [
        ("prerequest", &scripts.pre_request),
        ("test", &scripts.test),
    ] {
        if let Some(text) = script.as_deref().filter(|text| !text.is_empty()) {
            let exec: Vec<Value> = text
                .split('\n')
                .map(|line| Value::String(line.to_owned()))
                .collect();
            out.push(json!({
                "listen": listen,
                "script": { "type": "text/javascript", "exec": exec },
            }));
        }
    }
    (!out.is_empty()).then_some(Value::Array(out))
}

/// A collection `variable` row: `disabled` when off, `type = "secret"` for a secret.
fn collection_variable(variable: &Variable) -> Value {
    let mut out = variable_row(variable, "string");
    if !variable.enabled {
        out.insert("disabled".to_owned(), Value::Bool(true));
    }
    Value::Object(out)
}

/// An environment `values` row, which carries `enabled` instead of `disabled`.
fn environment_variable(variable: &Variable) -> Value {
    let mut out = variable_row(variable, "default");
    out.insert("enabled".to_owned(), Value::Bool(variable.enabled));
    Value::Object(out)
}

fn variable_row(variable: &Variable, plain: &str) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("key".to_owned(), Value::String(variable.key.clone()));
    out.insert(
        "value".to_owned(),
        Value::String(variable.value.clone().unwrap_or_default()),
    );
    out.insert(
        "type".to_owned(),
        Value::String(if variable.secret { "secret" } else { plain }.to_owned()),
    );
    if let Some(description) = &variable.description {
        out.insert("description".to_owned(), Value::String(description.clone()));
    }
    out
}

/// The keys the import kept as JSON text under `postman`; empty when there are none or the
/// text is not a JSON object.
pub(crate) fn postman_rest(extensions: &toml::Table) -> Map<String, Value> {
    match extensions
        .get("postman")
        .and_then(toml::Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
    {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn insert_some(out: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        out.insert(key.to_owned(), value);
    }
}

/// Adds the kept keys the mapping did not write itself.
fn extend_missing(out: &mut Map<String, Value>, rest: Map<String, Value>) {
    for (key, value) in rest {
        out.entry(key).or_insert(value);
    }
}
