//! The per-workspace cookie jar (v1-features S5): cookies captured from `Set-Cookie`, sent on
//! matching requests, recorded masked, listed, deleted, cleared and kept in the app state.
//! Every test uses a temporary workspace and state file and an in-memory secret store.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use common::{runtime, temp_dir, write, write_get_workspace};
use pretty_assertions::assert_eq;
use reqflux_engine::{CookieError, Engine, ExecuteOptions, Executed, MASK, StoredCookie};
use tokio::runtime::Runtime;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A server whose `/login` sets `session=abc123` (path `/`) and `/api/scoped` sets
/// `scoped=1` for `/api` only; every other path answers 200.
fn server(runtime: &Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "session=abc123; Path=/; HttpOnly"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/api/scoped"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("set-cookie", "scoped=1; Path=/api"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/logout"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("set-cookie", "session=; Max-Age=0"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/me"))
            .and(header("cookie", "session=abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_string("known"))
            .mount(&server)
            .await;
        Mock::given(path("/me"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        server
    })
}

/// Writes a workspace whose `api/r` GETs `base/login`, with one more GET request per
/// `(slug, path)` of `requests`, as `api/<slug>` to `base/<path>`.
fn workspace(root: &Path, base: &str, requests: &[(&str, &str)]) {
    write_get_workspace(root, &format!("{base}/login"));
    for (index, (slug, target)) in requests.iter().enumerate() {
        write(
            root,
            &format!("collections/api/{slug}.req.toml"),
            &format!(
                "id = \"{}\"\nname = \"{slug}\"\nmethod = \"GET\"\nurl = \"{base}/{target}\"\n",
                common::id_text(10 + u32::try_from(index).unwrap())
            ),
        );
    }
}

fn send(engine: &Engine, selector: &str) -> Executed {
    engine
        .execute_blocking(selector, &ExecuteOptions::default())
        .unwrap()
}

fn names(cookies: &[StoredCookie]) -> Vec<&str> {
    cookies.iter().map(|cookie| cookie.name.as_str()).collect()
}

#[test]
fn a_cookie_set_by_a_response_is_sent_on_the_next_matching_request_and_recorded_masked() {
    let runtime = runtime();
    let server = server(&runtime);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri(), &[("me", "me")]);
    let engine = common::open_engine(dir.path()).unwrap();

    let before = send(&engine, "api/me");
    send(&engine, "api/r");
    let after = send(&engine, "api/me");

    assert_eq!(before.response.status, 401);
    assert!(
        !before
            .sent
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("cookie"))
    );
    assert_eq!(after.response.body, b"known");
    assert!(
        after
            .sent
            .headers
            .contains(&("Cookie".to_owned(), MASK.to_owned())),
        "{:?}",
        after.sent.headers
    );
    let cookies = engine.cookies();
    assert_eq!(cookies.len(), 1);
    let host = url::Url::parse(&server.uri())
        .unwrap()
        .host_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        cookies[0],
        StoredCookie {
            domain: host,
            host_only: true,
            path: "/".to_owned(),
            name: "session".to_owned(),
            value: "abc123".to_owned(),
            expires: None,
            secure: false,
            http_only: true,
            same_site: None,
        }
    );
    assert_eq!(cookies[0].masked().value, MASK);
    assert!(!format!("{cookies:?}").contains("abc123"));
}

#[test]
fn a_cookie_is_sent_only_under_its_path_and_an_expired_one_is_removed() {
    let runtime = runtime();
    let server = server(&runtime);
    let dir = temp_dir();
    workspace(
        dir.path(),
        &server.uri(),
        &[("scoped", "api/scoped"), ("logout", "logout"), ("me", "me")],
    );
    let engine = common::open_engine(dir.path()).unwrap();

    send(&engine, "api/r");
    send(&engine, "api/scoped");
    // `/me` is outside `/api`, so only the session goes.
    let me = send(&engine, "api/me");
    send(&engine, "api/logout");

    assert_eq!(me.response.body, b"known");
    assert_eq!(names(&engine.cookies()), vec!["scoped"]);
}

#[test]
fn cookies_are_added_deleted_and_cleared_through_the_engine() {
    let dir = temp_dir();
    write_get_workspace(dir.path(), "http://example.test/");
    let engine = common::open_engine(dir.path()).unwrap();

    let added = engine
        .add_cookie(
            "https://example.test/app",
            "a=1; Domain=example.test; Secure",
        )
        .unwrap()
        .unwrap();
    engine
        .add_cookie("https://example.test/", "b=2; Path=/x")
        .unwrap();
    engine.add_cookie("https://example.test/", "c=3").unwrap();

    assert_eq!(added.domain, "example.test");
    assert!(!added.host_only);
    assert!(added.secure);
    assert_eq!(names(&engine.cookies()), vec!["a", "c", "b"]);
    assert!(engine.delete_cookie("example.test", "/x", "b").unwrap());
    assert!(!engine.delete_cookie("example.test", "/x", "b").unwrap());
    assert_eq!(names(&engine.cookies()), vec!["a", "c"]);
    assert_eq!(engine.clear_cookies().unwrap(), 2);
    assert_eq!(engine.cookies(), Vec::new());
}

#[test]
fn a_refused_cookie_changes_nothing() {
    let dir = temp_dir();
    write_get_workspace(dir.path(), "http://example.test/");
    let engine = common::open_engine(dir.path()).unwrap();

    let mismatch = engine
        .add_cookie("https://example.test/", "a=secret-1; Domain=other.test")
        .unwrap_err();
    let not_http = engine.add_cookie("ftp://example.test/", "a=1").unwrap_err();

    assert!(matches!(mismatch, CookieError::Rejected(_)), "{mismatch}");
    assert!(!mismatch.to_string().contains("secret-1"), "{mismatch}");
    assert_eq!(
        not_http.to_string(),
        "cannot add a cookie for ftp://example.test/: it is not an http or https URL"
    );
    assert_eq!(engine.cookies(), Vec::new());
}

#[test]
fn a_persisted_jar_outlives_the_engine_and_is_shared_by_the_next_one() {
    let dir = temp_dir();
    let state = temp_dir();
    let state_file = state.path().join("state.sqlite3");
    write_get_workspace(dir.path(), "http://example.test/");
    {
        let engine = common::open_engine(dir.path()).unwrap();
        // A cookie added before the state is attached is written to it on attach.
        engine
            .add_cookie("https://example.test/", "early=1")
            .unwrap();
        engine.persist_cookies(&state_file).unwrap();
        engine
            .add_cookie("https://example.test/", "late=2; Max-Age=3600")
            .unwrap();
    }

    let engine = common::open_engine(dir.path()).unwrap();
    assert_eq!(engine.cookies(), Vec::new());
    engine.persist_cookies(&state_file).unwrap();

    let cookies = engine.cookies();
    assert_eq!(names(&cookies), vec!["early", "late"]);
    assert!(cookies[1].expires.is_some());
    engine.clear_cookies().unwrap();
    let again = common::open_engine(dir.path()).unwrap();
    again.persist_cookies(&state_file).unwrap();
    assert_eq!(again.cookies(), Vec::new());
}

#[test]
fn another_workspace_has_its_own_jar() {
    let first = temp_dir();
    let second = temp_dir();
    let state = temp_dir();
    let state_file = state.path().join("state.sqlite3");
    write_get_workspace(first.path(), "http://example.test/");
    write(
        second.path(),
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"Other\"\nformat = 1\n",
            common::id_text(901)
        ),
    );
    let engine = common::open_engine(first.path()).unwrap();
    engine.persist_cookies(&state_file).unwrap();
    engine.add_cookie("https://example.test/", "a=1").unwrap();

    let other = common::open_engine(second.path()).unwrap();
    other.persist_cookies(&state_file).unwrap();

    assert_eq!(other.cookies(), Vec::new());
}
