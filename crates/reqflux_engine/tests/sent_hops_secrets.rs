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
