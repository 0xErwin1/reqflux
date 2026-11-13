//! Secrets the masker only learns late, or sees in another case, never reach a hop's wire
//! record in clear (sent-headers S4).

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{id_text, temp_dir, write};
use reqflux_engine::state::RecordedResponse;
use reqflux_engine::{Engine, ExecuteOptions, Executed, MemorySecretStore};
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn start(runtime: &Runtime, mocks: Vec<Mock>) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        for mock in mocks {
            mock.mount(&server).await;
        }
        server
    })
}

fn engine(root: &Path, request: &str) -> Engine {
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
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"R\"\nmethod = \"GET\"\n{request}",
            id_text(2)
        ),
    );
    Engine::open_with_secrets(root, Arc::new(MemorySecretStore::new())).unwrap()
}

fn run(engine: &Engine, token: &str) -> Executed {
    let mut options = ExecuteOptions::default();
    options.vars.push(("token".to_owned(), token.to_owned()));
    engine.execute_blocking("api/r", &options).unwrap()
}

fn assert_clean(executed: &Executed, needles: &[&str]) {
    let wire = format!(
        "{:?}",
        executed
            .response
            .timings
            .hops
            .iter()
            .map(|hop| &hop.wire)
            .collect::<Vec<_>>()
    );
    let history = format!(
        "{:?}",
        RecordedResponse::from_response(&executed.response).timings
    );
    assert!(!wire.is_empty() && wire != "[]", "no hop was recorded");
    for needle in needles {
        assert!(!wire.contains(needle), "wire leaks {needle}: {wire}");
        assert!(
            !history.contains(needle),
            "history leaks {needle}: {history}"
        );
    }
}

#[test]
fn an_oauth_query_token_is_masked_in_every_hop() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"access_token": "query-token-xyz"})),
                ),
            Mock::given(method("GET"))
                .and(path("/api"))
                .respond_with(ResponseTemplate::new(302).insert_header("Location", "/done")),
            Mock::given(method("GET"))
                .and(path("/done"))
                .respond_with(ResponseTemplate::new(200)),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &format!(
            "url = \"{u}/api\"\n\n[auth]\ntype = \"oauth2\"\ntoken_url = \"{u}/token\"\n\
             grant_type = \"client_credentials\"\nclient_id = \"app\"\nclient_secret = \"cs\"\n\
             client_auth = \"header\"\ntoken_location = \"query\"\nquery_param = \"token\"\n",
            u = server.uri()
        ),
    );
    let executed = run(&engine, "unused");
    // The redirect's second hop carries the first URL in its Referer.
    assert_eq!(executed.response.timings.hops.len(), 2);
    assert_clean(&executed, &["query-token-xyz"]);
}

#[test]
fn a_mixed_case_secret_in_a_header_name_is_masked_on_the_wire() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(200))],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &format!(
            "url = \"{}/x\"\n\n[[headers]]\nkey = \"X-{{{{token}}}}\"\nvalue = \"v\"\n",
            server.uri()
        ),
    );
    assert_clean(&run(&engine, "MixCase9Q"), &["MixCase9Q", "mixcase9q"]);
}

#[test]
fn a_mixed_case_secret_in_the_host_is_masked_on_the_wire() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(200))],
    );
    let dir = temp_dir();
    let port = server.address().port();
    let engine = engine(
        dir.path(),
        &format!("url = \"http://{{{{token}}}}.localhost:{port}/x\"\n"),
    );
    let executed = run(&engine, "HostSecretQ7");
    // The URL parser lowercases the host, so the secret reaches the wire in another case.
    let wire = format!("{:?}", executed.response.timings).to_lowercase();
    assert!(!wire.contains("hostsecretq7"), "{wire}");
    assert!(wire.contains(".localhost"), "{wire}");
}

#[test]
fn a_mixed_case_host_secret_is_masked_in_a_redirect_referer() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![
            Mock::given(method("GET"))
                .and(path("/r"))
                .respond_with(ResponseTemplate::new(302).insert_header("Location", "/done")),
            Mock::given(method("GET"))
                .and(path("/done"))
                .respond_with(ResponseTemplate::new(200)),
        ],
    );
    let dir = temp_dir();
    let port = server.address().port();
    let engine = engine(
        dir.path(),
        &format!("url = \"http://{{{{token}}}}.localhost:{port}/r\"\n"),
    );
    let executed = run(&engine, "RedirSecretK");
    assert_eq!(executed.response.timings.hops.len(), 2);
    assert_clean(&executed, &["redirsecretk", "RedirSecretK"]);
}

#[test]
fn an_oauth_query_token_never_reaches_another_origin_in_the_referer() {
    let runtime = runtime();
    let other = start(
        &runtime,
        vec![
            Mock::given(method("GET"))
                .and(path("/done"))
                .respond_with(ResponseTemplate::new(200)),
        ],
    );
    let server = start(
        &runtime,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"access_token": "cross+origin/token=1"})),
                ),
            Mock::given(method("GET")).and(path("/api")).respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/done", other.uri()).as_str()),
            ),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &format!(
            "url = \"{u}/api\"\n\n[auth]\ntype = \"oauth2\"\ntoken_url = \"{u}/token\"\n\
             grant_type = \"client_credentials\"\nclient_id = \"app\"\nclient_secret = \"cs\"\n\
             client_auth = \"header\"\ntoken_location = \"query\"\nquery_param = \"token\"\n",
            u = server.uri()
        ),
    );
    let executed = run(&engine, "unused");
    assert_eq!(executed.response.status, 200);
    // The first hop carried the token in its query, encoded, so its URL is what a Referer repeats.
    let first = runtime.block_on(server.received_requests()).unwrap();
    let api = first
        .iter()
        .find(|request| request.url.path() == "/api")
        .unwrap();
    assert_eq!(api.url.query(), Some("token=cross%2Borigin%2Ftoken%3D1"));
    let received = runtime.block_on(other.received_requests()).unwrap();
    assert_eq!(received.len(), 1);
    let sent: Vec<String> = received[0]
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {}", value.to_str().unwrap_or_default()))
        .collect();
    assert!(
        sent.iter()
            .all(|line| !line.contains("cross+origin/token=1")
                && !line.contains("cross%2Borigin%2Ftoken%3D1")),
        "the other origin received the token: {sent:?}"
    );
    assert!(received[0].url.query().is_none());
}

#[test]
fn a_literal_password_in_the_url_is_masked_in_the_recorded_request() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(200))],
    );
    let dir = temp_dir();
    let port = server.address().port();
    let engine = engine(
        dir.path(),
        &format!("url = \"http://ana:lit-pass-42@127.0.0.1:{port}/x\"\n"),
    );
    let executed = run(&engine, "unused");
    assert!(
        !executed.sent.url.contains("lit-pass-42"),
        "{}",
        executed.sent.url
    );
    assert_eq!(
        executed.sent.url,
        format!("http://ana:{}@127.0.0.1:{port}/x", reqflux_engine::MASK)
    );
    assert_clean(&executed, &["lit-pass-42"]);
}

#[test]
fn a_url_password_needing_percent_encoding_is_masked_in_the_recorded_request() {
    let runtime = runtime();
    let server = start(
        &runtime,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(200))],
    );
    let dir = temp_dir();
    let port = server.address().port();
    let engine = engine(
        dir.path(),
        &format!("url = \"http://ana:p@ss^w|rd@127.0.0.1:{port}/x\"\n"),
    );
    let executed = run(&engine, "unused");
    assert_eq!(
        executed.sent.url,
        format!("http://ana:{}@127.0.0.1:{port}/x", reqflux_engine::MASK)
    );
}
