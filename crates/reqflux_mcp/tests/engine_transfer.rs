//! `postman_export`, `bundle_export`, `bundle_import` and `docs_generate` (v1-features S6):
//! the exports are reads that return the document and never a secret value; the import is a
//! write into the open workspace. A temp config file, audit log and in-memory secret store.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use reqflux_engine::{MemorySecretStore, SecretKey, SecretScope, SecretStore};
use reqflux_mcp::engine_tools::{
    self, BundleExportArgs, BundleImportArgs, DocsArgs, EngineState, PathArgs, PostmanExportArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::Value;
use tempfile::TempDir;

const FIXTURE: &str =
    include_str!("../../reqflux_postman/tests/fixtures/collection-roundtrip.json");
const SECRET: &str = "kept-in-the-keyring";

/// A workspace imported from the round-trip fixture at `<dir>/ws`, open in a state whose
/// store holds the collection secret `api_secret`, under the config `config`.
fn opened(dir: &TempDir, config: &str) -> EngineState {
    let root = dir.path().join("ws");
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    reqflux_engine::postman(&root, &fixture, &[]).unwrap();
    let loaded =
        reqflux_engine::Engine::open_with_secrets(&root, Arc::new(MemorySecretStore::new()))
            .unwrap();
    let store = Arc::new(MemorySecretStore::new());
    store
        .set(
            &SecretKey {
                workspace_id: loaded.workspace().manifest.id,
                scope: SecretScope::Collection(loaded.workspace().collections[0].collection.id),
                key: "api_secret".to_owned(),
            },
            SECRET,
        )
        .unwrap();
    let mut state = EngineState::new(store, None, common::paths(dir.path(), Some(config)));
    ok(engine_tools::workspace_open(
        &mut state,
        PathArgs {
            path: root.display().to_string(),
        },
    ));
    state
}

fn ok(result: Result<String, ToolError>) -> Value {
    match result {
        Ok(text) => serde_json::from_str(&text).unwrap(),
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn refused(result: Result<String, ToolError>) -> String {
    match result {
        Err(ToolError::Refused(text)) => text,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn audit(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("state/reqflux/audit.log")).unwrap_or_default()
}

const READ_ONLY: &str =
    "[mcp]\nread = \"allow\"\nwrite = \"deny\"\nexecute = \"deny\"\ndestructive = \"deny\"\n";

#[test]
fn postman_export_is_a_read_that_never_carries_a_secret_value() {
    let dir = TempDir::new().unwrap();
    let mut state = opened(&dir, READ_ONLY);
    let result = ok(engine_tools::postman_export(
        &mut state,
        PostmanExportArgs {
            target: "round-trip".to_owned(),
        },
    ));
    assert_eq!(result["kind"], "collection");
    assert_eq!(result["name"], "Round Trip");
    assert_eq!(result["document"]["info"]["name"], "Round Trip");
    assert_eq!(result["empty_secrets"], serde_json::json!(["api_secret"]));
    assert!(!result.to_string().contains(SECRET), "{result}");
    assert_eq!(audit(dir.path()), "", "reads are not audited");

    // The config is read before every call: deny reads once the workspace is open.
    let denied_dir = TempDir::new().unwrap();
    let mut denied = opened(&denied_dir, READ_ONLY);
    common::write_config(
        &common::paths(denied_dir.path(), None),
        "[mcp]\nread = \"deny\"\n",
    );
    let text = refused(engine_tools::postman_export(
        &mut denied,
        PostmanExportArgs {
            target: "round-trip".to_owned(),
        },
    ));
    assert!(text.contains("postman_export"), "{text}");
}

#[test]
fn bundle_export_leaves_secrets_out_and_docs_generate_returns_the_files() {
    let dir = TempDir::new().unwrap();
    let mut state = opened(&dir, READ_ONLY);
    let bundle = ok(engine_tools::bundle_export(
        &mut state,
        BundleExportArgs { target: None },
    ));
    assert_eq!(bundle["kind"], "workspace");
    assert!(!bundle.to_string().contains(SECRET), "{bundle}");
    assert_eq!(
        bundle["secrets_omitted"],
        serde_json::json!([{ "scope": "round-trip", "key": "api_secret" }])
    );
    let text = bundle["bundle"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["format"], "reqflux-bundle");

    let docs = ok(engine_tools::docs_generate(
        &mut state,
        DocsArgs {
            collection: "round-trip".to_owned(),
        },
    ));
    assert_eq!(docs["collection"], "Round Trip");
    assert_eq!(docs["files"][0]["name"], "README.md");
    assert_eq!(docs["files"][1]["name"], "index.html");
    assert!(
        docs["files"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with("# Round Trip")
    );
}

#[test]
fn bundle_import_is_an_audited_write_into_the_open_workspace() {
    let dir = TempDir::new().unwrap();
    let mut state = opened(&dir, common::ALLOW_ALL);
    let bundle = ok(engine_tools::bundle_export(
        &mut state,
        BundleExportArgs {
            target: Some("round-trip/login".to_owned()),
        },
    ));
    let text = bundle["bundle"].as_str().unwrap().to_owned();
    let summary = ok(engine_tools::bundle_import(
        &mut state,
        BundleImportArgs {
            bundle: text.clone(),
            into: Some("round-trip/users".to_owned()),
        },
    ));
    assert_eq!(summary["kind"], "request");
    assert_eq!(summary["requests"], 1);
    assert_eq!(summary["renewed_ids"], 1);
    assert!(
        dir.path()
            .join("ws/collections/round-trip/users/login.req.toml")
            .exists()
    );
    let lines = audit(dir.path());
    assert!(lines.contains("\"tool\":\"bundle_import\""), "{lines}");
    assert!(
        !lines.contains("format_version"),
        "no argument value: {lines}"
    );

    let denied_dir = TempDir::new().unwrap();
    let mut denied = opened(&denied_dir, READ_ONLY);
    let text = refused(engine_tools::bundle_import(
        &mut denied,
        BundleImportArgs {
            bundle: text,
            into: Some("round-trip/users".to_owned()),
        },
    ));
    assert!(text.contains("bundle_import"), "{text}");
    assert!(
        !denied_dir
            .path()
            .join("ws/collections/round-trip/users/login.req.toml")
            .exists()
    );
}
