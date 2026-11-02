//! The collection runner (v1-features S1): run order, `pm.setNextRequest`, variables carried
//! between requests, cancellation, events, the summary and history.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use pretty_assertions::assert_eq;
use reqflux_engine::state::{AppState, HistoryOutcome};
use reqflux_engine::{
    CancellationToken, Engine, ExecuteError, ExecuteOptions, RunEnd, RunEvent, RunOptions,
    RunSummary,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{id_text, open_engine, temp_dir, write};

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

/// A server answering `GET /<name>` with 200 and the body `<name>` for each name, and any
/// other path with 404.
fn server(runtime: &Runtime, names: &[&str]) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        for name in names {
            Mock::given(method("GET"))
                .and(path(format!("/{name}")))
                .respond_with(ResponseTemplate::new(200).set_body_string(*name))
                .mount(&server)
                .await;
        }
        server
    })
}

/// A workspace with the collection `api` (id 3) holding, in this order, `a`, the folder `f`
/// (id 4) with `b` and the nested folder `g` with `c`, then `d`. Each request `x` has id
/// `id_text(n)` and sends `GET {base}/x`.
fn tree(base: &str) -> TempDir {
    let dir = temp_dir();
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"token\"\nvalue = \"shared\"\n",
            id_text(900)
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{}\"\nname = \"Dev\"\n\n[[variables]]\nkey = \"who\"\nvalue = \"dev\"\n",
            id_text(1)
        ),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\norder = [\"a\", \"f\", \"d\"]\n",
            id_text(3)
        ),
    );
    write(
        root,
        "collections/api/f/folder.toml",
        &format!(
            "id = \"{}\"\nname = \"F\"\norder = [\"b\", \"g\"]\n",
            id_text(4)
        ),
    );
    write(
        root,
        "collections/api/f/g/folder.toml",
        &format!("id = \"{}\"\nname = \"G\"\n", id_text(5)),
    );
    for (n, relative, name) in [
        (10, "a", "A"),
        (11, "f/b", "B"),
        (12, "f/g/c", "C"),
        (13, "d", "D"),
    ] {
        let slug = relative.rsplit('/').next().unwrap_or(relative);
        write(
            root,
            &format!("collections/api/{relative}.req.toml"),
            &format!(
                "id = \"{}\"\nname = \"{name}\"\nmethod = \"GET\"\nurl = \"{base}/{slug}\"\n",
                id_text(n)
            ),
        );
    }
    dir
}

fn engine(root: &Path) -> Engine {
    match open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

/// Runs `selector` collecting every event.
fn run_with(
    engine: &Engine,
    selector: &str,
    options: &RunOptions,
    cancel: &CancellationToken,
    history: Option<&mut AppState>,
) -> (RunSummary, Vec<RunEvent>) {
    let mut events = Vec::new();
    let summary = match engine.run_collection(selector, options, cancel, history, &mut |event| {
        events.push(event.clone());
    }) {
        Ok(summary) => summary,
        Err(error) => panic!("run {selector}: {error}"),
    };
    (summary, events)
}

fn run(engine: &Engine, selector: &str) -> RunSummary {
    run_with(
        engine,
        selector,
        &RunOptions::default(),
        &CancellationToken::new(),
        None,
    )
    .0
}

/// The names of the requests the run sent, in order.
fn sent_names(summary: &RunSummary) -> Vec<String> {
    summary
        .results
        .iter()
        .map(|result| result.item.name.clone())
        .collect()
}

#[test]
fn a_collection_runs_in_tree_order_recursively() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    let engine = engine(dir.path());

    let summary = run(&engine, "api");
    assert_eq!(sent_names(&summary), ["A", "B", "C", "D"]);
    assert_eq!(summary.end, RunEnd::Completed);
    assert_eq!(summary.requests(), 4);
    assert_eq!(summary.failed_requests(), 0);
    assert!(summary.passed());
    let paths: Vec<&str> = summary
        .items
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    assert_eq!(paths, ["api/a", "api/f/b", "api/f/g/c", "api/d"]);
    let first = &summary.results[0];
    assert_eq!(first.status, Some(200));
    assert_eq!(first.method.as_deref(), Some("GET"));
    assert_eq!(
        first.url.as_deref(),
        Some(format!("{}/a", server.uri()).as_str())
    );
    assert_eq!(first.size, 1);
    assert_eq!(first.error, None);
}

#[test]
fn a_folder_runs_by_slug_path_or_id_and_a_collection_by_id() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    let engine = engine(dir.path());

    assert_eq!(sent_names(&run(&engine, "api/f")), ["B", "C"]);
    assert_eq!(sent_names(&run(&engine, &id_text(4))), ["B", "C"]);
    assert_eq!(sent_names(&run(&engine, &id_text(3))), ["A", "B", "C", "D"]);
}

#[test]
fn an_unknown_target_or_environment_fails_before_anything_is_sent() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    let engine = engine(dir.path());
    let mut events = 0;
    let error = engine
        .run_collection(
            "api/nope",
            &RunOptions::default(),
            &CancellationToken::new(),
            None,
            &mut |_| events += 1,
        )
        .err();
    assert!(
        matches!(&error, Some(ExecuteError::UnknownTarget(selector)) if selector == "api/nope"),
        "{error:?}"
    );
    assert_eq!(
        error.map(|error| error.to_string()),
        Some("no collection, folder or request matches api/nope".to_owned())
    );

    let options = RunOptions {
        execute: ExecuteOptions {
            environment: Some("nope".to_owned()),
            ..ExecuteOptions::default()
        },
        ..RunOptions::default()
    };
    let error = engine
        .run_collection(
            "api",
            &options,
            &CancellationToken::new(),
            None,
            &mut |_| events += 1,
        )
        .err();
    assert!(
        matches!(error, Some(ExecuteError::UnknownEnvironment(_))),
        "{error:?}"
    );
    assert_eq!(events, 0);
    let received = rt.block_on(server.received_requests()).unwrap_or_default();
    assert!(received.is_empty(), "{received:?}");
}

#[test]
fn set_next_request_jumps_by_name_or_id_and_null_stops() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/a.test.js",
        "pm.setNextRequest('C');\n",
    );
    write(
        dir.path(),
        "collections/api/f/g/c.pre.js",
        &format!("pm.setNextRequest('{}');\n", id_text(10)),
    );
    // `a` jumps to `C`; `C` jumps back to `a` by id; the loop guard ends it.
    let engine = engine(dir.path());
    let options = RunOptions {
        max_steps: Some(5),
        ..RunOptions::default()
    };
    let (summary, _) = run_with(&engine, "api", &options, &CancellationToken::new(), None);
    assert_eq!(sent_names(&summary), ["A", "C", "A", "C", "A"]);
    assert_eq!(summary.end, RunEnd::StepLimit(5));
    assert!(!summary.passed());

    write(
        dir.path(),
        "collections/api/f/g/c.pre.js",
        "pm.setNextRequest(null);\n",
    );
    let engine = self::engine(dir.path());
    let summary = run(&engine, "api");
    assert_eq!(sent_names(&summary), ["A", "C"]);
    assert_eq!(summary.end, RunEnd::Stopped);
    assert!(summary.passed());
}

#[test]
fn set_next_request_to_an_unknown_request_ends_the_run_as_a_failure() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/a.test.js",
        "pm.setNextRequest('Nowhere');\n",
    );
    let engine = engine(dir.path());
    let summary = run(&engine, "api");
    assert_eq!(sent_names(&summary), ["A"]);
    assert_eq!(summary.end, RunEnd::UnknownNext("Nowhere".to_owned()));
    assert!(!summary.passed());
}

#[test]
fn script_writes_carry_to_the_next_request_as_current_values() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d", "abc", "dev-run"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/a.test.js",
        "pm.globals.set('token', 'abc');\npm.environment.set('who', 'dev-run');\n",
    );
    write(
        dir.path(),
        "collections/api/f/b.req.toml",
        &format!(
            "id = \"{}\"\nname = \"B\"\nmethod = \"GET\"\nurl = \"{}/{{{{token}}}}\"\n",
            id_text(11),
            server.uri()
        ),
    );
    write(
        dir.path(),
        "collections/api/f/g/c.req.toml",
        &format!(
            "id = \"{}\"\nname = \"C\"\nmethod = \"GET\"\nurl = \"{}/{{{{who}}}}\"\n",
            id_text(12),
            server.uri()
        ),
    );
    let before = common::snapshot(dir.path());
    let engine = engine(dir.path());
    let options = RunOptions {
        execute: ExecuteOptions {
            environment: Some("dev".to_owned()),
            ..ExecuteOptions::default()
        },
        ..RunOptions::default()
    };
    let (summary, _) = run_with(&engine, "api", &options, &CancellationToken::new(), None);
    let urls: Vec<String> = summary
        .results
        .iter()
        .filter_map(|result| result.url.clone())
        .collect();
    let base = server.uri();
    assert_eq!(
        urls,
        [
            format!("{base}/a"),
            format!("{base}/abc"),
            format!("{base}/dev-run"),
            format!("{base}/d"),
        ]
    );
    assert!(summary.passed(), "{summary:?}");
    assert_eq!(common::snapshot(dir.path()), before, "the run wrote files");
}

#[test]
fn events_tests_and_the_summary_count_passes_and_failures() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/a.test.js",
        "pm.test('ok', () => {});\npm.test('bad', () => { throw new Error('no'); });\n",
    );
    // `d` targets a port nothing listens on: a failed request.
    write(
        dir.path(),
        "collections/api/d.req.toml",
        &format!(
            "id = \"{}\"\nname = \"D\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:9/d\"\n",
            id_text(13)
        ),
    );
    let engine = engine(dir.path());
    let (summary, events) = run_with(
        &engine,
        "api",
        &RunOptions::default(),
        &CancellationToken::new(),
        None,
    );
    let shape: Vec<String> = events
        .iter()
        .map(|event| match event {
            RunEvent::Started { step, item } => format!("start {step} {}", item.name),
            RunEvent::Finished(result) => format!("finish {} {}", result.step, result.item.name),
        })
        .collect();
    assert_eq!(
        shape,
        [
            "start 0 A",
            "finish 0 A",
            "start 1 B",
            "finish 1 B",
            "start 2 C",
            "finish 2 C",
            "start 3 D",
            "finish 3 D",
        ]
    );
    let a = &summary.results[0];
    assert_eq!(a.tests.len(), 2);
    assert!(a.failed(), "a failing test fails the step");
    let d = &summary.results[3];
    assert_eq!(d.status, None);
    assert!(d.error.is_some());
    assert_eq!(summary.end, RunEnd::Completed);
    assert_eq!(summary.requests(), 4);
    assert_eq!(summary.failed_requests(), 1);
    assert_eq!(summary.tests_passed(), 1);
    assert_eq!(summary.tests_failed(), 1);
    assert!(!summary.passed());
}

#[test]
fn bail_stops_at_the_first_failure() {
    let rt = runtime();
    let server = server(&rt, &["a", "c", "d"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/f/b.test.js",
        "pm.test('status', () => pm.expect(pm.response.code).to.equal(200));\n",
    );
    let engine = engine(dir.path());
    let options = RunOptions {
        bail: true,
        ..RunOptions::default()
    };
    let (summary, _) = run_with(&engine, "api", &options, &CancellationToken::new(), None);
    assert_eq!(sent_names(&summary), ["A", "B"]);
    assert_eq!(summary.results[1].status, Some(404));
    assert_eq!(summary.results[1].tests[0].name, "status");
    assert_eq!(
        summary.results[1].tests[0].message.as_deref(),
        Some("expected 404 to deeply equal 200")
    );
    assert_eq!(summary.end, RunEnd::Bailed);
    assert!(!summary.passed());
}

#[test]
fn a_cancelled_run_sends_nothing_more() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    let engine = engine(dir.path());
    let cancel = CancellationToken::new();
    let mut names = Vec::new();
    let summary = engine.run_collection(
        "api",
        &RunOptions::default(),
        &cancel.clone(),
        None,
        &mut |event| {
            if let RunEvent::Finished(result) = event {
                names.push(result.item.name.clone());
                if result.item.name == "B" {
                    cancel.cancel();
                }
            }
        },
    );
    let summary = match summary {
        Ok(summary) => summary,
        Err(error) => panic!("run: {error}"),
    };
    assert_eq!(names, ["A", "B"]);
    assert_eq!(summary.end, RunEnd::Cancelled);
    assert!(!summary.passed());
    let received = rt.block_on(server.received_requests()).unwrap_or_default();
    assert_eq!(received.len(), 2);
}

#[test]
fn a_send_in_flight_is_cancelled_by_the_token() {
    let rt = runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/a"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
            .mount(&server)
            .await;
        server
    });
    let dir = tree(&server.uri());
    let engine = engine(dir.path());
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let started = std::time::Instant::now();
    let summary =
        engine.run_collection("api", &RunOptions::default(), &cancel, None, &mut |event| {
            if matches!(event, RunEvent::Started { .. }) {
                let trigger = trigger.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    trigger.cancel();
                });
            }
        });
    let summary = match summary {
        Ok(summary) => summary,
        Err(error) => panic!("run: {error}"),
    };
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(summary.end, RunEnd::Cancelled);
    assert_eq!(sent_names(&summary), ["A"]);
    assert!(summary.results[0].error.is_some());
}

#[test]
fn every_send_is_recorded_in_history_with_secrets_masked() {
    let rt = runtime();
    let server = server(&rt, &["a", "b", "c", "d"]);
    let dir = tree(&server.uri());
    write(
        dir.path(),
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\norder = [\"a\", \"f\", \"d\"]\n\n[[variables]]\nkey = \"pw\"\nsecret = true\n",
            id_text(3)
        ),
    );
    write(
        dir.path(),
        "collections/api/a.req.toml",
        &format!(
            "id = \"{}\"\nname = \"A\"\nmethod = \"GET\"\nurl = \"{}/a\"\n\n[[headers]]\nkey = \"X-Pw\"\nvalue = \"{{{{pw}}}}\"\n",
            id_text(10),
            server.uri()
        ),
    );
    // `d` fails: a failed send is recorded too.
    write(
        dir.path(),
        "collections/api/d.req.toml",
        &format!(
            "id = \"{}\"\nname = \"D\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:9/d\"\n",
            id_text(13)
        ),
    );
    let state_dir = temp_dir();
    let mut state = match AppState::open(&state_dir.path().join("state.sqlite3")) {
        Ok(state) => state,
        Err(error) => panic!("state: {error}"),
    };
    let secrets = reqflux_engine::MemorySecretStore::new();
    let key = reqflux_engine::SecretKey {
        workspace_id: common::id(900),
        scope: reqflux_engine::SecretScope::Collection(common::id(3)),
        key: "pw".to_owned(),
    };
    if let Err(error) = reqflux_engine::SecretStore::set(&secrets, &key, "hunter2") {
        panic!("secret: {error}");
    }
    let engine = match Engine::open_with_secrets(dir.path(), std::sync::Arc::new(secrets)) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    let (summary, events) = run_with(
        &engine,
        "api",
        &RunOptions::default(),
        &CancellationToken::new(),
        Some(&mut state),
    );
    assert_eq!(summary.requests(), 4);
    let entries = match state.history(&id_text(900), 10) {
        Ok(entries) => entries,
        Err(error) => panic!("history: {error}"),
    };
    let paths: Vec<&str> = entries
        .iter()
        .rev()
        .map(|entry| entry.record.request_path.as_str())
        .collect();
    assert_eq!(paths, ["api/a", "api/f/b", "api/f/g/c", "api/d"]);
    let a = &entries[3].record;
    assert_eq!(a.request_id, id_text(10));
    assert_eq!(a.request_name, "A");
    assert!(
        a.sent
            .headers
            .iter()
            .any(|(name, value)| name == "X-Pw" && value == reqflux_engine::MASK),
        "{:?}",
        a.sent.headers
    );
    assert!(matches!(a.outcome, HistoryOutcome::Response(_)));
    assert!(matches!(
        entries[0].record.outcome,
        HistoryOutcome::Failed { .. }
    ));
    let debug = format!("{events:?}{summary:?}");
    assert!(!debug.contains("hunter2"), "{debug}");
}
