//! A proxy URL typed with a password never reaches `config.toml`: the password goes to the
//! keyring (`network/proxy`), the file keeps the URL without it and the username.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use std::fs;

use reqflux_engine::config::{ConfigError, set_proxy_url, set_proxy_url_with_secret};
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

#[test]
fn the_password_goes_to_the_keyring_and_the_file_keeps_the_rest() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let store = MemorySecretStore::new();
    let outcome = set_proxy_url_with_secret(&path, "http://u:secretpw@127.0.0.1:3999", &store);
    assert!(outcome.is_ok(), "{outcome:?}");
    let text = read(&path);
    assert!(!text.contains(PASSWORD), "{text}");
    assert!(text.contains("url = \"http://u@127.0.0.1:3999\""), "{text}");
    assert!(text.contains("username = \"u\""), "{text}");
    assert_eq!(
        store.get_account("network/proxy").ok().flatten().as_deref(),
        Some(PASSWORD)
    );
}

#[test]
fn a_percent_encoded_password_is_stored_decoded() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let store = MemorySecretStore::new();
    let outcome = set_proxy_url_with_secret(&path, "http://u:p%40ss@proxy.test", &store);
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(
        store.get_account("network/proxy").ok().flatten().as_deref(),
        Some("p@ss")
    );
    assert!(!read(&path).contains("p%40ss"));
}

#[test]
fn a_keyring_failure_writes_nothing_and_never_shows_the_password() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let store = MemorySecretStore::unavailable("no keyring");
    match set_proxy_url_with_secret(&path, "http://u:secretpw@127.0.0.1:3999", &store) {
        Err(error) => assert!(!error.to_string().contains(PASSWORD), "{error}"),
        Ok(()) => panic!("expected the keyring failure"),
    }
    assert!(!path.exists(), "nothing is written");
}

#[test]
fn a_url_without_a_password_is_written_as_is_and_touches_no_secret() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    let store = MemorySecretStore::unavailable("no keyring");
    let outcome = set_proxy_url_with_secret(&path, "http://u@proxy.test:3128", &store);
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(read(&path).contains("url = \"http://u@proxy.test:3128\""));
}

#[test]
fn set_proxy_url_itself_refuses_a_url_with_a_password() {
    let dir = dir();
    let path = dir.path().join("config.toml");
    match set_proxy_url(&path, "http://u:secretpw@127.0.0.1:3999") {
        Err(error @ ConfigError::PasswordInUrl { .. }) => {
            assert!(!error.to_string().contains(PASSWORD), "{error}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(!path.exists(), "nothing is written");
}

#[test]
fn a_password_with_an_unencoded_slash_question_mark_or_hash_never_reaches_the_file() {
    for password in ["p/ss", "p?ss", "p#ss"] {
        let dir = dir();
        let path = dir.path().join("config.toml");
        let store = MemorySecretStore::new();
        let url = format!("http://u:{password}@proxy.test:3128");
        let outcome = set_proxy_url_with_secret(&path, &url, &store);
        assert!(outcome.is_ok(), "{password}: {outcome:?}");
        let text = read(&path);
        assert!(!text.contains(password), "{password}: {text}");
        assert!(
            text.contains("url = \"http://u@proxy.test:3128\""),
            "{password}: {text}"
        );
        assert_eq!(
            store.get_account("network/proxy").ok().flatten().as_deref(),
            Some(password)
        );
    }
}
