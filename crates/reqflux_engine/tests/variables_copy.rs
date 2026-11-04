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
use std::sync::atomic::{AtomicBool, Ordering};

use common::{id, id_text, snapshot, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::Variable;
use reqflux_engine::{
    CopyPolicy, Engine, MemorySecretStore, SecretError, SecretKey, SecretScope, SecretStore,
    SkipReason, SkippedVariable, VariableEdit,
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

/// A memory store whose writes or deletes can be switched off after the setup.
#[derive(Debug, Default)]
struct FlakyStore {
    inner: MemorySecretStore,
    fail_set: AtomicBool,
    fail_delete: AtomicBool,
}

impl FlakyStore {
    fn refuse(flag: &AtomicBool) -> Result<(), SecretError> {
        if flag.load(Ordering::SeqCst) {
            return Err(SecretError::Unavailable("locked".to_owned()));
        }
        Ok(())
    }
}

impl SecretStore for FlakyStore {
    fn get_account(&self, account: &str) -> Result<Option<String>, SecretError> {
        self.inner.get_account(account)
    }

    fn set_account(&self, account: &str, value: &str) -> Result<(), SecretError> {
        Self::refuse(&self.fail_set)?;
        self.inner.set_account(account, value)
    }

    fn delete_account(&self, account: &str) -> Result<(), SecretError> {
        Self::refuse(&self.fail_delete)?;
        self.inner.delete_account(account)
    }
}

fn open_flaky(root: &Path, store: Arc<FlakyStore>) -> Engine {
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn add_secret(engine: &mut Engine, name: &str, value: &str) {
    let mut variable = Variable::new(name, value);
    variable.secret = true;
    if let Err(error) = engine.add_variable(env(), variable) {
        panic!("add secret: {error}");
    }
}

#[test]
fn overwrite_replaces_every_field_and_the_position_stays() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let mut changed = Variable::new("off", "keep");
    changed.description = Some("Old".to_owned());
    if let Err(error) = engine.add_variable(env(), changed) {
        panic!("add: {error}");
    }
    let report =
        match engine.copy_variables(collection(), &keys(&["off"]), env(), CopyPolicy::Overwrite) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert_eq!(report.copied, keys(&["off"]));
    let variables = env_variables(&engine);
    assert_eq!(variables[1].key, "off");
    assert_eq!(variables[1].value.as_deref(), Some("x"));
    assert_eq!(variables[1].description, None);
    assert!(!variables[1].enabled);
}

#[test]
fn overwrite_carries_a_secret_over_a_plain_variable() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    if let Err(error) = store.set(&key(collection(), "pass"), "s3cret") {
        panic!("seed: {error}");
    }
    let mut engine = open(root, store.clone());
    if let Err(error) = engine.add_variable(env(), Variable::new("pass", "plain")) {
        panic!("add: {error}");
    }
    let report =
        match engine.copy_variables(collection(), &keys(&["pass"]), env(), CopyPolicy::Overwrite) {
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
    let text = read(root, ENV);
    assert!(
        !text.contains("s3cret") && !text.contains("plain"),
        "{text}"
    );
}

#[test]
fn overwrite_turns_a_secret_into_a_plain_variable_and_drops_its_entry() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = open(root, store.clone());
    add_secret(&mut engine, "base", "old-secret");
    let report =
        match engine.copy_variables(collection(), &keys(&["base"]), env(), CopyPolicy::Overwrite) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert_eq!(report.copied, keys(&["base"]));
    let variables = env_variables(&engine);
    assert!(!variables[1].secret);
    assert_eq!(variables[1].value.as_deref(), Some("http://a.test"));
    assert_eq!(store.get(&key(env(), "base")).ok().flatten(), None);
}

#[test]
fn an_overwrite_cleanup_failure_still_leaves_the_whole_new_variable() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(FlakyStore::default());
    let mut engine = open_flaky(root, store.clone());
    add_secret(&mut engine, "base", "old-secret");
    store.fail_delete.store(true, Ordering::SeqCst);
    if let Err(error) =
        engine.copy_variables(collection(), &keys(&["base"]), env(), CopyPolicy::Overwrite)
    {
        panic!("copy: {error}");
    }
    let variables = env_variables(&engine);
    assert!(!variables[1].secret);
    assert_eq!(variables[1].value.as_deref(), Some("http://a.test"));
}

#[test]
fn a_keyring_failure_on_overwrite_leaves_the_old_variable_untouched() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let store = Arc::new(FlakyStore::default());
    if let Err(error) = store.set(&key(collection(), "pass"), "new-secret") {
        panic!("seed: {error}");
    }
    let mut engine = open_flaky(root, store.clone());
    add_secret(&mut engine, "pass", "old-secret");
    store.fail_set.store(true, Ordering::SeqCst);
    let before = snapshot(root);
    let report =
        match engine.copy_variables(collection(), &keys(&["pass"]), env(), CopyPolicy::Overwrite) {
            Ok(report) => report,
            Err(error) => panic!("copy: {error}"),
        };
    assert!(report.copied.is_empty());
    assert_eq!(
        report.skipped,
        [SkippedVariable {
            key: "pass".to_owned(),
            reason: SkipReason::Keyring,
        }]
    );
    assert_eq!(snapshot(root), before);
    assert_eq!(
        store.get(&key(env(), "pass")).ok().flatten().as_deref(),
        Some("old-secret")
    );
}

#[test]
fn an_unknown_destination_scope_is_an_error_even_when_no_key_reaches_it() {
    let dir = temp_dir();
    let root = dir.path();
    workspace(root);
    let mut engine = open(root, Arc::new(MemorySecretStore::new()));
    let missing = SecretScope::Environment(id(77));
    for asked in [keys(&[]), keys(&["nope"]), keys(&["base"])] {
        let result = engine.copy_variables(collection(), &asked, missing, CopyPolicy::Skip);
        assert!(result.is_err(), "{asked:?} -> {result:?}");
    }
}
