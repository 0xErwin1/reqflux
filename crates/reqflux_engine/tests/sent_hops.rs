//! The request each hop sent, masked before it leaves the send task and kept by history
//! (sent-headers S3, S4).

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use common::{id_text, runtime, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::state::{AppState, HistoryOutcome, HistoryRecord, RecordedResponse};
use reqflux_engine::{Engine, Executed, MASK, Timings};
use tokio::runtime::Runtime;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "tok-7f3a9c";

/// `/login` sets the cookie `session=abc123`; `/start` redirects to `/echo`, which answers.
fn server(runtime: &Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/login"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("set-cookie", "session=abc123; Path=/"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/echo"))
            .mount(&server)
            .await;
        Mock::given(path("/echo"))
            .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
            .mount(&server)
            .await;
        server
    })
}

/// A workspace whose collection declares the secret variable `token`; `api/login` GETs
/// `/login`, and `api/r` GETs `/start?key={{token}}` with the secret in `X-Api-Key` and in an
/// `Authorization` bearer.
fn workspace(root: &Path, base: &str) {
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
        "collections/api/login.req.toml",
        &format!(
            "id = \"{}\"\nname = \"login\"\nmethod = \"GET\"\nurl = \"{base}/login\"\n",
            id_text(3)
        ),
    );
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\nurl = \"{base}/start?key={{{{token}}}}\"\n\n[[headers]]\nkey = \"X-Api-Key\"\nvalue = \"{{{{token}}}}\"\n\n[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{token}}}}\"\n\n[[headers]]\nkey = \"X-Plain\"\nvalue = \"plain\"\n",
            id_text(2)
        ),
    );
}

fn send(engine: &Engine) -> Executed {
    let options = common::options(None, &[("token", SECRET)]);
    engine.execute_blocking("api/login", &options).unwrap();
    engine.execute_blocking("api/r", &options).unwrap()
}

fn value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn every_hop_wire_is_masked_before_it_leaves_the_send() {
    let runtime = runtime();
    let server = server(&runtime);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri());
    let engine = common::open_engine(dir.path()).unwrap();

    let executed = send(&engine);

    let hops = &executed.response.timings.hops;
    assert_eq!(hops.len(), 2);
    for hop in hops {
        let headers = &hop.wire.headers;
        assert_eq!(value(headers, "x-api-key"), Some(MASK), "{headers:?}");
        assert_eq!(
            value(headers, "authorization"),
            Some(format!("Bearer {MASK}").as_str()),
            "{headers:?}"
        );
        assert_eq!(value(headers, "cookie"), Some(MASK), "{headers:?}");
        assert_eq!(value(headers, "x-plain"), Some("plain"), "{headers:?}");
    }
    assert_eq!(
        hops[0].wire.url,
        format!("{}/start?key={MASK}", server.uri())
    );
    assert_eq!(hops[1].wire.url, format!("{}/echo", server.uri()));
    let debug = format!("{:?}", executed.response);
    assert!(!debug.contains(SECRET), "{debug}");
    assert!(!debug.contains("abc123"), "{debug}");
}

fn record(executed: &Executed) -> HistoryRecord {
    HistoryRecord {
        sent_at: 1,
        request_id: "req".to_owned(),
        request_name: "r".to_owned(),
        request_path: "api/r".to_owned(),
        document: String::new(),
        sent: executed.sent.clone(),
        outcome: HistoryOutcome::Response(RecordedResponse::from_response(&executed.response)),
    }
}

fn timings_of(record: &HistoryRecord) -> Option<&Timings> {
    match &record.outcome {
        HistoryOutcome::Response(response) => response.timings.as_ref(),
        HistoryOutcome::Failed { .. } => None,
    }
}

#[test]
fn history_keeps_the_masked_hop_requests() {
    let runtime = runtime();
    let server = server(&runtime);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri());
    let engine = common::open_engine(dir.path()).unwrap();
    let executed = send(&engine);
    let mut state = AppState::open(&dir.path().join("state.sqlite3")).unwrap();

    let id = state.record_history("ws", record(&executed)).unwrap();

    let entry = state.history_entry(id).unwrap().unwrap();
    let stored = timings_of(&entry.record).unwrap();
    assert_eq!(stored.hops.len(), 2);
    for (stored, sent) in stored.hops.iter().zip(&executed.response.timings.hops) {
        assert_eq!(stored.wire, sent.wire);
        assert!(!stored.wire.headers.is_empty());
    }
    assert_eq!(stored.hops[0].wire.method, "GET");
    let listed = state.history("ws", 10).unwrap();
    assert_eq!(listed[0].record, entry.record);
}

#[test]
fn an_old_timings_row_without_wire_fields_decodes_with_an_empty_wire() {
    let dir = temp_dir();
    let path = dir.path().join("state.sqlite3");
    let executed = {
        let runtime = runtime();
        let server = server(&runtime);
        workspace(dir.path(), &server.uri());
        let engine = common::open_engine(dir.path()).unwrap();
        send(&engine)
    };
    let id = AppState::open(&path)
        .unwrap()
        .record_history("ws", record(&executed))
        .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE history SET timings = ?1 WHERE id = ?2",
            rusqlite::params![
                r#"[{"connect_us":5,"first_byte_us":10,"download_us":2},{"first_byte_us":4,"download_us":1}]"#,
                id
            ],
        )
        .unwrap();
    }

    let entry = AppState::open(&path)
        .unwrap()
        .history_entry(id)
        .unwrap()
        .unwrap();

    let hops = &timings_of(&entry.record).unwrap().hops;
    assert_eq!(hops.len(), 2);
    assert!(
        hops.iter()
            .all(|hop| hop.wire == reqflux_engine::WireRequest::default())
    );
    assert_eq!(hops[0].first_byte, std::time::Duration::from_micros(10));
}
