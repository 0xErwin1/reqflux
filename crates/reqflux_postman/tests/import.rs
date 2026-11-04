//! Fixture-driven tests for the collection and environment mapping (S1-S7).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::collections::BTreeSet;

use reqflux_core::model::{Auth, Body, EnvironmentNode, Item, RawLanguage};
use reqflux_postman::{Imported, PostmanError, collection, environment, environment_in};

const BASIC: &str = include_str!("fixtures/collection-basic.json");
const GRAPHQL: &str = include_str!("fixtures/collection-graphql.json");
const STAGING: &str = include_str!("fixtures/environment.json");

fn document(text: &str) -> serde_json::Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("fixture is not valid JSON: {error}"),
    }
}

fn import(text: &str) -> Imported {
    match collection(&document(text)) {
        Ok(imported) => imported,
        Err(error) => panic!("fixture does not import: {error}"),
    }
}

fn environment_node(taken: &mut BTreeSet<String>) -> EnvironmentNode {
    match environment_in(&document(STAGING), taken) {
        Ok(node) => node,
        Err(error) => panic!("environment fixture does not import: {error}"),
    }
}

fn postman_text(extensions: &toml::Table) -> &str {
    match extensions.get("postman").and_then(toml::Value::as_str) {
        Some(text) => text,
        None => panic!("no postman payload in extensions"),
    }
}

#[test]
fn tree_slugs_and_order_follow_the_document() {
    let imported = import(BASIC);
    let node = &imported.collection;
    assert_eq!(node.slug, "basic-collection");
    assert_eq!(node.collection.name, "Basic Collection");
    assert_eq!(node.collection.order, vec!["auth", "create-user"]);
    let [Item::Folder(auth), Item::Request(create_user)] = node.items.as_slice() else {
        panic!("collection holds one folder then one request");
    };
    assert_eq!(auth.slug, "auth");
    assert_eq!(auth.folder.name, "Auth");
    assert_eq!(auth.folder.order, vec!["get-token", "get-token-2"]);
    assert_eq!(create_user.slug, "create-user");
    assert!(imported.slugs.contains("get-token-2"));
}

#[test]
fn request_maps_method_url_params_headers_auth_body_and_scripts() {
    let imported = import(BASIC);
    let Item::Folder(auth) = &imported.collection.items[0] else {
        panic!("first item is a folder");
    };
    let Item::Request(get_token) = &auth.items[0] else {
        panic!("first child is a request");
    };
    let request = &get_token.request;
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url, "https://api.example.com/token");
    assert!(request.params.is_empty());
    assert_eq!(request.headers.len(), 2);
    assert_eq!(request.headers[0].key, "Content-Type");
    assert!(request.headers[0].enabled);
    assert!(!request.headers[1].enabled);
    assert_eq!(
        request.auth,
        Auth::Bearer {
            token: "secret-token".to_owned()
        }
    );
    assert_eq!(
        request.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: "{\"grant\":\"client\"}".to_owned()
        }
    );
    assert_eq!(
        get_token.scripts.pre_request.as_deref(),
        Some("console.log('before');")
    );
    assert_eq!(
        get_token.scripts.test.as_deref(),
        Some("pm.test('ok', () => true);")
    );
}

#[test]
fn url_assembles_and_query_rows_map_to_params() {
    let imported = import(BASIC);
    let Item::Request(create_user) = &imported.collection.items[1] else {
        panic!("second item is a request");
    };
    assert_eq!(create_user.request.url, "api.example.com/users");
    let params = &create_user.request.params;
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].key, "sendWelcome");
    assert!(params[0].enabled);
    assert_eq!(params[1].key, "dryRun");
    assert!(!params[1].enabled);
    assert_eq!(
        create_user.request.body,
        Body::Urlencoded {
            fields: vec![reqflux_core::model::KeyValue {
                key: "name".to_owned(),
                value: "Ada".to_owned(),
                description: None,
                enabled: true,
            }]
        }
    );
}

#[test]
fn collection_variables_and_inherited_auth_map() {
    let imported = import(BASIC);
    let variables = &imported.collection.collection.variables;
    assert_eq!(variables.len(), 2);
    assert_eq!(variables[0].key, "baseUrl");
    assert!(!variables[0].secret);
    assert!(variables[1].secret);
    assert_eq!(variables[1].value, None);
    assert!(imported.collection.collection.auth.is_inherit());
}

#[test]
fn extensions_keep_the_original_json() {
    let imported = import(BASIC);
    let kept = postman_text(&imported.collection.collection.extensions);
    assert!(kept.contains("\"_postman_id\":\"abc-123\""));
    let Item::Folder(auth) = &imported.collection.items[0] else {
        panic!("first item is a folder");
    };
    let Item::Request(get_token) = &auth.items[0] else {
        panic!("first child is a request");
    };
    let kept = postman_text(&get_token.request.extensions);
    // The description is read into the document, not kept as text.
    assert!(!kept.contains("description"));
    assert_eq!(
        get_token.request.description.as_deref(),
        Some("Fetches a token.")
    );
    assert!(kept.contains("\"response\":[]"));
}

#[test]
fn graphql_body_and_unsupported_auth_survive() {
    let imported = import(GRAPHQL);
    let Item::Request(query) = &imported.collection.items[0] else {
        panic!("first item is a request");
    };
    assert_eq!(
        query.request.body,
        Body::Graphql {
            query: "query { me { id } }".to_owned(),
            variables: "{\"limit\":5}".to_owned()
        }
    );
    let mut fields = toml::Table::new();
    fields.insert(
        "accessTokenUrl".to_owned(),
        toml::Value::from("https://auth.example.com/token"),
    );
    assert_eq!(
        query.request.auth,
        Auth::Other {
            kind: "oauth2".to_owned(),
            fields
        }
    );
}

#[test]
fn environment_maps_values_and_hides_secret_values() {
    let mut taken = BTreeSet::new();
    let node = environment_node(&mut taken);
    assert_eq!(node.slug, "staging");
    assert_eq!(node.environment.name, "Staging");
    let variables = &node.environment.variables;
    assert_eq!(variables.len(), 3);
    assert_eq!(variables[0].key, "host");
    assert_eq!(variables[0].value.as_deref(), Some("staging.example.com"));
    assert!(variables[1].secret);
    assert_eq!(variables[1].value, None);
    assert!(!variables[2].enabled);
    let kept = postman_text(&node.environment.extensions);
    assert!(kept.contains("\"_postman_id\":\"env-42\""));
}

#[test]
fn environment_slug_avoids_taken_collection_slugs() {
    let mut taken = BTreeSet::from(["staging".to_owned()]);
    let node = environment_node(&mut taken);
    assert_eq!(node.slug, "staging-2");
    let plain = match environment(&document(STAGING)) {
        Ok(node) => node,
        Err(error) => panic!("environment fixture does not import: {error}"),
    };
    assert_eq!(plain.slug, "staging");
}

#[test]
fn errors_name_what_is_wrong_and_where() {
    assert_eq!(
        collection(&serde_json::json!([])),
        Err(PostmanError::NotAnObject)
    );
    assert_eq!(
        collection(&serde_json::json!({"info": {}})),
        Err(PostmanError::MissingName)
    );
    assert_eq!(
        collection(&serde_json::json!({
            "info": { "name": "X" },
            "item": [{ "name": "No verb", "request": { "url": "https://x.dev" } }]
        })),
        Err(PostmanError::MissingMethod {
            path: "item[0].request".to_owned()
        })
    );
    assert_eq!(
        collection(&serde_json::json!({
            "info": { "name": "X" },
            "item": [{ "name": "No url", "request": { "method": "GET" } }]
        })),
        Err(PostmanError::MissingUrl {
            path: "item[0].request".to_owned()
        })
    );
    assert_eq!(
        collection(&serde_json::json!({
            "info": { "name": "X" },
            "item": [{ "name": "Empty body", "request": {
                "method": "POST", "url": "https://x.dev", "body": { "mode": "urlencoded" }
            } }]
        })),
        Err(PostmanError::MissingBodyPayload {
            path: "item[0].request".to_owned(),
            mode: "urlencoded".to_owned(),
            field: "urlencoded"
        })
    );
    assert_eq!(
        collection(&serde_json::json!({
            "info": { "name": "X" },
            "item": [{ "name": "Junk" }]
        })),
        Err(PostmanError::UnknownItem {
            path: "item[0]".to_owned()
        })
    );
}

#[test]
fn a_bad_nested_item_fails_with_its_path() {
    let nested = serde_json::json!({
        "info": { "name": "N" },
        "item": [{ "name": "F", "item": [
            { "name": "a", "request": { "method": "GET", "url": "https://x.dev" } },
            { "name": "b", "request": { "method": "GET", "url": "https://x.dev" } },
            { "name": "c", "request": { "method": "GET", "url": "https://x.dev" } },
            { "name": "d", "request": { "method": "GET" } }
        ] }]
    });
    let error = collection(&nested).expect_err("the fourth item has no URL");
    assert_eq!(
        error,
        PostmanError::MissingUrl {
            path: "item[0].item[3].request".to_owned()
        }
    );
    assert!(error.to_string().contains("item[0].item[3].request"));
}
