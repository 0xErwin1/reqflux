//! Writing an export to a file (v1-features S6, v1-close S4): the rule `reqflux export`,
//! `reqflux bundle` and the window's Export… share. A new file unless forced, readable by its
//! owner only when it holds secret values in clear.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::fs;

use pretty_assertions::assert_eq;
use reqflux_engine::{ExportFileError, write_export_file};

use common::temp_dir;

fn read(path: &std::path::Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read {}: {error}", path.display()),
    }
}

#[test]
fn a_new_file_is_written_and_an_existing_one_is_kept_unless_forced() {
    let dir = temp_dir();
    let file = dir.path().join("api.postman_collection.json");
    if let Err(error) = write_export_file(&file, "first\n", false, false) {
        panic!("write: {error}");
    }
    assert_eq!(read(&file), "first\n");

    match write_export_file(&file, "second\n", false, false) {
        Err(ExportFileError::Exists { path }) => assert_eq!(path, file),
        other => panic!("expected Exists, got {other:?}"),
    }
    assert_eq!(read(&file), "first\n");
    let refused = ExportFileError::Exists { path: file.clone() };
    assert_eq!(
        refused.to_string(),
        format!("{} already exists", file.display())
    );

    if let Err(error) = write_export_file(&file, "second\n", true, false) {
        panic!("forced write: {error}");
    }
    assert_eq!(read(&file), "second\n");
}

#[test]
fn a_missing_directory_says_which_file_could_not_be_written() {
    let dir = temp_dir();
    let file = dir.path().join("missing").join("out.json");
    match write_export_file(&file, "x", false, false) {
        Err(error @ ExportFileError::Write { .. }) => {
            let text = error.to_string();
            assert!(
                text.starts_with(&format!("could not write {}: ", file.display())),
                "{text}"
            );
        }
        other => panic!("expected Write, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_file_with_secrets_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = temp_dir();
    let file = dir.path().join("secrets.json");
    if let Err(error) = write_export_file(&file, "{}\n", false, true) {
        panic!("write: {error}");
    }
    let mode = match fs::metadata(&file) {
        Ok(metadata) => metadata.permissions().mode() & 0o777,
        Err(error) => panic!("metadata: {error}"),
    };
    assert_eq!(mode, 0o600);
}
