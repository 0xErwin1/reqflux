//! Postman `response[]` entries imported as saved examples (v1-features S4).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::fs;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use reqflux_core::model::{ExampleBody, Item, KeyValue};
use reqflux_engine::{Engine, ImportError, MASK, postman, postman_preview};

fn collection() -> serde_json::Value {
    serde_json::json!({
        "info": { "name": "Shop", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
        "item": [
            {
                "name": "Get User",
                "request": {
                    "method": "GET",
                    "url": { "raw": "{{base}}/users/1?verbose=true", "query": [{ "key": "verbose", "value": "true" }] }
                },
                "response": [
                    {
                        "name": "OK",
                        "originalRequest": {
                            "method": "GET",
                            "header": [{ "key": "Authorization", "value": "Bearer {{token}}" }],
                            "url": { "raw": "{{base}}/users/42" }
                        },
                        "status": "OK",
                        "code": 200,
                        "_postman_previewlanguage": "json",
                        "header": [
                            { "key": "Content-Type", "value": "application/json" },
                            { "key": "Set-Cookie", "value": "session=abc123; Path=/" },
                            { "key": "Authorization", "value": "Bearer live-token" }
                        ],
                        "body": "{\"id\":42}"
                    },
                    {
                        "name": "Missing",
                        "code": 404,
                        "body": ""
                    },
                    {
                        "name": "Broken",
                        "body": "no status code"
                    }
                ]
            },
            {
                "name": "Ping",
                "request": { "method": "GET", "url": "{{base}}/ping" },
                "response": []
            }
        ]
    })
}

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

/// Opens with an empty in-memory secret store, so no test touches the OS keyring.
fn open_engine(root: &Path) -> Engine {
    match Engine::open_with_secrets(
        root,
        std::sync::Arc::new(reqflux_engine::MemorySecretStore::new()),
    ) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

/// Every file under `root` with its bytes, relative to it, sorted.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => panic!("read {}: {error}", dir.display()),
        };
        for entry in entries {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => panic!("list {}: {error}", dir.display()),
            };
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let bytes = match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => panic!("read {}: {error}", path.display()),
                };
                if let Ok(relative) = path.strip_prefix(root) {
                    out.push((relative.to_path_buf(), bytes));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn responses_become_examples_readable_through_the_engine() {
    let dir = temp();
    let summary = match postman(dir.path(), &collection(), &[]) {
        Ok(summary) => summary,
        Err(error) => panic!("import: {error}"),
    };
    assert_eq!(summary.requests, 2);
    assert_eq!(summary.examples, 2);

    let engine = open_engine(dir.path());
    let examples = match engine.examples("shop/get-user") {
        Ok(examples) => examples,
        Err(error) => panic!("examples: {error}"),
    };
    let names: Vec<_> = examples.iter().map(|saved| saved.slug.as_str()).collect();
    assert_eq!(names, ["missing", "ok"]);

    let missing = &examples[0].example;
    assert_eq!(missing.response.status, 404);
    assert_eq!(missing.response.body, ExampleBody::None);
    assert_eq!(missing.request.url, "{{base}}/users/1");
    assert_eq!(
        missing.request.params,
        [KeyValue::new("verbose", "true")],
        "without originalRequest the item's request is the example's request"
    );

    let ok = &examples[1];
    assert_eq!(ok.example.name, "OK");
    assert_eq!(ok.example.request.url, "{{base}}/users/42");
    assert_eq!(
        ok.example.request.headers,
        [KeyValue::new("Authorization", "Bearer {{token}}")],
        "the request is kept as written"
    );
    assert_eq!(ok.example.response.status, 200);
    assert_eq!(ok.example.response.reason.as_deref(), Some("OK"));
    assert_eq!(
        ok.example.response.headers,
        [
            KeyValue::new("Content-Type", "application/json"),
            KeyValue::new("Set-Cookie", format!("session={MASK}; Path=/")),
            KeyValue::new("Authorization", format!("Bearer {MASK}")),
        ]
    );
    assert_eq!(ok.body, b"{\"id\":42}");
    assert!(
        ok.example
            .extensions
            .get("postman")
            .and_then(toml::Value::as_str)
            .is_some_and(|kept| kept.contains("_postman_previewlanguage")),
        "keys the example does not map stay as Postman text"
    );

    let items = &engine.workspace().collections[0].items;
    let Item::Request(get_user) = &items[0] else {
        panic!("expected Get User first");
    };
    let kept = get_user
        .request
        .extensions
        .get("postman")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    assert!(
        kept.contains("no status code"),
        "the unmappable response stays"
    );
    assert!(
        !kept.contains("\"OK\""),
        "a mapped response leaves extensions"
    );
    assert!(!dir.path().join("collections/shop/ping.examples").exists());
}

#[test]
fn a_request_whose_responses_all_map_drops_response_from_extensions() {
    let mut document = collection();
    if let Some(responses) = document["item"][0]["response"].as_array_mut() {
        responses.truncate(2);
    }
    let dir = temp();
    if let Err(error) = postman(dir.path(), &document, &[]) {
        panic!("import: {error}");
    }
    let engine = open_engine(dir.path());
    let Item::Request(get_user) = &engine.workspace().collections[0].items[0] else {
        panic!("expected Get User first");
    };
    assert!(get_user.request.extensions.get("postman").is_none());
}

#[test]
fn the_preview_counts_and_lists_the_examples() {
    let previewed = temp();
    let preview = match postman_preview(previewed.path(), &collection(), &[]) {
        Ok(preview) => preview,
        Err(error) => panic!("preview: {error}"),
    };
    assert_eq!(preview.examples, 2);
    assert!(snapshot(previewed.path()).is_empty());

    let imported = temp();
    if let Err(error) = postman(imported.path(), &collection(), &[]) {
        panic!("import: {error}");
    }
    let mut listed = preview.files.clone();
    listed.sort();
    let written: Vec<_> = snapshot(imported.path())
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(listed, written);
    assert!(listed.contains(&PathBuf::from(
        "collections/shop/get-user.examples/ok.example.toml"
    )));
}

#[test]
fn a_re_import_with_examples_fails_and_changes_nothing() {
    let dir = temp();
    if let Err(error) = postman(dir.path(), &collection(), &[]) {
        panic!("import: {error}");
    }
    let before = snapshot(dir.path());
    match postman(dir.path(), &collection(), &[]) {
        Err(ImportError::Exists { .. }) => {}
        other => panic!("expected Exists, got {other:?}"),
    }
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn an_import_without_responses_writes_no_examples() {
    let mut document = collection();
    if let Some(item) = document["item"][0].as_object_mut() {
        item.remove("response");
    }
    let dir = temp();
    let summary = match postman(dir.path(), &document, &[]) {
        Ok(summary) => summary,
        Err(error) => panic!("import: {error}"),
    };
    assert_eq!(summary.examples, 0);
    let paths: Vec<_> = snapshot(dir.path())
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        paths,
        [
            PathBuf::from("collections/shop/collection.toml"),
            PathBuf::from("collections/shop/get-user.req.toml"),
            PathBuf::from("collections/shop/ping.req.toml"),
            PathBuf::from("reqflux.toml"),
        ]
    );
    let engine = open_engine(dir.path());
    match engine.examples("shop/get-user") {
        Ok(examples) => assert!(examples.is_empty()),
        Err(error) => panic!("examples: {error}"),
    }
}
