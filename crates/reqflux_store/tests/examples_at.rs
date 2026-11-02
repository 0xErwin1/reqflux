//! Creating examples by request path, without a loaded workspace (v1-features S4).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use reqflux_core::model::{Example, ExampleBody, ExampleRequest, ExampleResponse, Id, Method};
use reqflux_store::{Parent, create_example_at, example_files_at};

fn example(name: &str) -> Example {
    Example {
        id: Id::new(),
        name: name.to_owned(),
        request: ExampleRequest {
            method: Method::GET,
            url: "{{base}}/ping".to_owned(),
            params: Vec::new(),
            headers: Vec::new(),
            auth: reqflux_core::model::Auth::Inherit,
            body: reqflux_core::model::Body::None,
        },
        response: ExampleResponse {
            status: 200,
            reason: None,
            headers: Vec::new(),
            body: ExampleBody::Text("pong".to_owned()),
        },
        extensions: toml::Table::new(),
    }
}

#[test]
fn planned_files_match_what_creating_in_order_writes() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let parent = Parent::collection("shop");
    let planned = match example_files_at(dir.path(), &parent, "ping", &["OK", "OK", "Gone"]) {
        Ok(files) => files,
        Err(error) => panic!("plan: {error}"),
    };
    let base = dir.path().join("collections/shop/ping.examples");
    assert_eq!(
        planned,
        [
            base.join("ok.example.toml"),
            base.join("ok-2.example.toml"),
            base.join("gone.example.toml"),
        ]
    );
    let mut written = Vec::<PathBuf>::new();
    for name in ["OK", "OK", "Gone"] {
        match create_example_at(dir.path(), &parent, "ping", example(name), None) {
            Ok(slug) => written.push(base.join(format!("{slug}.example.toml"))),
            Err(error) => panic!("create: {error}"),
        }
    }
    assert_eq!(written, planned);
    assert!(written.iter().all(|file| file.exists()));
}

#[test]
fn a_reserved_request_slug_is_refused() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let parent = Parent::collection("shop");
    assert!(create_example_at(dir.path(), &parent, "collection", example("OK"), None).is_err());
    assert!(!dir.path().join("collections").exists());
}
