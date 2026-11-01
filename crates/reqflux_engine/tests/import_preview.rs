//! Previewing a Postman import: what it would write, with nothing written (app-pending S6).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::fs;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use reqflux_engine::{ImportError, postman, postman_preview};

const BASIC: &str = include_str!("../../reqflux_postman/tests/fixtures/collection-basic.json");
const STAGING: &str = include_str!("../../reqflux_postman/tests/fixtures/environment.json");

fn document(text: &str) -> serde_json::Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("fixture is not valid JSON: {error}"),
    }
}

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

/// Every file under `root`, relative to it, sorted.
fn files(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
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
            } else if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn the_preview_lists_what_the_import_writes_and_writes_nothing() {
    let previewed = temp();
    let preview = match postman_preview(previewed.path(), &document(BASIC), &[document(STAGING)]) {
        Ok(preview) => preview,
        Err(error) => panic!("preview: {error}"),
    };
    assert!(
        files(previewed.path()).is_empty(),
        "a preview writes nothing"
    );
    assert_eq!(preview.collection, "Basic Collection");
    assert_eq!(preview.folders, 1);
    assert_eq!(preview.requests, 3);
    assert_eq!(preview.scripts, 2);
    assert_eq!(preview.environments, 1);
    assert_eq!(
        preview.files.first().map(PathBuf::as_path),
        Some(Path::new("reqflux.toml")),
        "an empty directory gets a manifest first"
    );

    let imported = temp();
    if let Err(error) = postman(imported.path(), &document(BASIC), &[document(STAGING)]) {
        panic!("import: {error}");
    }
    let mut listed = preview.files.clone();
    listed.sort();
    assert_eq!(listed, files(imported.path()));
}

#[test]
fn an_existing_workspace_keeps_its_manifest_out_of_the_preview() {
    let dir = temp();
    if let Err(error) = fs::write(
        dir.path().join("reqflux.toml"),
        "id = \"01J00000000000000000000A01\"\nname = \"Billing\"\nformat = 1\n",
    ) {
        panic!("write manifest: {error}");
    }
    let preview = match postman_preview(dir.path(), &document(BASIC), &[]) {
        Ok(preview) => preview,
        Err(error) => panic!("preview: {error}"),
    };
    assert_eq!(preview.environments, 0);
    assert!(
        !preview.files.contains(&PathBuf::from("reqflux.toml")),
        "{:?}",
        preview.files
    );
    assert_eq!(
        preview.files.first().map(PathBuf::as_path),
        Some(Path::new("collections/basic-collection/collection.toml"))
    );
}

#[test]
fn a_collision_fails_the_preview_as_it_fails_the_import() {
    let dir = temp();
    if let Err(error) = postman(dir.path(), &document(BASIC), &[]) {
        panic!("first import: {error}");
    }
    let before = files(dir.path());
    match postman_preview(dir.path(), &document(BASIC), &[]) {
        Err(ImportError::Exists { path }) => {
            assert!(
                path.ends_with("collections/basic-collection/collection.toml"),
                "{path:?}"
            );
        }
        other => panic!("expected Exists, got {other:?}"),
    }
    assert_eq!(files(dir.path()), before);
}

#[test]
fn a_document_that_does_not_map_fails_the_preview() {
    let dir = temp();
    let not_postman = serde_json::json!({ "hello": "world" });
    assert!(matches!(
        postman_preview(dir.path(), &not_postman, &[]),
        Err(ImportError::Postman(_))
    ));
}
