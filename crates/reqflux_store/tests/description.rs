//! Descriptions (feedback-nov3 S11): written through `set_description`, multi-line text stays
//! a TOML multi-line string, and the copy an earlier Postman import kept goes with the edit.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use common::*;
use pretty_assertions::assert_eq;
use reqflux_store::StoreError;

const API: &str = "collections/api/collection.toml";
const FOLDER: &str = "collections/api/docs/folder.toml";
const REQUEST: &str = "collections/api/docs/list.req.toml";

fn fixture(root: &std::path::Path) {
    write(
        root,
        API,
        &collection_toml(1, "API", "order = [\"docs\"]\n"),
    );
    write(
        root,
        FOLDER,
        &folder_toml(2, "Docs", "order = [\"list\"]\n"),
    );
    write(root, REQUEST, &request_toml(3, "List"));
}

#[test]
fn a_description_is_written_to_each_kind_of_file_and_reads_back() {
    let dir = minimal_workspace();
    let root = dir.path();
    fixture(root);
    let loaded = load(root);

    for (n, file) in [(1, API), (2, FOLDER), (3, REQUEST)] {
        let written =
            reqflux_store::set_description(root, &loaded, id(n), "Line one.\n\nLine two.\n");
        assert!(written.is_ok(), "{file}: {written:?}");
        assert!(
            read(root, file).contains("description = \"\"\"\nLine one.\n\nLine two.\n\"\"\"\n"),
            "{file}: {}",
            read(root, file)
        );
    }

    let again = load(root);
    let collection = &again.workspace.collections[0];
    assert_eq!(
        collection.collection.description.as_deref(),
        Some("Line one.\n\nLine two.\n")
    );
}

#[test]
fn blank_text_clears_the_description_and_restores_the_original_bytes() {
    let dir = minimal_workspace();
    let root = dir.path();
    fixture(root);
    let before = read(root, API);
    let loaded = load(root);
    assert!(reqflux_store::set_description(root, &loaded, id(1), "Docs").is_ok());

    let loaded = load(root);
    assert!(reqflux_store::set_description(root, &loaded, id(1), "  \n").is_ok());

    assert_eq!(read(root, API), before);
}

#[test]
fn a_kept_postman_description_is_read_but_not_rewritten_by_a_load() {
    let dir = minimal_workspace();
    let root = dir.path();
    let text = format!(
        "{}\n[extensions]\npostman = '{{\"description\":\"From Postman\",\"id\":\"p1\"}}'\n",
        request_toml(3, "List")
    );
    write(
        root,
        API,
        &collection_toml(1, "API", "order = [\"list\"]\n"),
    );
    write(root, "collections/api/list.req.toml", &text);

    let loaded = load(root);

    let Some(reqflux_core::model::Item::Request(node)) =
        loaded.workspace.collections[0].items.first()
    else {
        panic!("no request");
    };
    assert_eq!(node.request.description, None);
    assert_eq!(
        node.request.description_text().as_deref(),
        Some("From Postman")
    );
    assert_eq!(read(root, "collections/api/list.req.toml"), text);
}

#[test]
fn editing_a_description_drops_the_postman_copy_and_keeps_the_other_kept_keys() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        API,
        &collection_toml(1, "API", "order = [\"list\"]\n"),
    );
    write(
        root,
        "collections/api/list.req.toml",
        &format!(
            "{}\n[extensions]\npostman = '{{\"description\":\"From Postman\",\"id\":\"p1\"}}'\n",
            request_toml(3, "List")
        ),
    );
    let loaded = load(root);

    assert!(reqflux_store::set_description(root, &loaded, id(3), "Mine").is_ok());

    let file = read(root, "collections/api/list.req.toml");
    assert!(file.contains("description = \"Mine\""), "{file}");
    assert!(file.contains("{\"id\":\"p1\"}"), "{file}");
    assert!(!file.contains("From Postman"), "{file}");
}

#[test]
fn a_file_changed_since_the_load_is_a_conflict_and_stays_as_it_is() {
    let dir = minimal_workspace();
    let root = dir.path();
    fixture(root);
    let loaded = load(root);
    let changed = collection_toml(1, "API renamed", "order = [\"docs\"]\n");
    write(root, API, &changed);

    let outcome = reqflux_store::set_description(root, &loaded, id(1), "Docs");

    assert!(
        matches!(outcome, Err(StoreError::Conflict { .. })),
        "{outcome:?}"
    );
    assert_eq!(read(root, API), changed);
}

#[test]
fn an_unknown_id_is_not_found() {
    let dir = minimal_workspace();
    let root = dir.path();
    fixture(root);
    let loaded = load(root);

    let outcome = reqflux_store::set_description(root, &loaded, id(99), "Docs");

    assert!(
        matches!(outcome, Err(StoreError::NotFound { .. })),
        "{outcome:?}"
    );
}
