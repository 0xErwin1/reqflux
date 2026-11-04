//! The proxy password typed on its own (the masked Password field) goes to the keyring entry
//! `network/proxy`; the file only gets the `username` the engine needs to send it.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use std::fs;

use reqflux_engine::config::set_proxy_password;
use reqflux_engine::{MemorySecretStore, SecretStore};

const PASSWORD: &str = "secretpw";

fn read(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn dir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

fn stored(store: &MemorySecretStore) -> Option<String> {
    store.get_account("network/proxy").ok().flatten()
}

#[test]
fn the_password_goes_to_the_keyring_and_the_username_comes_from_the_url() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let before = "[network.proxy]\nmode = \"manual\"\nurl = \"http://me@proxy.test:3128\"\n";
    fs::write(&path, before).ok();
    let store = MemorySecretStore::new();
    let outcome = set_proxy_password(&path, PASSWORD, &store);
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(stored(&store).as_deref(), Some(PASSWORD));
    let text = read(&path);
    assert!(!text.contains(PASSWORD), "{text}");
    assert_eq!(text, format!("{before}username = \"me\"\n"));
}

#[test]
fn an_existing_username_is_kept() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let before = "[network.proxy]\nmode = \"manual\"\nurl = \"http://me@proxy.test\"\n\
                  username = \"other\"\n";
    fs::write(&path, before).ok();
    let store = MemorySecretStore::new();
    assert!(set_proxy_password(&path, PASSWORD, &store).is_ok());
    assert_eq!(read(&path), before);
    assert_eq!(stored(&store).as_deref(), Some(PASSWORD));
}

#[test]
fn an_empty_password_removes_the_entry_and_leaves_the_file_alone() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let before = "[network.proxy]\nmode = \"manual\"\nusername = \"me\"\n";
    fs::write(&path, before).ok();
    let store = MemorySecretStore::new();
    assert!(store.set_account("network/proxy", PASSWORD).is_ok());
    assert!(set_proxy_password(&path, "", &store).is_ok());
    assert_eq!(stored(&store), None);
    assert_eq!(read(&path), before);
}

#[test]
fn a_keyring_failure_writes_nothing_and_never_shows_the_password() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let before = "[network.proxy]\nmode = \"manual\"\nurl = \"http://me@proxy.test\"\n";
    fs::write(&path, before).ok();
    let store = MemorySecretStore::unavailable("no keyring");
    match set_proxy_password(&path, PASSWORD, &store) {
        Err(error) => assert!(!error.to_string().contains(PASSWORD), "{error}"),
        Ok(()) => panic!("expected the keyring failure"),
    }
    assert_eq!(read(&path), before, "nothing is written");
}

#[test]
fn a_file_that_does_not_parse_leaves_the_keyring_alone() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[network\n").ok();
    let store = MemorySecretStore::new();
    assert!(set_proxy_password(&path, PASSWORD, &store).is_err());
    assert_eq!(stored(&store), None);
}
