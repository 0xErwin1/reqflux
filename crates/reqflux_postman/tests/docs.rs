//! Descriptions (feedback-nov3 S11): the import reads the collection's, the folders' and the
//! requests' markdown into `description`, and the export writes it back where Postman reads it.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use reqflux_core::model::{FolderNode, Item, RequestNode};
use reqflux_postman::{Imported, collection, export_collection};
use serde_json::{Value, json};

const DOCS: &str = include_str!("fixtures/collection-docs.json");

fn document() -> Value {
    match serde_json::from_str(DOCS) {
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

fn folder(imported: &Imported, index: usize) -> &FolderNode {
    match &imported.collection.items[index] {
        Item::Folder(node) => node,
        Item::Request(_) => panic!("item {index} is a request"),
    }
}

fn request(items: &[Item], index: usize) -> &RequestNode {
    match &items[index] {
        Item::Request(node) => node,
        Item::Folder(_) => panic!("item {index} is a folder"),
    }
}

#[test]
fn the_import_reads_the_collection_folder_and_request_descriptions() {
    let imported = import(&document());

    assert_eq!(
        imported.collection.collection.description.as_deref(),
        Some("# Documented API\n\nStart with the **Users** folder.")
    );
    let users = folder(&imported, 0);
    assert_eq!(
        users.folder.description.as_deref(),
        Some("Everything about users.\n\n- list\n- create")
    );
    assert_eq!(
        request(&users.items, 0).request.description.as_deref(),
        Some("Lists users, paged.")
    );
    assert_eq!(
        request(&users.items, 1).request.description.as_deref(),
        Some("Described on the item, not the request.")
    );
}

#[test]
fn a_description_the_import_read_is_not_kept_twice_in_extensions() {
    let imported = import(&document());

    let users = folder(&imported, 0);
    assert!(users.folder.extensions.is_empty(), "{:?}", users.folder);
    assert!(request(&users.items, 0).request.extensions.is_empty());
    assert!(request(&users.items, 1).request.extensions.is_empty());
}

#[test]
fn a_request_without_a_description_or_with_a_blank_one_has_none() {
    let imported = import(&document());

    let items = &imported.collection.items;
    assert_eq!(request(items, 1).request.description, None);
    assert_eq!(request(items, 2).request.description, None);
    // The blank text is not lost: it stays in the kept Postman text.
    assert!(!request(items, 2).request.extensions.is_empty());
}

#[test]
fn the_export_writes_the_descriptions_where_postman_reads_them() {
    let imported = import(&document());

    let exported = export_collection(&imported.collection, &imported.examples);

    assert_eq!(
        exported["info"]["description"],
        "# Documented API\n\nStart with the **Users** folder."
    );
    assert_eq!(
        exported["item"][0]["description"],
        "Everything about users.\n\n- list\n- create"
    );
    let list = &exported["item"][0]["item"][0];
    assert_eq!(list["request"]["description"], "Lists users, paged.");
    assert_eq!(
        exported["item"][0]["item"][1]["request"]["description"],
        "Described on the item, not the request."
    );
    assert_eq!(exported["item"][1]["request"].get("description"), None);
    assert_eq!(exported["item"][1].get("description"), None);
}

#[test]
fn import_export_import_keeps_every_description() {
    let first = import(&document());
    let second = import(&export_collection(&first.collection, &first.examples));

    assert_eq!(
        second.collection.collection.description,
        first.collection.collection.description
    );
    let (before, after) = (folder(&first, 0), folder(&second, 0));
    assert_eq!(after.folder.description, before.folder.description);
    for index in 0..2 {
        assert_eq!(
            request(&after.items, index).request.description,
            request(&before.items, index).request.description
        );
    }
    let exported = export_collection(&second.collection, &second.examples);
    assert_eq!(exported["info"]["name"], json!("Documented"));
}
