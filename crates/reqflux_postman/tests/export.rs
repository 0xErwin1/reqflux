//! Export to Postman v2.1 (v1-features S6): what the import keeps comes back, so an import,
//! an export and another import give the same tree.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::collections::BTreeMap;

use reqflux_core::model::{Auth, CollectionNode, EnvironmentNode, Example, Id, Item, Variable};
use reqflux_postman::{Imported, collection, environment, export_collection, export_environment};
use serde_json::{Value, json};

const ROUND_TRIP: &str = include_str!("fixtures/collection-roundtrip.json");
const BASIC: &str = include_str!("fixtures/collection-basic.json");
const STAGING: &str = include_str!("fixtures/environment.json");

fn document(text: &str) -> Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("fixture is not valid JSON: {error}"),
    }
}

fn import(value: &Value) -> Imported {
    match collection(value) {
        Ok(imported) => imported,
        Err(error) => panic!("does not import: {error}"),
    }
}

fn import_environment(value: &Value) -> EnvironmentNode {
    match environment(value) {
        Ok(node) => node,
        Err(error) => panic!("environment does not import: {error}"),
    }
}

fn fixed_id() -> Id {
    match "01JB7X3K9Q0000000000000000".parse() {
        Ok(id) => id,
        Err(error) => panic!("id: {error}"),
    }
}

/// The kept Postman text with its keys sorted: an export cannot tell whether a key the
/// import flattened was on the item or on its request, so only the order may change.
fn sorted_postman(extensions: &mut toml::Table) {
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
    let Some(text) = extensions.get("postman").and_then(toml::Value::as_str) else {
        return;
    };
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("kept text is not JSON: {error}"),
    };
    extensions.insert(
        "postman".to_owned(),
        toml::Value::String(sort(value).to_string()),
    );
}

/// The imported tree with every id replaced by one fixed id, the kept Postman keys sorted
/// and the examples listed in tree order, so two imports of the same document compare
/// equal.
fn normalized(imported: &Imported) -> (CollectionNode, Vec<Vec<Example>>) {
    let mut node = imported.collection.clone();
    node.collection.id = fixed_id();
    sorted_postman(&mut node.collection.extensions);
    let mut examples = Vec::new();
    normalize_items(&mut node.items, &imported.examples, &mut examples);
    (node, examples)
}

fn normalize_items(
    items: &mut [Item],
    by_id: &BTreeMap<Id, Vec<Example>>,
    examples: &mut Vec<Vec<Example>>,
) {
    for item in items {
        match item {
            Item::Folder(folder) => {
                folder.folder.id = fixed_id();
                sorted_postman(&mut folder.folder.extensions);
                normalize_items(&mut folder.items, by_id, examples);
            }
            Item::Request(request) => {
                let mut kept = by_id.get(&request.request.id).cloned().unwrap_or_default();
                for example in &mut kept {
                    example.id = fixed_id();
                }
                examples.push(kept);
                request.request.id = fixed_id();
                sorted_postman(&mut request.request.extensions);
            }
        }
    }
}

#[test]
fn import_export_import_gives_the_same_collection() {
    let first = import(&document(ROUND_TRIP));
    let exported = export_collection(&first.collection, &first.examples);
    let second = import(&exported);
    assert_eq!(normalized(&second), normalized(&first));
    // Keys kept on the item and the request stay in their order when the item has them.
    let (Item::Folder(before), Item::Folder(after)) =
        (&first.collection.items[0], &second.collection.items[0])
    else {
        panic!("the first item is the Users folder");
    };
    let (Item::Request(before), Item::Request(after)) = (&before.items[0], &after.items[0]) else {
        panic!("the first request is List users");
    };
    assert_eq!(after.request.extensions, before.request.extensions);
}

#[test]
fn the_basic_fixture_round_trips_too() {
    let first = import(&document(BASIC));
    let second = import(&export_collection(&first.collection, &first.examples));
    assert_eq!(normalized(&second), normalized(&first));
}

#[test]
fn the_export_is_a_v2_1_collection_with_the_postman_id_restored() {
    let first = import(&document(ROUND_TRIP));
    let exported = export_collection(&first.collection, &first.examples);
    assert_eq!(
        exported["info"],
        json!({
            "_postman_id": "rt-0001",
            "name": "Round Trip",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        })
    );
    let list = &exported["item"][0]["item"][0];
    assert_eq!(list["id"], "item-list");
    assert_eq!(list["request"]["description"], "Lists users, paged.");
    assert_eq!(
        list["request"]["url"]["raw"],
        "{{base_url}}/users?page=2&size=10"
    );
    assert_eq!(list["response"][0]["code"], 200);
    assert_eq!(list["response"][0]["name"], "Page two");
    assert_eq!(list["response"][1]["name"], "Broken");
    let oauth = &exported["item"][1]["request"]["auth"];
    assert_eq!(oauth["type"], "oauth2");
    let params = oauth["oauth2"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["key"] == "authRequestParams"));
    assert_eq!(
        params.map(|row| row["value"].clone()),
        Some(json!([{ "key": "audience", "value": "api", "enabled": true }]))
    );
}

#[test]
fn secret_variables_keep_only_the_value_the_caller_put_in() {
    let first = import(&document(ROUND_TRIP));
    let exported = export_collection(&first.collection, &first.examples);
    let secret = exported["variable"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["key"] == "api_secret"))
        .cloned();
    assert_eq!(
        secret,
        Some(json!({ "key": "api_secret", "value": "", "type": "secret" }))
    );

    let mut with_value = first.collection.clone();
    for variable in &mut with_value.collection.variables {
        if variable.secret {
            variable.value = Some("given".to_owned());
        }
    }
    let exported = export_collection(&with_value, &first.examples);
    assert_eq!(
        exported["variable"][1],
        json!({ "key": "api_secret", "value": "given", "type": "secret" })
    );
}

#[test]
fn native_auth_keys_take_the_postman_spelling() {
    let mut node = import(&document(BASIC)).collection;
    let mut fields = toml::Table::new();
    fields.insert("access_key".to_owned(), "{{k}}".into());
    fields.insert("secret_key".to_owned(), "{{s}}".into());
    fields.insert("region".to_owned(), "eu-west-1".into());
    fields.insert("service".to_owned(), "s3".into());
    node.collection.auth = Auth::Other {
        kind: "awsv4".to_owned(),
        fields,
    };
    let exported = export_collection(&node, &BTreeMap::new());
    let keys: Vec<&str> = exported["auth"]["awsv4"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["key"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(keys, ["accessKey", "secretKey", "region", "service"]);
}

#[test]
fn an_environment_round_trips_with_its_extra_keys() {
    let first = import_environment(&document(STAGING));
    let exported = export_environment(&first.environment);
    assert_eq!(exported["_postman_id"], "env-42");
    assert_eq!(
        exported["values"][1],
        json!({ "key": "password", "value": "", "type": "secret", "enabled": true })
    );
    let mut second = import_environment(&exported);
    second.environment.id = first.environment.id;
    assert_eq!(second, first);
}

#[test]
fn a_native_environment_says_it_is_an_environment() {
    let mut node = import_environment(&document(STAGING));
    node.environment.extensions = toml::Table::new();
    node.environment.variables = vec![Variable::new("host", "a.test")];
    let exported = export_environment(&node.environment);
    assert_eq!(exported["_postman_variable_scope"], "environment");
    assert_eq!(exported["name"], "Staging");
}
