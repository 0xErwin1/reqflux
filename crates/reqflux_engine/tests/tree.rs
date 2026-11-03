//! Tree operations through the [`Engine`]: each one calls the store and reloads (S8).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use common::{id, id_text, snapshot, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Id, Item, Method, Request};
use reqflux_engine::state::{AppState, Draft};
use reqflux_engine::{Engine, TreeError};
use reqflux_store::Parent;

/// A workspace with collection `c` (id 1) holding requests `a` (id 2) and `b` (id 3) and an
/// empty folder `f` (id 4), in that order.
fn workspace(root: &Path) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/c/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"C\"\norder = [\"a\", \"b\", \"f\"]\n",
            id_text(1)
        ),
    );
    write(
        root,
        "collections/c/f/folder.toml",
        &format!("id = \"{}\"\nname = \"F\"\n", id_text(4)),
    );
    for (n, slug) in [(2, "a"), (3, "b")] {
        let request = Request::new(id(n), slug.to_uppercase(), Method::GET, "https://x.test");
        if let Err(error) =
            reqflux_store::save_request(root, &Parent::collection("c"), slug, &request, None)
        {
            panic!("create request {slug}: {error}");
        }
    }
}

fn open(root: &Path) -> Engine {
    match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn ok<T>(result: Result<T, TreeError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("tree operation: {error}"),
    }
}

/// The ids of the collection's children, in display order.
fn children(engine: &Engine) -> Vec<Id> {
    engine.workspace().collections[0]
        .items
        .iter()
        .map(item_id)
        .collect()
}

fn item_id(item: &Item) -> Id {
    match item {
        Item::Folder(node) => node.folder.id,
        Item::Request(node) => node.request.id,
    }
}

/// The request `id` anywhere in the workspace.
fn request(engine: &Engine, id: Id) -> Option<Request> {
    fn walk(items: &[Item], id: Id) -> Option<Request> {
        items.iter().find_map(|item| match item {
            Item::Request(node) if node.request.id == id => Some(node.request.clone()),
            Item::Request(_) => None,
            Item::Folder(node) => walk(&node.items, id),
        })
    }
    engine
        .workspace()
        .collections
        .iter()
        .find_map(|collection| walk(&collection.items, id))
}

/// The children ids of folder `f` (id 4).
fn folder_children(engine: &Engine) -> Vec<Id> {
    match engine.workspace().collections[0]
        .items
        .iter()
        .find(|item| item_id(item) == id(4))
    {
        Some(Item::Folder(node)) => node.items.iter().map(item_id).collect(),
        _ => panic!("folder f is missing"),
    }
}

#[test]
fn create_collection_is_visible_after_reload() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let created = ok(engine.create_collection());
    let names: Vec<_> = engine
        .workspace()
        .collections
        .iter()
        .map(|node| (node.collection.id, node.collection.name.clone()))
        .collect();
    assert!(names.contains(&(created, "New collection".to_owned())));
}

#[test]
fn create_request_is_a_named_get_at_the_end() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let created = ok(engine.create_request(id(1)));
    assert_eq!(children(&engine), vec![id(2), id(3), id(4), created]);
    let Some(request) = request(&engine, created) else {
        panic!("new request not loaded");
    };
    assert_eq!(request.name, "New request");
    assert_eq!(request.method, Method::GET);
}

#[test]
fn create_request_as_writes_the_document_under_its_name() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let document = Request::new(id(7), "Create invoice", Method::POST, "https://x.test/v1");
    let created = ok(engine.create_request_as(id(4), document));
    assert_eq!(folder_children(&engine), vec![created]);
    let Some(saved) = request(&engine, created) else {
        panic!("created request not loaded");
    };
    assert_eq!(saved.name, "Create invoice");
    assert_eq!(saved.method, Method::POST);
    assert_eq!(saved.url, "https://x.test/v1");
    assert!(
        dir.path()
            .join("collections/c/f/create-invoice.req.toml")
            .exists(),
        "the file sits under the name's slug"
    );
}

#[test]
fn create_folder_is_visible_after_reload() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let created = ok(engine.create_folder(id(1)));
    assert_eq!(children(&engine), vec![id(2), id(3), id(4), created]);
}

#[test]
fn rename_changes_the_name_and_keeps_the_id() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    assert_eq!(ok(engine.rename(id(2), "Renamed")), id(2));
    let Some(request) = request(&engine, id(2)) else {
        panic!("renamed request not loaded");
    };
    assert_eq!(request.name, "Renamed");
    assert_eq!(children(&engine), vec![id(2), id(3), id(4)]);
}

#[test]
fn duplicate_returns_a_new_id_after_the_original() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let copy = ok(engine.duplicate(id(2)));
    assert!(copy != id(2));
    assert_eq!(children(&engine), vec![id(2), copy, id(3), id(4)]);
}

#[test]
fn delete_removes_the_item() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    ok(engine.delete(id(3)));
    assert_eq!(children(&engine), vec![id(2), id(4)]);
    assert_eq!(request(&engine, id(3)), None);
}

#[test]
fn move_item_changes_the_parent() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    assert_eq!(ok(engine.move_item(id(2), id(4), 0)), id(2));
    assert_eq!(children(&engine), vec![id(3), id(4)]);
    assert_eq!(folder_children(&engine), vec![id(2)]);
}

#[test]
fn reorder_changes_the_order() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    assert_eq!(ok(engine.reorder(id(4), 0)), id(4));
    assert_eq!(children(&engine), vec![id(4), id(2), id(3)]);
}

#[test]
fn a_failed_operation_leaves_the_workspace_unchanged() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    let before = engine.workspace().clone();
    let files = snapshot(dir.path());
    let Err(error) = engine.rename(id(2), "B") else {
        panic!("a taken slug must fail");
    };
    assert!(matches!(error, TreeError::Store(_)), "{error:?}");
    assert!(!error.to_string().is_empty());
    assert_eq!(engine.workspace(), &before);
    assert_eq!(snapshot(dir.path()), files);
}

#[test]
fn a_draft_keyed_by_id_survives_rename_and_move() {
    let dir = temp_dir();
    workspace(dir.path());
    let state_dir = temp_dir();
    let mut state = match AppState::open(&state_dir.path().join("state.sqlite3")) {
        Ok(state) => state,
        Err(error) => panic!("state: {error}"),
    };
    let workspace_id = id_text(900);
    let entity = id(2).to_string();
    if let Err(error) = state.put_draft(&workspace_id, &entity, Draft::new("name = \"x\"", None)) {
        panic!("put draft: {error}");
    }
    let mut engine = open(dir.path());
    ok(engine.rename(id(2), "Renamed"));
    ok(engine.move_item(id(2), id(4), 0));
    let draft = match state.draft(&workspace_id, &entity) {
        Ok(draft) => draft,
        Err(error) => panic!("read draft: {error}"),
    };
    assert_eq!(
        draft.map(|draft| draft.payload),
        Some("name = \"x\"".to_owned())
    );
    let selector = id(2).to_string();
    assert!(matches!(engine.request_base_hash(&selector), Ok(Some(_))));
    assert_eq!(folder_children(&engine), vec![id(2)]);
}

#[test]
fn save_request_works_after_a_tree_operation() {
    let dir = temp_dir();
    workspace(dir.path());
    let mut engine = open(dir.path());
    ok(engine.rename(id(2), "Renamed"));
    let selector = id(2).to_string();
    let base = match engine.request_base_hash(&selector) {
        Ok(base) => base,
        Err(error) => panic!("base hash: {error}"),
    };
    let edited = Request::new(id(2), "Renamed", Method::POST, "https://x.test/edited");
    if let Err(error) = engine.save_request(&selector, &edited, base) {
        panic!("save: {error}");
    }
    if let Err(error) = engine.reload() {
        panic!("reload: {error}");
    }
    let Some(request) = request(&engine, id(2)) else {
        panic!("saved request not loaded");
    };
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.url, "https://x.test/edited");
}
