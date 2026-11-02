//! Export to Postman v2.1 through the engine (v1-features S6): a collection, a folder, a
//! request or an environment; secret values only when asked for.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use reqflux_core::model::{CollectionNode, EnvironmentNode, Example, Id, Item};
use reqflux_engine::{
    Engine, ExportError, MemorySecretStore, PostmanExport, PostmanKind, SecretKey, SecretScope,
    SecretStore, postman,
};
use serde_json::{Value, json};

use common::{open_engine, snapshot, temp_dir};

const ROUND_TRIP: &str =
    include_str!("../../reqflux_postman/tests/fixtures/collection-roundtrip.json");
const BASIC: &str = include_str!("../../reqflux_postman/tests/fixtures/collection-basic.json");
const STAGING: &str = include_str!("../../reqflux_postman/tests/fixtures/environment.json");

fn document(text: &str) -> Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("fixture is not valid JSON: {error}"),
    }
}

fn imported(root: &Path, collection: &Value, environments: &[Value]) {
    if let Err(error) = postman(root, collection, environments) {
        panic!("import: {error}");
    }
}

fn open_with(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn open(root: &Path) -> Engine {
    match open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn export(engine: &Engine, selector: &str, include_secrets: bool) -> PostmanExport {
    match engine.export_postman(selector, include_secrets) {
        Ok(export) => export,
        Err(error) => panic!("export {selector}: {error}"),
    }
}

fn fixed() -> Id {
    match "01JB7X3K9Q0000000000000000".parse() {
        Ok(id) => id,
        Err(error) => panic!("id: {error}"),
    }
}

fn sort(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sort(v))).collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort).collect()),
        other => other,
    }
}

fn sorted_postman(extensions: &mut toml::Table) {
    if let Some(text) = extensions.get("postman").and_then(toml::Value::as_str) {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(error) => panic!("kept text: {error}"),
        };
        extensions.insert("postman".to_owned(), sort(value).to_string().into());
    }
}

/// The collections of the workspace with fixed ids and sorted kept keys, each request with
/// its saved examples (ids fixed), so two imports compare equal.
fn normalized(engine: &Engine) -> (Vec<CollectionNode>, Vec<Vec<Example>>, Vec<EnvironmentNode>) {
    let mut collections = engine.workspace().collections.clone();
    let mut examples = Vec::new();
    for collection in &mut collections {
        collection.collection.id = fixed();
        sorted_postman(&mut collection.collection.extensions);
        let slug = collection.slug.clone();
        normalize(engine, &slug, &mut collection.items, &mut examples);
    }
    let mut environments = engine.workspace().environments.clone();
    for environment in &mut environments {
        environment.environment.id = fixed();
    }
    (collections, examples, environments)
}

fn normalize(engine: &Engine, path: &str, items: &mut [Item], examples: &mut Vec<Vec<Example>>) {
    for item in items {
        match item {
            Item::Folder(folder) => {
                folder.folder.id = fixed();
                sorted_postman(&mut folder.folder.extensions);
                let path = format!("{path}/{}", folder.slug);
                normalize(engine, &path, &mut folder.items, examples);
            }
            Item::Request(request) => {
                let selector = format!("{path}/{}", request.slug);
                let saved = match engine.examples(&selector) {
                    Ok(saved) => saved,
                    Err(error) => panic!("examples of {selector}: {error}"),
                };
                examples.push(
                    saved
                        .into_iter()
                        .map(|saved| Example {
                            id: fixed(),
                            ..saved.example
                        })
                        .collect(),
                );
                request.request.id = fixed();
                sorted_postman(&mut request.request.extensions);
            }
        }
    }
}

#[test]
fn import_export_import_gives_the_same_workspace() {
    let first = temp_dir();
    imported(first.path(), &document(ROUND_TRIP), &[document(STAGING)]);
    let engine = open(first.path());
    let collection = export(&engine, "round-trip", false);
    let environment = export(&engine, "env:staging", false);
    assert_eq!(collection.kind, PostmanKind::Collection);
    assert_eq!(environment.kind, PostmanKind::Environment);

    let second = temp_dir();
    imported(second.path(), &collection.document, &[environment.document]);
    assert_eq!(normalized(&open(second.path())), normalized(&engine));
}

#[test]
fn secret_values_are_written_empty_unless_asked_for() {
    let dir = temp_dir();
    imported(dir.path(), &document(ROUND_TRIP), &[document(STAGING)]);
    let store = Arc::new(MemorySecretStore::new());
    let engine = open_with(dir.path(), Arc::clone(&store));
    let workspace_id = engine.workspace().manifest.id;
    let collection_id = engine.workspace().collections[0].collection.id;
    let set = store.set(
        &SecretKey {
            workspace_id,
            scope: SecretScope::Collection(collection_id),
            key: "api_secret".to_owned(),
        },
        "kept-in-the-keyring",
    );
    assert_eq!(set, Ok(()));

    let plain = export(&engine, "round-trip", false);
    assert!(
        !plain.document.to_string().contains("kept-in-the-keyring"),
        "{}",
        plain.document
    );
    assert_eq!(plain.empty_secrets, ["api_secret"]);
    assert_eq!(
        plain.document["variable"][1],
        json!({ "key": "api_secret", "value": "", "type": "secret" })
    );

    let clear = export(&engine, "round-trip", true);
    assert_eq!(clear.empty_secrets, Vec::<String>::new());
    assert_eq!(
        clear.document["variable"][1]["value"],
        "kept-in-the-keyring"
    );

    let environment = export(&engine, "env:staging", true);
    assert_eq!(environment.empty_secrets, ["password"]);
    assert_eq!(environment.document["values"][1]["value"], "");
}

#[test]
fn an_unreadable_secret_store_fails_only_when_secrets_are_asked_for() {
    let dir = temp_dir();
    imported(dir.path(), &document(ROUND_TRIP), &[]);
    let engine = open_with(
        dir.path(),
        Arc::new(MemorySecretStore::unavailable("no keyring here")),
    );
    assert!(engine.export_postman("round-trip", false).is_ok());
    let error = match engine.export_postman("round-trip", true) {
        Err(error) => error,
        Ok(_) => panic!("the store is unavailable"),
    };
    assert!(matches!(error, ExportError::Secret(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "secret values could not be read: the OS keyring is not available: no keyring here"
    );
}

#[test]
fn a_folder_or_a_request_comes_inside_the_path_to_it() {
    let dir = temp_dir();
    imported(dir.path(), &document(ROUND_TRIP), &[]);
    let engine = open(dir.path());

    let folder = export(&engine, "round-trip/users/nested", false);
    assert_eq!(folder.document["info"]["name"], "Round Trip");
    assert_eq!(folder.document["auth"]["type"], "bearer");
    let users = &folder.document["item"];
    assert_eq!(users.as_array().map(Vec::len), Some(1));
    assert_eq!(users[0]["name"], "Users");
    assert_eq!(users[0]["auth"]["type"], "digest");
    assert_eq!(users[0]["item"][0]["name"], "Nested");
    assert_eq!(users[0]["item"][0]["item"][0]["name"], "Upload avatar");
    assert_eq!(users[0]["item"].as_array().map(Vec::len), Some(1));

    let list_id = engine.workspace().collections[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Folder(folder) => folder.items.iter().find_map(|item| match item {
                Item::Request(request) if request.slug == "list-users" => Some(request.request.id),
                _ => None,
            }),
            Item::Request(_) => None,
        });
    let Some(list_id) = list_id else {
        panic!("list-users is in the Users folder");
    };
    let request = export(&engine, &list_id.to_string(), false);
    let list = &request.document["item"][0]["item"][0];
    assert_eq!(list["name"], "List users");
    assert_eq!(list["response"][0]["name"], "Page two");
    assert_eq!(
        request.document["item"][0]["item"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn an_unknown_target_names_the_selector() {
    let dir = temp_dir();
    imported(dir.path(), &document(BASIC), &[]);
    let engine = open(dir.path());
    let error = match engine.export_postman("nope/missing", false) {
        Err(error) => error,
        Ok(_) => panic!("nothing is there"),
    };
    assert_eq!(
        error.to_string(),
        "no collection, folder, request or environment matches nope/missing"
    );
    assert!(engine.export_postman("env:missing", false).is_err());
}

/// The import keeps writing the same files: an export adds no field to it.
#[test]
fn the_postman_import_still_writes_the_same_files() {
    let first = temp_dir();
    let second = temp_dir();
    imported(first.path(), &document(BASIC), &[document(STAGING)]);
    imported(second.path(), &document(BASIC), &[document(STAGING)]);
    let names =
        |root: &Path| -> Vec<String> { snapshot(root).into_iter().map(|(name, _)| name).collect() };
    assert_eq!(
        names(first.path()),
        [
            "collections/basic-collection/auth/folder.toml",
            "collections/basic-collection/auth/get-token-2.req.toml",
            "collections/basic-collection/auth/get-token.pre.js",
            "collections/basic-collection/auth/get-token.req.toml",
            "collections/basic-collection/auth/get-token.test.js",
            "collections/basic-collection/collection.toml",
            "collections/basic-collection/create-user.req.toml",
            "environments/staging.env.toml",
            "reqflux.toml",
        ]
    );
    assert_eq!(names(second.path()), names(first.path()));
}
