//! Saving the edited sidecar scripts of a request through the [`Engine`]: hash-checked
//! against the sidecars as loaded, an empty script deletes its sidecar.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;
use std::path::Path;

use common::{id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Item, Scripts};
use reqflux_engine::{Engine, SaveError};

const REQUEST: &str = "collections/c/r.req.toml";
const PRE: &str = "collections/c/r.pre.js";
const TEST: &str = "collections/c/r.test.js";

/// Writes a workspace whose collection `c` holds request `c/r`, with the sidecars given.
fn workspace(root: &Path, pre: Option<&str>, test: Option<&str>) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/c/collection.toml",
        &format!("id = \"{}\"\nname = \"C\"\n", id_text(1)),
    );
    write(
        root,
        REQUEST,
        &format!(
            "id = \"{}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"https://h/\"\n",
            id_text(2)
        ),
    );
    if let Some(text) = pre {
        write(root, PRE, text);
    }
    if let Some(text) = test {
        write(root, TEST, text);
    }
}

fn open(root: &Path) -> Engine {
    match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

/// The scripts of `c/r` as the engine loaded them.
fn loaded_scripts(engine: &Engine) -> Scripts {
    match &engine.workspace().collections[0].items[0] {
        Item::Request(node) => node.scripts.clone(),
        Item::Folder(_) => panic!("expected a request"),
    }
}

fn scripts(pre: Option<&str>, test: Option<&str>) -> Scripts {
    Scripts {
        pre_request: pre.map(str::to_owned),
        test: test.map(str::to_owned),
    }
}

#[test]
fn base_hashes_name_the_sidecars_as_loaded() {
    let dir = temp_dir();
    workspace(dir.path(), Some("pre();\n"), None);
    let engine = open(dir.path());

    let base = match engine.script_base_hashes(&id_text(2)) {
        Ok(base) => base,
        Err(error) => panic!("base: {error}"),
    };
    assert!(base.pre_request.is_some(), "the pre-request sidecar exists");
    assert_eq!(base.test, None, "no test sidecar");
    assert!(engine.script_base_hashes("c/missing").is_err());
}

#[test]
fn a_save_writes_a_script_and_deletes_the_sidecar_of_an_empty_one() {
    let dir = temp_dir();
    workspace(dir.path(), Some("old();\n"), Some("test();\n"));
    let mut engine = open(dir.path());
    let base = match engine.script_base_hashes("c/r") {
        Ok(base) => base,
        Err(error) => panic!("base: {error}"),
    };

    let saved = engine.save_scripts("c/r", &scripts(Some("new();\n"), Some("")), base);
    let hashes = match saved {
        Ok(hashes) => hashes,
        Err(error) => panic!("save: {error}"),
    };
    assert!(hashes.pre_request.is_some());
    assert_eq!(hashes.test, None, "the empty script has no file");
    assert_eq!(
        fs::read_to_string(dir.path().join(PRE)).unwrap_or_default(),
        "new();\n"
    );
    assert!(!dir.path().join(TEST).exists(), "the empty sidecar is gone");

    if let Err(error) = engine.reload() {
        panic!("reload: {error}");
    }
    assert_eq!(loaded_scripts(&engine), scripts(Some("new();\n"), None));
}

#[test]
fn a_sidecar_changed_since_the_load_is_a_conflict_that_writes_nothing() {
    let dir = temp_dir();
    workspace(dir.path(), None, Some("test();\n"));
    let engine = open(dir.path());
    let base = match engine.script_base_hashes("c/r") {
        Ok(base) => base,
        Err(error) => panic!("base: {error}"),
    };
    write(dir.path(), TEST, "other();\n");

    let result = engine.save_scripts("c/r", &scripts(Some("pre();\n"), Some("mine();\n")), base);
    match result {
        Err(SaveError::Conflict { path }) => assert!(path.ends_with("r.test.js"), "{path:?}"),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(
        !dir.path().join(PRE).exists(),
        "neither sidecar was written"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(TEST)).unwrap_or_default(),
        "other();\n"
    );
}

#[test]
fn an_unknown_request_saves_nothing() {
    let dir = temp_dir();
    workspace(dir.path(), None, None);
    let engine = open(dir.path());
    let base = match engine.script_base_hashes("c/r") {
        Ok(base) => base,
        Err(error) => panic!("base: {error}"),
    };

    let result = engine.save_scripts("c/missing", &scripts(Some("x();\n"), None), base);
    assert!(
        matches!(result, Err(SaveError::UnknownRequest(ref selector)) if selector == "c/missing"),
        "{result:?}"
    );
    assert!(!dir.path().join("collections/c/missing.pre.js").exists());
}

#[test]
fn saving_the_request_document_leaves_the_sidecars_alone() {
    let dir = temp_dir();
    workspace(dir.path(), Some("pre();\n"), None);
    let engine = open(dir.path());
    let Item::Request(node) = &engine.workspace().collections[0].items[0] else {
        panic!("expected a request");
    };
    let mut request = node.request.clone();
    request.url = "https://h/changed".to_owned();
    let base = match engine.request_base_hash("c/r") {
        Ok(base) => base,
        Err(error) => panic!("base: {error}"),
    };
    if let Err(error) = engine.save_request("c/r", &request, base) {
        panic!("save: {error}");
    }
    assert_eq!(
        fs::read_to_string(dir.path().join(PRE)).unwrap_or_default(),
        "pre();\n"
    );
}
