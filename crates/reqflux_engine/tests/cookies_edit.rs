//! Creating and editing a jar cookie from its fields (feedback-nov3 S9): the Cookies page's
//! "Add cookie" and pencil go through `Engine::set_cookie` and `Engine::replace_cookie`.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use common::{temp_dir, write_get_workspace};
use pretty_assertions::assert_eq;
use reqflux_engine::{CookieError, Engine, StoredCookie};

fn engine(dir: &tempfile::TempDir) -> Engine {
    write_get_workspace(dir.path(), "http://example.test/");
    common::open_engine(dir.path()).unwrap()
}

fn cookie(domain: &str, path: &str, name: &str, value: &str) -> StoredCookie {
    StoredCookie {
        domain: domain.to_owned(),
        host_only: true,
        path: path.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
        expires: None,
        secure: false,
        http_only: false,
        same_site: None,
    }
}

#[test]
fn a_cookie_written_from_its_fields_is_stored_as_given() {
    let dir = temp_dir();
    let engine = engine(&dir);
    let expires = time::OffsetDateTime::now_utc().unix_timestamp() + 3600;
    let wanted = StoredCookie {
        domain: "example.test".to_owned(),
        host_only: false,
        path: "/app".to_owned(),
        name: "theme".to_owned(),
        value: "dark mode".to_owned(),
        expires: Some(expires),
        secure: true,
        http_only: true,
        same_site: Some("Lax".to_owned()),
    };

    let stored = engine.set_cookie(&wanted).unwrap();

    assert_eq!(stored, wanted);
    assert_eq!(engine.cookies(), vec![wanted]);
}

#[test]
fn writing_a_cookie_under_an_existing_key_replaces_it() {
    let dir = temp_dir();
    let engine = engine(&dir);
    engine
        .set_cookie(&cookie("example.test", "/", "a", "1"))
        .unwrap();

    engine
        .set_cookie(&cookie("example.test", "/", "a", "2"))
        .unwrap();

    let cookies = engine.cookies();
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].value, "2");
}

#[test]
fn a_cookie_without_a_name_or_a_domain_is_refused_and_changes_nothing() {
    let dir = temp_dir();
    let engine = engine(&dir);
    let kept = cookie("example.test", "/", "keep", "v");
    engine.set_cookie(&kept).unwrap();

    let no_name = engine
        .set_cookie(&cookie("example.test", "/", " ", "secret-1"))
        .unwrap_err();
    let no_domain = engine
        .set_cookie(&cookie("", "/", "a", "secret-2"))
        .unwrap_err();
    let bad_path = engine
        .set_cookie(&cookie("example.test", "app", "a", "secret-3"))
        .unwrap_err();
    let bad_value = engine
        .set_cookie(&cookie("example.test", "/", "a", "x; y=z"))
        .unwrap_err();

    assert_eq!(
        no_name.to_string(),
        "the cookie was not saved: the name is empty"
    );
    assert_eq!(
        no_domain.to_string(),
        "the cookie was not saved: the domain is empty"
    );
    assert_eq!(
        bad_path.to_string(),
        "the cookie was not saved: the path must start with /"
    );
    assert!(matches!(bad_value, CookieError::Invalid(_)), "{bad_value}");
    for error in [&no_name, &no_domain, &bad_path, &bad_value] {
        assert!(!error.to_string().contains("secret-"), "{error}");
        assert!(!format!("{error:?}").contains("secret-"), "{error:?}");
    }
    assert_eq!(engine.cookies(), vec![kept]);
}

#[test]
fn an_already_expired_date_is_refused() {
    let dir = temp_dir();
    let engine = engine(&dir);
    let mut old = cookie("example.test", "/", "a", "1");
    old.expires = Some(time::OffsetDateTime::now_utc().unix_timestamp() - 60);

    let error = engine.set_cookie(&old).unwrap_err();

    assert_eq!(
        error.to_string(),
        "the cookie was not saved: it has already expired"
    );
    assert_eq!(engine.cookies(), Vec::new());
}

#[test]
fn replacing_a_cookie_under_a_new_key_moves_it() {
    let dir = temp_dir();
    let engine = engine(&dir);
    engine
        .set_cookie(&cookie("example.test", "/", "old", "1"))
        .unwrap();
    engine
        .set_cookie(&cookie("example.test", "/", "other", "9"))
        .unwrap();

    let moved = cookie("example.test", "/x", "new", "2");
    engine
        .replace_cookie("example.test", "/", "old", &moved)
        .unwrap();

    let names: Vec<_> = engine.cookies().into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["other", "new"]);
}

#[test]
fn a_refused_replacement_keeps_the_original() {
    let dir = temp_dir();
    let engine = engine(&dir);
    let original = cookie("example.test", "/", "old", "1");
    engine.set_cookie(&original).unwrap();

    let result = engine.replace_cookie(
        "example.test",
        "/",
        "old",
        &cookie("example.test", "/", "", "2"),
    );

    assert!(result.is_err());
    assert_eq!(engine.cookies(), vec![original]);
}

#[test]
fn a_written_cookie_is_kept_in_the_app_state() {
    let dir = temp_dir();
    let state = temp_dir();
    let file = state.path().join("state.sqlite3");
    let engine = engine(&dir);
    engine.persist_cookies(&file).unwrap();
    let written = cookie("example.test", "/", "a", "1");
    engine.set_cookie(&written).unwrap();
    engine
        .replace_cookie(
            "example.test",
            "/",
            "a",
            &cookie("example.test", "/", "b", "2"),
        )
        .unwrap();

    let again = common::open_engine(dir.path()).unwrap();
    again.persist_cookies(&file).unwrap();

    let names: Vec<_> = again.cookies().into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["b"]);
}
