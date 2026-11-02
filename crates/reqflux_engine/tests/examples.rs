//! Saved examples (v1-features S4): a request and its response kept beside the request as
//! files, created, listed, renamed and deleted through the engine; and saving a response
//! body to a file.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::{id, id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::{
    Engine, ExampleError, MASK, MemorySecretStore, Response, SecretKey, SecretScope, SecretStore,
    Timings,
};

const REQUEST: &str = "collections/api/r.req.toml";

/// `api/r`: GET `{{base}}/items` with a bearer `{{token}}`; `token` is a collection secret
/// whose value, `s3cret`, is in the secret store.
fn engine(root: &Path) -> Engine {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\n\n[[variables]]\nkey = \"token\"\nsecret = true\n",
            id_text(1)
        ),
    );
    write(
        root,
        REQUEST,
        &format!(
            "id = \"{}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/items\"\n\n[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{token}}}}\"\n",
            id_text(2)
        ),
    );
    let store = Arc::new(MemorySecretStore::new());
    let key = SecretKey {
        workspace_id: id(900),
        scope: SecretScope::Collection(id(1)),
        key: "token".to_owned(),
    };
    store.set(&key, "s3cret").unwrap();
    Engine::open_with_secrets(root, store).unwrap()
}

fn response(body: &[u8]) -> Response {
    Response {
        status: 200,
        reason: Some("OK".to_owned()),
        version: "1.1".to_owned(),
        headers: vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("x-echo".to_owned(), "token s3cret".to_owned()),
            ("set-cookie".to_owned(), "sid=abc123; Path=/".to_owned()),
        ],
        body: body.to_vec(),
        final_url: "http://api.test/items".to_owned(),
        elapsed: Duration::from_millis(5),
        timings: Timings::default(),
    }
}

fn save(engine: &Engine, name: &str, body: &[u8]) -> reqflux_core::model::Id {
    let request = engine.workspace().collections[0]
        .items
        .iter()
        .find_map(|item| match item {
            reqflux_core::model::Item::Request(node) => Some(node.request.clone()),
            reqflux_core::model::Item::Folder(_) => None,
        })
        .unwrap();
    let (masker, problem) = engine.workspace_masker();
    assert_eq!(problem, None);
    engine
        .save_example("api/r", name, &request, &response(body), &masker)
        .unwrap()
}

fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap()
}

#[test]
fn an_example_is_saved_beside_its_request_as_written_with_secrets_masked() {
    let dir = temp_dir();
    let root = dir.path();
    let engine = engine(root);

    let id = save(&engine, "OK", b"{\"token\": \"s3cret\"}");

    assert_eq!(
        read(root, "collections/api/r.examples/ok.example.toml"),
        format!(
            "id = \"{id}\"
name = \"OK\"

[request]
method = \"GET\"
url = \"{{{{base}}}}/items\"

[[request.headers]]
key = \"Authorization\"
value = \"Bearer {{{{token}}}}\"

[response]
status = 200
reason = \"OK\"

[[response.headers]]
key = \"content-type\"
value = \"application/json\"

[[response.headers]]
key = \"x-echo\"
value = \"token {MASK}\"

[[response.headers]]
key = \"set-cookie\"
value = \"sid={MASK}; Path=/\"

[response.body]
text = '{{\"token\": \"{MASK}\"}}'
"
        )
    );
    let listed = engine.examples("api/r").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].slug, "ok");
    assert_eq!(listed[0].example.id, id);
    assert_eq!(
        listed[0].body,
        format!("{{\"token\": \"{MASK}\"}}").into_bytes()
    );
    let reopened = common::open_engine(root).unwrap();
    assert!(reopened.warnings().is_empty(), "{:?}", reopened.warnings());
}

#[test]
fn a_binary_body_is_kept_as_received_in_a_body_file() {
    let dir = temp_dir();
    let root = dir.path();
    let engine = engine(root);
    let bytes = [0xff, 0xd8, 0x00, b's', b'3'];

    save(&engine, "Image", &bytes);

    let example = read(root, "collections/api/r.examples/image.example.toml");
    assert!(
        example.ends_with("[response.body]\nfile = \"image.body\"\n"),
        "{example}"
    );
    assert_eq!(
        std::fs::read(root.join("collections/api/r.examples/image.body")).unwrap(),
        bytes
    );
    assert_eq!(engine.examples("api/r").unwrap()[0].body, bytes);
}

#[test]
fn a_taken_name_is_numbered_and_examples_list_by_name() {
    let dir = temp_dir();
    let engine = engine(dir.path());

    save(&engine, "OK", b"a");
    save(&engine, "OK", b"b");
    save(&engine, "Error", b"");

    let listed: Vec<(String, String)> = engine
        .examples("api/r")
        .unwrap()
        .into_iter()
        .map(|saved| (saved.example.name, saved.slug))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("Error".to_owned(), "error".to_owned()),
            ("OK".to_owned(), "ok".to_owned()),
            ("OK 2".to_owned(), "ok-2".to_owned()),
        ]
    );
}

#[test]
fn rename_moves_the_example_and_its_body_file_and_keeps_the_id() {
    let dir = temp_dir();
    let root = dir.path();
    let engine = engine(root);
    let id = save(&engine, "Image", &[0xff, 0x00]);

    engine.rename_example("api/r", id, "Logo").unwrap();

    let examples = root.join("collections/api/r.examples");
    assert!(!examples.join("image.example.toml").exists());
    assert!(!examples.join("image.body").exists());
    assert_eq!(
        std::fs::read(examples.join("logo.body")).unwrap(),
        [0xff, 0x00]
    );
    let listed = engine.examples("api/r").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].example.id, id);
    assert_eq!(listed[0].example.name, "Logo");
    assert_eq!(listed[0].body, [0xff, 0x00]);
}

#[test]
fn delete_removes_the_files_and_the_empty_directory() {
    let dir = temp_dir();
    let root = dir.path();
    let engine = engine(root);
    let id = save(&engine, "Image", &[0xff]);

    engine.delete_example("api/r", id).unwrap();

    assert!(!root.join("collections/api/r.examples").exists());
    assert!(engine.examples("api/r").unwrap().is_empty());
    let error = engine.delete_example("api/r", id).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("the request has no saved example with the id {id}")
    );
}

#[test]
fn an_unknown_request_is_an_error_and_writes_nothing() {
    let dir = temp_dir();
    let root = dir.path();
    let engine = engine(root);
    let before = common::snapshot(root);

    let error = engine.examples("api/missing").unwrap_err();

    assert!(matches!(error, ExampleError::Request(_)), "{error:?}");
    assert_eq!(common::snapshot(root), before);
}

#[test]
fn a_response_body_is_saved_to_a_file_as_received() {
    let dir = temp_dir();
    let engine = engine(dir.path());
    let out = temp_dir();
    let path = out.path().join("body.bin");

    engine
        .save_response_body(&response(&[0xff, b's', b'3', b'c']), &path)
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), [0xff, b's', b'3', b'c']);
    let error = engine
        .save_response_body(&response(b"x"), out.path())
        .unwrap_err();
    assert!(
        error.to_string().starts_with(&format!(
            "could not write the response body to {}: ",
            out.path().display()
        )),
        "{error}"
    );
}
