//! The native bundle (v1-features S6): one file holding a workspace, a collection, a
//! folder, a request or an environment, its secrets only age-encrypted, and its import with
//! the Postman import's rule: nothing is overwritten.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use reqflux_core::model::{Id, Item};
use reqflux_engine::{
    Bundle, BundleError, BundleImport, BundleKind, BundleSecrets, BundleSummary, Engine,
    MemorySecretStore, SecretKey, SecretScope, SecretStore, import_bundle, postman,
};
use serde_json::Value;

use common::{snapshot, temp_dir, write};

const ROUND_TRIP: &str =
    include_str!("../../reqflux_postman/tests/fixtures/collection-roundtrip.json");
const STAGING: &str = include_str!("../../reqflux_postman/tests/fixtures/environment.json");
const SECRET: &str = "kept-in-the-keyring";
const PASSPHRASE: &str = "correct horse battery staple";

fn document(text: &str) -> Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("fixture is not valid JSON: {error}"),
    }
}

/// A workspace from the round-trip fixture and the staging environment, with the collection
/// secret `api_secret`, the environment secret `password` and a global secret `token` set
/// in an in-memory store.
fn source() -> (tempfile::TempDir, Engine, Arc<MemorySecretStore>) {
    let dir = temp_dir();
    if let Err(error) = postman(dir.path(), &document(ROUND_TRIP), &[document(STAGING)]) {
        panic!("import: {error}");
    }
    let manifest = match std::fs::read_to_string(dir.path().join("reqflux.toml")) {
        Ok(text) => text,
        Err(error) => panic!("manifest: {error}"),
    };
    write(
        dir.path(),
        "reqflux.toml",
        &format!("{manifest}\n[[globals]]\nkey = \"token\"\nsecret = true\n"),
    );
    let store = Arc::new(MemorySecretStore::new());
    let engine =
        match Engine::open_with_secrets(dir.path(), Arc::clone(&store) as Arc<dyn SecretStore>) {
            Ok(engine) => engine,
            Err(error) => panic!("open: {error}"),
        };
    let workspace = engine.workspace();
    let workspace_id = workspace.manifest.id;
    for (scope, key, value) in [
        (
            SecretScope::Collection(workspace.collections[0].collection.id),
            "api_secret",
            SECRET,
        ),
        (
            SecretScope::Environment(workspace.environments[0].environment.id),
            "password",
            "env-secret",
        ),
        (SecretScope::Global, "token", "global-secret"),
    ] {
        let key = SecretKey {
            workspace_id,
            scope,
            key: key.to_owned(),
        };
        if let Err(error) = store.set(&key, value) {
            panic!("set: {error}");
        }
    }
    (dir, engine, store)
}

fn bundle(engine: &Engine, selector: Option<&str>, secrets: BundleSecrets<'_>) -> Bundle {
    match engine.bundle(selector, secrets) {
        Ok(bundle) => bundle,
        Err(error) => panic!("bundle {selector:?}: {error}"),
    }
}

fn import(
    root: &Path,
    text: &str,
    options: &BundleImport<'_>,
    store: &MemorySecretStore,
) -> BundleSummary {
    match import_bundle(root, text, options, store) {
        Ok(summary) => summary,
        Err(error) => panic!("import bundle: {error}"),
    }
}

fn import_err(
    root: &Path,
    text: &str,
    options: &BundleImport<'_>,
    store: &MemorySecretStore,
) -> BundleError {
    match import_bundle(root, text, options, store) {
        Err(error) => error,
        Ok(summary) => panic!("the import should fail: {summary:?}"),
    }
}

fn open(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

/// The files under `collections/` and `environments/`, which a bundle carries byte for byte.
fn entity_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    snapshot(root)
        .into_iter()
        .filter(|(name, _)| name.starts_with("collections/") || name.starts_with("environments/"))
        .collect()
}

#[test]
fn a_collection_bundle_imports_to_the_same_files_without_secrets() {
    let (dir, engine, _) = source();
    let made = bundle(&engine, Some("round-trip"), BundleSecrets::Omit);
    assert_eq!(made.kind, BundleKind::Collection);
    assert_eq!(made.name, "Round Trip");
    assert_eq!(made.secrets_included, 0);
    assert_eq!(
        made.secrets_omitted,
        [("round-trip".to_owned(), "api_secret".to_owned())]
    );
    assert!(!made.text.contains(SECRET), "{}", made.text);
    let parsed = document(&made.text);
    assert_eq!(parsed["format"], "reqflux-bundle");
    assert_eq!(parsed["format_version"], 1);
    assert_eq!(parsed["kind"], "collection");
    assert!(parsed.get("secrets").is_none(), "{parsed}");

    let target = temp_dir();
    let store = MemorySecretStore::new();
    let summary = import(target.path(), &made.text, &BundleImport::default(), &store);
    assert_eq!(summary.kind, BundleKind::Collection);
    assert_eq!(
        (
            summary.collections,
            summary.folders,
            summary.requests,
            summary.examples
        ),
        (1, 2, 6, 1)
    );
    assert_eq!(summary.secrets_imported, 0);
    assert!(!summary.secrets_skipped);
    let collections = |root: &Path| -> Vec<(String, Vec<u8>)> {
        entity_files(root)
            .into_iter()
            .filter(|(name, _)| name.starts_with("collections/"))
            .collect()
    };
    assert_eq!(collections(target.path()), collections(dir.path()));
    assert!(target.path().join("reqflux.toml").exists());
}

#[test]
fn encrypted_secrets_come_back_only_with_the_passphrase() {
    let (_dir, engine, _) = source();
    let made = bundle(
        &engine,
        Some("round-trip"),
        BundleSecrets::Encrypt {
            passphrase: PASSPHRASE,
        },
    );
    assert_eq!(made.secrets_included, 1);
    assert!(made.secrets_omitted.is_empty());
    assert!(!made.text.contains(SECRET), "{}", made.text);
    let parsed = document(&made.text);
    assert_eq!(parsed["secrets"]["cipher"], "age-scrypt");
    let armor = parsed["secrets"]["armor"].as_str().unwrap_or_default();
    assert!(
        armor.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"),
        "{armor}"
    );

    let wrong = temp_dir();
    let store = MemorySecretStore::new();
    let error = import_err(
        wrong.path(),
        &made.text,
        &BundleImport {
            into: None,
            passphrase: Some("not it"),
        },
        &store,
    );
    assert_eq!(
        error.to_string(),
        "the secrets do not decrypt with this passphrase"
    );
    assert_eq!(snapshot(wrong.path()), Vec::new());
    assert_eq!(store.accounts(), Vec::<String>::new());

    let skipped = temp_dir();
    let summary = import(skipped.path(), &made.text, &BundleImport::default(), &store);
    assert!(summary.secrets_skipped);
    assert_eq!(store.accounts(), Vec::<String>::new());

    let target = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let summary = import(
        target.path(),
        &made.text,
        &BundleImport {
            into: None,
            passphrase: Some(PASSPHRASE),
        },
        &store,
    );
    assert_eq!(summary.secrets_imported, 1);
    assert_eq!(summary.secret_problem, None);
    let imported = open(target.path(), Arc::clone(&store));
    let key = SecretKey {
        workspace_id: imported.workspace().manifest.id,
        scope: SecretScope::Collection(imported.workspace().collections[0].collection.id),
        key: "api_secret".to_owned(),
    };
    assert_eq!(store.get(&key), Ok(Some(SECRET.to_owned())));
}

#[test]
fn a_workspace_bundle_is_a_new_workspace_with_every_secret() {
    let (dir, engine, _) = source();
    let made = bundle(
        &engine,
        None,
        BundleSecrets::Encrypt {
            passphrase: PASSPHRASE,
        },
    );
    assert_eq!(made.kind, BundleKind::Workspace);
    assert_eq!(made.secrets_included, 3);
    for value in [SECRET, "env-secret", "global-secret"] {
        assert!(!made.text.contains(value), "{value} is in the bundle");
    }

    let target = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let summary = import(
        target.path(),
        &made.text,
        &BundleImport {
            into: None,
            passphrase: Some(PASSPHRASE),
        },
        &store,
    );
    assert_eq!((summary.collections, summary.environments), (1, 1));
    assert_eq!(summary.secrets_imported, 3);
    assert_eq!(entity_files(target.path()), entity_files(dir.path()));
    let imported = open(target.path(), Arc::clone(&store));
    let manifest = &imported.workspace().manifest;
    assert_ne!(manifest.id, engine.workspace().manifest.id);
    assert_eq!(manifest.name, engine.workspace().manifest.name);
    assert_eq!(manifest.globals, engine.workspace().manifest.globals);
    let global = SecretKey {
        workspace_id: manifest.id,
        scope: SecretScope::Global,
        key: "token".to_owned(),
    };
    assert_eq!(store.get(&global), Ok(Some("global-secret".to_owned())));

    let before = snapshot(target.path());
    let error = import_err(target.path(), &made.text, &BundleImport::default(), &store);
    assert!(matches!(error, BundleError::Exists { .. }), "{error}");
    assert_eq!(snapshot(target.path()), before);
}

#[test]
fn a_request_bundle_goes_into_a_folder_with_a_new_id_when_its_own_is_taken() {
    let (dir, engine, _) = source();
    let made = bundle(&engine, Some("round-trip/login"), BundleSecrets::Omit);
    assert_eq!(made.kind, BundleKind::Request);

    let store = MemorySecretStore::new();
    let error = import_err(dir.path(), &made.text, &BundleImport::default(), &store);
    assert_eq!(
        error.to_string(),
        "a request bundle needs a collection or folder to go into"
    );
    let before = snapshot(dir.path());
    let error = import_err(
        dir.path(),
        &made.text,
        &BundleImport {
            into: Some("round-trip"),
            passphrase: None,
        },
        &store,
    );
    assert!(matches!(error, BundleError::Exists { .. }), "{error}");
    assert_eq!(snapshot(dir.path()), before);

    let summary = import(
        dir.path(),
        &made.text,
        &BundleImport {
            into: Some("round-trip/users/nested"),
            passphrase: None,
        },
        &store,
    );
    assert_eq!(summary.requests, 1);
    assert_eq!(summary.renewed_ids, 1);
    let reloaded = open(dir.path(), Arc::new(MemorySecretStore::new()));
    let ids = |items: &[Item]| -> Vec<(String, Id)> {
        items
            .iter()
            .filter_map(|item| match item {
                Item::Request(request) => Some((request.slug.clone(), request.request.id)),
                Item::Folder(_) => None,
            })
            .collect()
    };
    let collection = &reloaded.workspace().collections[0];
    let Some(original) = ids(&collection.items)
        .into_iter()
        .find(|(slug, _)| slug == "login")
    else {
        panic!("login is still at the root");
    };
    let Item::Folder(users) = &collection.items[0] else {
        panic!("users is first");
    };
    let Some(Item::Folder(nested)) = users.items.iter().find(|item| item.slug() == "nested") else {
        panic!("nested is in users");
    };
    let copy = ids(&nested.items);
    assert_eq!(copy.len(), 2, "{copy:?}");
    assert!(
        copy.iter()
            .any(|(slug, id)| slug == "login" && *id != original.1)
    );
}

#[test]
fn an_environment_bundle_and_a_damaged_one() {
    let (_dir, engine, _) = source();
    let made = bundle(&engine, Some("env:staging"), BundleSecrets::Omit);
    assert_eq!(made.kind, BundleKind::Environment);
    assert_eq!(
        made.secrets_omitted,
        [("env:staging".to_owned(), "password".to_owned())]
    );
    let target = temp_dir();
    let store = MemorySecretStore::new();
    let summary = import(target.path(), &made.text, &BundleImport::default(), &store);
    assert_eq!(summary.environments, 1);
    assert!(target.path().join("environments/staging.env.toml").exists());

    let empty = temp_dir();
    let error = import_err(
        empty.path(),
        "{\"format\": \"other\"}",
        &BundleImport::default(),
        &store,
    );
    assert!(matches!(error, BundleError::Invalid(_)), "{error}");
    let newer = made
        .text
        .replacen("\"format_version\": 1", "\"format_version\": 7", 1);
    let error = import_err(empty.path(), &newer, &BundleImport::default(), &store);
    assert_eq!(
        error.to_string(),
        "bundle format 7 is newer than this ReqFlux reads (1)"
    );
    assert_eq!(snapshot(empty.path()), Vec::new());
}

#[test]
fn an_unreadable_store_fails_an_encrypted_bundle_and_an_empty_passphrase_is_refused() {
    let (dir, _, _) = source();
    let engine = open(
        dir.path(),
        Arc::new(MemorySecretStore::unavailable("locked")),
    );
    assert!(engine.bundle(None, BundleSecrets::Omit).is_ok());
    let error = match engine.bundle(
        None,
        BundleSecrets::Encrypt {
            passphrase: PASSPHRASE,
        },
    ) {
        Err(error) => error,
        Ok(_) => panic!("the store is unavailable"),
    };
    assert!(matches!(error, BundleError::Secret(_)), "{error}");
    let error = match engine.bundle(None, BundleSecrets::Encrypt { passphrase: "" }) {
        Err(error) => error,
        Ok(_) => panic!("an empty passphrase"),
    };
    assert_eq!(error.to_string(), "the passphrase is empty");
}
