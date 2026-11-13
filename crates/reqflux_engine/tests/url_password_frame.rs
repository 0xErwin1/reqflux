//! A password typed in a URL's user info is masked only in its URL frame, `:password@`, so a
//! short or common one never masks unrelated text or refuses a redirect (sent-headers T5).

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use common::{id_text, open_engine, runtime, temp_dir, write, write_get_workspace};
use reqflux_core::model::Request;
use reqflux_engine::state::{AppState, HistoryOutcome};
use reqflux_engine::{CancellationToken, MASK, RunOptions, history_document};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ok_server(runtime: &tokio::runtime::Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn a_short_or_common_password_is_masked_only_in_its_frame() {
    let runtime = runtime();
    let server = ok_server(&runtime);
    let port = server.address().port();
    for password in ["1", "admin"] {
        let dir = temp_dir();
        write_get_workspace(
            dir.path(),
            &format!(
                "http://admin:{password}@127.0.0.1:{port}/{password}/x?n=1&admin={password}&at=a1"
            ),
        );
        let engine = open_engine(dir.path()).unwrap();
        let executed = engine
            .execute_blocking("api/r", &common::options(None, &[]))
            .unwrap();
        let expected =
            format!("http://admin:{MASK}@127.0.0.1:{port}/{password}/x?n=1&admin={password}&at=a1");
        assert_eq!(executed.sent.url, expected, "{password}");
        let wire = &executed.response.timings.hops[0].wire;
        assert!(!wire.url.contains(&format!(":{password}@")), "{}", wire.url);
        // The prepared request's masker leaves the bare value alone.
        let prepared = engine
            .prepare("api/r", &common::options(None, &[]))
            .unwrap();
        let masker = prepared.masker();
        assert_eq!(
            masker.mask(&format!("n={password} user=admin {{\"n\":10}}")),
            format!("n={password} user=admin {{\"n\":10}}"),
            "{password}"
        );
    }
}

#[test]
fn a_cross_origin_307_with_a_short_password_still_follows_with_its_body() {
    let runtime = runtime();
    let other = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/landing"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    });
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/p"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/landing", other.uri()).as_str()),
            )
            .mount(&server)
            .await;
        server
    });
    let port = server.address().port();
    let dir = temp_dir();
    write(
        dir.path(),
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        dir.path(),
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", id_text(1)),
    );
    write(
        dir.path(),
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"R\"\nmethod = \"POST\"\nurl = \"http://ana:1@127.0.0.1:{port}/p\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"n\":10}}'\n",
            id_text(2)
        ),
    );
    let engine = open_engine(dir.path()).unwrap();
    let executed = engine
        .execute_blocking("api/r", &common::options(None, &[]))
        .unwrap();
    assert_eq!(executed.response.status, 200);
    let arrived = runtime.block_on(other.received_requests()).unwrap();
    assert_eq!(arrived.len(), 1);
    assert_eq!(arrived[0].body, b"{\"n\":10}");
}

#[test]
fn the_history_document_masks_a_literal_url_password_and_still_parses() {
    let request: Request = toml::from_str(
        "id = \"01J00000000000000000000002\"\nname = \"R\"\nmethod = \"GET\"\n\
         url = \"http://admin:admin@api.test/admin?x=1\"\n",
    )
    .unwrap();
    let document = history_document(&request).unwrap();
    assert!(document.contains(&format!("http://admin:{MASK}@api.test/admin?x=1")));
    let reopened: Request = toml::from_str(&document).unwrap();
    assert_eq!(
        reopened.url,
        format!("http://admin:{MASK}@api.test/admin?x=1")
    );
    assert_eq!(reopened.name, "R");
}

#[test]
fn the_history_document_keeps_a_variable_password_reference() {
    let request: Request = toml::from_str(
        "id = \"01J00000000000000000000002\"\nname = \"R\"\nmethod = \"GET\"\n\
         url = \"http://ana:{{pw}}@{{host}}/x\"\n",
    )
    .unwrap();
    let document = history_document(&request).unwrap();
    let reopened: Request = toml::from_str(&document).unwrap();
    assert_eq!(reopened.url, "http://ana:{{pw}}@{{host}}/x");
}

#[test]
fn a_run_records_the_document_with_the_url_password_masked() {
    let runtime = runtime();
    let server = ok_server(&runtime);
    let port = server.address().port();
    let dir = temp_dir();
    write_get_workspace(dir.path(), &format!("http://ana:1@127.0.0.1:{port}/x"));
    let state_dir = temp_dir();
    let mut state = AppState::open(&state_dir.path().join("state.sqlite3")).unwrap();
    let engine = open_engine(dir.path()).unwrap();
    engine
        .run_collection(
            "api",
            &RunOptions::default(),
            &CancellationToken::new(),
            Some(&mut state),
            &mut |_| {},
        )
        .unwrap();
    let entries = state.history(&id_text(900), 10).unwrap();
    assert_eq!(entries.len(), 1);
    let record = &entries[0].record;
    assert!(matches!(record.outcome, HistoryOutcome::Response(_)));
    let reopened: Request = toml::from_str(&record.document).unwrap();
    assert_eq!(
        reopened.url,
        format!("http://ana:{MASK}@127.0.0.1:{port}/x")
    );
    assert_eq!(record.sent.url, reopened.url);
}
