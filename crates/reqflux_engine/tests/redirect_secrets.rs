//! A redirect to another origin never carries a secret of the send, for the main request and
//! for a script's `pm.sendRequest`, with no host guard (redirect-secret-headers S1-S4).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use common::{id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::{ConsoleLevel, ExecuteError, Executed, MASK};
use tokio::runtime::Runtime;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "tok-7f3a9c";

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

/// A server answering `/echo` with 200 and `/start` with `status` to `location`, relative
/// to the server itself when it starts with `/`.
fn server(runtime: &Runtime, status: u16, location: Option<&str>) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/echo"))
            .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
            .mount(&server)
            .await;
        if let Some(location) = location {
            Mock::given(path("/start"))
                .respond_with(ResponseTemplate::new(status).insert_header("Location", location))
                .mount(&server)
                .await;
        }
        server
    })
}

fn received(runtime: &Runtime, server: &MockServer) -> Vec<wiremock::Request> {
    runtime
        .block_on(server.received_requests())
        .unwrap_or_default()
}

/// A workspace whose collection declares the secret variable `token`, its value set as a
/// local variable by [`execute`], and one request `api/r`:
/// `method` to `url` with the headers `X-Api-Key: {{token}}` and `X-Request-Id: plain`, an
/// optional raw `body`, and an optional pre-request `script`.
fn workspace(root: &Path, method: &str, url: &str, body: Option<&str>, script: Option<&str>) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\n\n[[variables]]\nkey = \"token\"\nsecret = true\n",
            id_text(1)
        ),
    );
    let body = body.map_or_else(String::new, |text| {
        format!("\n[body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{text}'\n")
    });
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"{method}\"\nurl = \"{url}\"\n\n[[headers]]\nkey = \"X-Api-Key\"\nvalue = \"{{{{token}}}}\"\n\n[[headers]]\nkey = \"X-Request-Id\"\nvalue = \"plain\"\n{body}",
            id_text(2)
        ),
    );
    if let Some(script) = script {
        write(root, "collections/api/r.pre.js", script);
    }
}

fn execute(root: &Path) -> Result<Executed, ExecuteError> {
    let engine = match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    engine.execute_blocking("api/r", &common::options(None, &[("token", SECRET)]))
}

fn executed(root: &Path) -> Executed {
    match execute(root) {
        Ok(executed) => executed,
        Err(error) => panic!("execute: {error}"),
    }
}

fn header<'a>(request: &'a wiremock::Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[test]
fn a_cross_origin_redirect_drops_the_secret_header_and_keeps_the_plain_one() {
    let rt = runtime();
    let b = server(&rt, 200, None);
    let a = server(&rt, 302, Some(&format!("{}/echo", b.uri())));
    let dir = temp_dir();
    workspace(dir.path(), "GET", &format!("{}/start", a.uri()), None, None);
    let executed = executed(dir.path());
    assert_eq!(executed.response.status, 200);
    assert_eq!(executed.response.final_url, format!("{}/echo", b.uri()));
    assert_eq!(header(&received(&rt, &a)[0], "x-api-key"), Some(SECRET));
    let arrived = received(&rt, &b);
    assert_eq!(arrived.len(), 1);
    assert_eq!(header(&arrived[0], "x-api-key"), None);
    assert_eq!(header(&arrived[0], "x-request-id"), Some("plain"));
}

#[test]
fn a_same_origin_redirect_keeps_the_secret_header() {
    let rt = runtime();
    let a = server(&rt, 302, Some("/echo"));
    let dir = temp_dir();
    workspace(dir.path(), "GET", &format!("{}/start", a.uri()), None, None);
    let executed = executed(dir.path());
    assert_eq!(executed.response.final_url, format!("{}/echo", a.uri()));
    let arrived = received(&rt, &a);
    assert_eq!(arrived.len(), 2);
    assert_eq!(header(&arrived[1], "x-api-key"), Some(SECRET));
}

#[test]
fn a_cross_origin_307_with_a_secret_body_is_refused_and_the_target_sees_nothing() {
    let rt = runtime();
    let b = server(&rt, 200, None);
    let a = server(&rt, 307, Some(&format!("{}/echo?t={SECRET}", b.uri())));
    let dir = temp_dir();
    workspace(
        dir.path(),
        "POST",
        &format!("{}/start", a.uri()),
        Some(r#"{"token":"{{token}}"}"#),
        None,
    );
    let message = match execute(dir.path()) {
        Err(error) => error.to_string(),
        Ok(executed) => panic!("expected an error, got {}", executed.response.status),
    };
    assert!(
        message.contains(&format!(
            "a redirect to {}/echo?t={MASK} was not followed: the request body holds a secret",
            b.uri()
        )),
        "{message}"
    );
    assert!(!message.contains(SECRET), "{message}");
    assert!(received(&rt, &b).is_empty());
}

#[test]
fn a_script_send_request_drops_the_secret_header_on_a_cross_origin_redirect() {
    let rt = runtime();
    let b = server(&rt, 200, None);
    let a = server(&rt, 302, Some(&format!("{}/echo", b.uri())));
    let main = server(&rt, 200, None);
    let dir = temp_dir();
    let script = format!(
        "pm.sendRequest({{ url: \"{}/start\", method: \"GET\", header: {{ \"X-Api-Key\": pm.variables.get(\"token\"), \"X-Request-Id\": \"plain\" }} }}, function (error, response) {{\n  if (error) {{ console.error(String(error)); }} else {{ console.log(String(response.code)); }}\n}});\n",
        a.uri()
    );
    workspace(
        dir.path(),
        "GET",
        &format!("{}/echo", main.uri()),
        None,
        Some(&script),
    );
    let executed = executed(dir.path());
    let console: Vec<(ConsoleLevel, String)> = executed
        .scripts
        .console
        .iter()
        .map(|entry| (entry.level, entry.message.clone()))
        .collect();
    assert_eq!(console, vec![(ConsoleLevel::Log, "200".to_owned())]);
    assert_eq!(header(&received(&rt, &a)[0], "x-api-key"), Some(SECRET));
    let arrived = received(&rt, &b);
    assert_eq!(arrived.len(), 1);
    assert_eq!(header(&arrived[0], "x-api-key"), None);
    assert_eq!(header(&arrived[0], "x-request-id"), Some("plain"));
}
