//! Descriptions (feedback-nov3 S11): the import fills them, the engine edits them, and the
//! generated docs show them, from the field or from the text an earlier import kept.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;

use reqflux_core::model::Item;
use reqflux_engine::{Engine, postman};
use serde_json::{Value, json};

use common::{open_engine, temp_dir};

const DOCS: &str = include_str!("../../reqflux_postman/tests/fixtures/collection-docs.json");

fn open(root: &std::path::Path) -> Engine {
    match open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn imported() -> (tempfile::TempDir, Engine) {
    let dir = temp_dir();
    let document: Value = match serde_json::from_str(DOCS) {
        Ok(value) => value,
        Err(error) => panic!("fixture: {error}"),
    };
    if let Err(error) = postman(dir.path(), &document, &[]) {
        panic!("import: {error}");
    }
    let engine = open(dir.path());
    (dir, engine)
}

fn markdown(engine: &Engine) -> String {
    match engine.docs("documented") {
        Ok(docs) => docs
            .files
            .iter()
            .find(|file| file.name == "README.md")
            .map(|file| file.content.clone())
            .unwrap_or_default(),
        Err(error) => panic!("docs: {error}"),
    }
}

#[test]
fn the_import_stores_the_descriptions_and_the_docs_show_them() {
    let (_dir, engine) = imported();

    let node = &engine.workspace().collections[0];
    assert_eq!(
        node.collection.description.as_deref(),
        Some("# Documented API\n\nStart with the **Users** folder.")
    );
    let text = markdown(&engine);
    assert!(text.contains("Start with the **Users** folder."), "{text}");
    assert!(text.contains("Everything about users."), "{text}");
    assert!(text.contains("Lists users, paged."), "{text}");
}

#[test]
fn set_description_edits_a_collection_a_folder_and_a_request_and_clears_on_blank() {
    let (_dir, mut engine) = imported();
    let collection = engine.workspace().collections[0].collection.id;
    let Item::Folder(folder) = &engine.workspace().collections[0].items[0] else {
        panic!("the first item is the Users folder");
    };
    let folder_id = folder.folder.id;
    let Item::Request(request) = &folder.items[0] else {
        panic!("the first child is a request");
    };
    let request_id = request.request.id;

    for (id, text) in [
        (collection, "Collection notes"),
        (folder_id, "Folder\n\nnotes"),
        (request_id, "Request notes"),
    ] {
        assert!(engine.set_description(id, text).is_ok());
    }

    let node = &engine.workspace().collections[0];
    assert_eq!(
        node.collection.description.as_deref(),
        Some("Collection notes")
    );
    let Item::Folder(folder) = &node.items[0] else {
        panic!("folder");
    };
    assert_eq!(
        folder.folder.description.as_deref(),
        Some("Folder\n\nnotes")
    );
    let Item::Request(request) = &folder.items[0] else {
        panic!("request");
    };
    assert_eq!(
        request.request.description.as_deref(),
        Some("Request notes")
    );
    assert!(markdown(&engine).contains("Folder\n\nnotes"));

    assert!(engine.set_description(request_id, " ").is_ok());
    let Item::Folder(folder) = &engine.workspace().collections[0].items[0] else {
        panic!("folder");
    };
    let Item::Request(request) = &folder.items[0] else {
        panic!("request");
    };
    assert_eq!(request.request.description, None);
    assert_eq!(request.request.description_text(), None);
}

#[test]
fn a_description_an_earlier_import_kept_still_shows_in_the_docs() {
    let (dir, engine) = imported();
    let Item::Request(second) = &engine.workspace().collections[0].items[1] else {
        panic!("the second item is a request");
    };
    let id = second.request.id;
    let file = dir
        .path()
        .join("collections/documented/undocumented.req.toml");
    let before = fs::read_to_string(&file).unwrap_or_default();
    let kept =
        json!({ "description": { "content": "Kept by the old import", "type": "text/markdown" } });
    let text = format!("{before}\n[extensions]\npostman = '{kept}'\n");
    if let Err(error) = fs::write(&file, &text) {
        panic!("write: {error}");
    }

    let engine = open(dir.path());

    assert!(markdown(&engine).contains("Kept by the old import"));
    assert_eq!(fs::read_to_string(&file).unwrap_or_default(), text);
    let mut engine = engine;
    assert!(engine.set_description(id, "Mine").is_ok());
    let written = fs::read_to_string(&file).unwrap_or_default();
    assert!(!written.contains("Kept by the old import"), "{written}");
    assert!(markdown(&engine).contains("Mine"));
}
