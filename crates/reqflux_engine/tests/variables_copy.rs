//! Copying scope variables into another scope through the [`Engine`] (feedback-nov3 S13).
//! Every test uses the in-memory secret store.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use common::{id, id_text, snapshot, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::Variable;
use reqflux_engine::{
    CopyPolicy, Engine, MemorySecretStore, SecretKey, SecretScope, SecretStore, SkipReason,
    SkippedVariable, VariableEdit,
};

const ENV: &str = "environments/dev.env.toml";

fn workspace(root: &Path) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://a.test\"\ndescription = \"Root\"\n\n\
             [[variables]]\nkey = \"token\"\nvalue = \"from-collection\"\n\n\
             [[variables]]\nkey = \"off\"\nvalue = \"x\"\nenabled = false\n\n\
             [[variables]]\nkey = \"pass\"\nsecret = true\n",
            id_text(1)
        ),
    );
    write(
        root,
        ENV,
        &format!(
            "id = \"{}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"from-env\"\n",
            id_text(3)
        ),
    );
}

fn open(root: &Path, store: Arc<MemorySecretStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn read(root: &Path, relative: &str) -> String {
    match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("read {relative}: {error}"),
    }
}

fn collection() -> SecretScope {
    SecretScope::Collection(id(1))
}

fn env() -> SecretScope {
    SecretScope::Environment(id(3))
}

fn key(scope: SecretScope, name: &str) -> SecretKey {
    SecretKey {
        workspace_id: id(900),
        scope,
        key: name.to_owned(),
    }
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn env_variables(engine: &Engine) -> Vec<Variable> {
    engine.workspace().environments[0]
        .environment
        .variables
        .clone()
}

#[test]
fn copying_adds_the_initial_value_description_and_enabled_flag() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let report = match engine.copy_variables(
        collection(),
        &keys(&["base", "off"]),
        env(),
        CopyPolicy::Skip,
    ) {
        Ok(report) => report,
        Err(error) => panic!("copy: {error}"),
    };
    assert_eq!(report.copied, keys(&["base", "off"]));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let variables = env_variables(&engine);
    assert_eq!(variables.len(), 3);
    assert_eq!(variables[1].key, "base");
    assert_eq!(variables[1].value.as_deref(), Some("http://a.test"));
    assert_eq!(variables[1].description.as_deref(), Some("Root"));
    assert!(variables[1].enabled);
    assert_eq!(variables[2].key, "off");
    assert!(!variables[2].enabled);
    let text = read(root, ENV);
    assert!(text.contains("value = \"http://a.test\""), "{text}");
}

#[test]
fn a_key_that_exists_is_skipped_by_default_and_changes_nothing() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let before = snapshot(root);
    let report =
        match engine.copy_variables(collection(), &keys(&["token"]), env(), CopyPolicy::Skip) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert!(report.copied.is_empty());
    assert_eq!(
        report.skipped,
        [SkippedVariable {
            key: "token".to_owned(),
            reason: SkipReason::Exists,
        }]
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn overwrite_replaces_the_value_of_a_key_that_exists_in_place() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let report = match engine.copy_variables(
        collection(),
        &keys(&["token", "base"]),
        env(),
        CopyPolicy::Overwrite,
    ) {
        Ok(report) => report,
        Err(error) => panic!("copy: {error}"),
    };
    assert_eq!(report.copied, keys(&["token", "base"]));
    let variables = env_variables(&engine);
    assert_eq!(variables[0].key, "token");
    assert_eq!(variables[0].value.as_deref(), Some("from-collection"));
    assert_eq!(variables[1].key, "base");
}

#[test]
fn a_secret_copies_its_keyring_value_and_never_reaches_the_file() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    if let Err(error) = store.set(&key(collection(), "pass"), "s3cret") {
        panic!("seed: {error}");
    }
    let mut engine = open(root, store.clone());
    let report =
        match engine.copy_variables(collection(), &keys(&["pass"]), env(), CopyPolicy::Skip) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert_eq!(report.copied, keys(&["pass"]));
    let variables = env_variables(&engine);
    assert!(variables[1].secret);
    assert_eq!(variables[1].value, None);
    assert_eq!(
        store.get(&key(env(), "pass")).ok().flatten().as_deref(),
        Some("s3cret")
    );
    assert!(!read(root, ENV).contains("s3cret"));
    assert!(!format!("{report:?}").contains("s3cret"));
}

#[test]
fn a_keyring_failure_skips_the_secret_and_never_copies_it_empty() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::unavailable("locked")));
    let before = snapshot(root);
    let report = match engine.copy_variables(
        collection(),
        &keys(&["pass", "base"]),
        env(),
        CopyPolicy::Skip,
    ) {
        Ok(report) => report,
        Err(error) => panic!("copy: {error}"),
    };
    assert_eq!(report.copied, keys(&["base"]));
    assert_eq!(
        report.skipped,
        [SkippedVariable {
            key: "pass".to_owned(),
            reason: SkipReason::Keyring,
        }]
    );
    assert!(env_variables(&engine).iter().all(|v| v.key != "pass"));
    assert_ne!(snapshot(root), before);
    assert!(!read(root, ENV).contains("pass"));
}

#[test]
fn a_key_the_source_does_not_declare_is_skipped() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let report =
        match engine.copy_variables(collection(), &keys(&["nope"]), env(), CopyPolicy::Skip) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert_eq!(
        report.skipped,
        [SkippedVariable {
            key: "nope".to_owned(),
            reason: SkipReason::Missing,
        }]
    );
}

#[test]
fn globals_copy_like_a_collection() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    if let Err(error) = engine.add_variable(SecretScope::Global, Variable::new("host", "h.test")) {
        panic!("add: {error}");
    }
    if let Err(error) = engine.edit_variable(
        SecretScope::Global,
        0,
        VariableEdit::Description(Some("Host".to_owned())),
    ) {
        panic!("edit: {error}");
    }
    let report = match engine.copy_variables(
        SecretScope::Global,
        &keys(&["host"]),
        env(),
        CopyPolicy::Skip,
    ) {
        Ok(report) => report,
        Err(error) => panic!("copy: {error}"),
    };
    assert_eq!(report.copied, keys(&["host"]));
    assert_eq!(
        env_variables(&engine)[1].description.as_deref(),
        Some("Host")
    );
}
