//! Local API docs from a collection (v1-features S6): Markdown and one static HTML page,
//! requests grouped by folder, never a secret value.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use reqflux_engine::{ApiDocs, DocsError, Engine, postman};
use serde_json::{Value, json};

use common::{open_engine, snapshot, temp_dir};

const ROUND_TRIP: &str =
    include_str!("../../reqflux_postman/tests/fixtures/collection-roundtrip.json");

fn fixture() -> Value {
    let mut value: Value = match serde_json::from_str(ROUND_TRIP) {
        Ok(value) => value,
        Err(error) => panic!("fixture: {error}"),
    };
    if let Some(items) = value["item"].as_array_mut() {
        items.push(json!({
            "name": "Tricky <script>",
            "request": {
                "method": "GET",
                "header": [
                    { "key": "Authorization", "value": "Bearer literal-token-123" },
                    { "key": "Cookie", "value": "{{session_cookie}}" },
                    { "key": "Proxy-Authorization", "value": "Basic {{proxy}}" }
                ],
                "url": "{{base_url}}/tricky",
                "description": "Says <b>hi</b> & leaves."
            }
        }));
    }
    value
}

fn workspace() -> (tempfile::TempDir, Engine) {
    let dir = temp_dir();
    if let Err(error) = postman(dir.path(), &fixture(), &[]) {
        panic!("import: {error}");
    }
    let engine = match open_engine(dir.path()) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    (dir, engine)
}

fn docs(engine: &Engine, selector: &str) -> ApiDocs {
    match engine.docs(selector) {
        Ok(docs) => docs,
        Err(error) => panic!("docs {selector}: {error}"),
    }
}

fn file<'a>(docs: &'a ApiDocs, name: &str) -> &'a str {
    match docs.files.iter().find(|file| file.name == name) {
        Some(file) => &file.content,
        None => panic!("{name} is not generated"),
    }
}

#[test]
fn markdown_groups_requests_by_folder_with_their_parts() {
    let (_dir, engine) = workspace();
    let docs = docs(&engine, "round-trip");
    assert_eq!(docs.collection, "Round Trip");
    let names: Vec<&str> = docs.files.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(names, ["README.md", "index.html"]);
    let markdown = file(&docs, "README.md");
    for expected in [
        "# Round Trip\n",
        "## Users\n",
        "Everything about users.",
        "### GET List users\n",
        "`GET {{base_url}}/users?page=2&size=10`",
        "Lists users, paged.",
        "Auth: inherited from Users (digest)",
        "| page | 2 |",
        "| debug | true | disabled |",
        "| Accept | application/json | json only |",
        "#### Example: Page two (200 OK)",
        "{\"users\": []}",
        "### Nested\n",
        "#### PUT Upload avatar\n",
        "| file | file: /tmp/avatar.png | the image |",
        "## POST Login\n",
        "Auth: oauth2",
        "Body: urlencoded",
        "Body: graphql",
        "{ users { id } }",
        "Auth: apikey",
        "Body: binary, /tmp/payload.bin",
        "| api_secret | (secret) |",
        "| base_url | https://api.example.com |",
    ] {
        assert!(
            markdown.contains(expected),
            "{expected:?} missing:\n{markdown}"
        );
    }
}

#[test]
fn no_secret_value_and_no_literal_credential_reaches_the_docs() {
    let (_dir, engine) = workspace();
    let docs = docs(&engine, "round-trip");
    for content in docs.files.iter().map(|file| file.content.as_str()) {
        assert!(!content.contains("literal-token-123"), "{content}");
        assert!(!content.contains("do-not-export"), "{content}");
        assert!(content.contains("{{session_cookie}}"), "{content}");
        assert!(content.contains("Basic {{proxy}}"), "{content}");
        assert!(content.contains("••••••••"), "{content}");
    }
}

#[test]
fn the_html_page_escapes_what_the_workspace_says() {
    let (_dir, engine) = workspace();
    let docs = docs(&engine, "round-trip");
    let html = file(&docs, "index.html");
    assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
    assert!(html.contains("<title>Round Trip</title>"), "{html}");
    assert!(html.contains("Tricky &lt;script&gt;"), "{html}");
    assert!(
        html.contains("Says &lt;b&gt;hi&lt;/b&gt; &amp; leaves."),
        "{html}"
    );
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("<h2 id=\""), "{html}");
    assert!(html.contains("<nav>"), "{html}");
}

#[test]
fn writing_refuses_to_replace_files_unless_forced() {
    let (_dir, engine) = workspace();
    let docs = docs(&engine, "round-trip");
    let out = temp_dir();
    let target = out.path().join("api-docs");
    let written = match docs.write(&target, false) {
        Ok(written) => written,
        Err(error) => panic!("write: {error}"),
    };
    assert_eq!(
        written,
        [target.join("README.md"), target.join("index.html")]
    );
    let before = snapshot(out.path());
    let error = match docs.write(&target, false) {
        Err(error) => error,
        Ok(_) => panic!("the files exist"),
    };
    assert!(matches!(error, DocsError::Exists { .. }), "{error}");
    assert_eq!(snapshot(out.path()), before);
    if let Err(error) = fs::write(target.join("README.md"), "old") {
        panic!("write: {error}");
    }
    assert!(docs.write(&target, true).is_ok());
    let markdown = fs::read_to_string(target.join("README.md")).unwrap_or_default();
    assert!(markdown.starts_with("# Round Trip"), "{markdown}");
}

#[test]
fn docs_take_a_collection_only() {
    let (_dir, engine) = workspace();
    for selector in ["round-trip/users", "missing", "env:x"] {
        let error = match engine.docs(selector) {
            Err(error) => error,
            Ok(_) => panic!("{selector} is not a collection"),
        };
        assert_eq!(
            error.to_string(),
            format!("no collection matches {selector}")
        );
    }
    let id = engine.workspace().collections[0].collection.id.to_string();
    assert!(engine.docs(&id).is_ok());
    assert!(!Path::new(&id).exists());
}
