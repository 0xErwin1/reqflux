//! Script execution through [`Engine`] against a local mock server (S7): the effective
//! scripts, their applied variable writes and the results beside the response.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::{id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::{ConsoleLevel, ExecuteOptions, VariableScope, VariableWrite};
use tokio::runtime::Runtime;
use wiremock::Mock;
use wiremock::matchers::{header, method, path};
use wiremock::{MockServer, ResponseTemplate};

/// The mock server runs on its own thread; this runtime only starts and inspects it, and
/// stays outside the engine's runtime so `execute_blocking` is not called from async code.
fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

fn server(runtime: &Runtime, mock: Mock) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        mock.mount(&server).await;
        server
    })
}

fn ok() -> ResponseTemplate {
    ResponseTemplate::new(201).set_body_string(r#"{"ok":true}"#)
}

fn manifest(root: &Path) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
}

fn collection(root: &Path, extra: &str) {
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n{extra}", id_text(1)),
    );
}

fn folder(root: &Path, slug: &str) {
    write(
        root,
        &format!("collections/api/{slug}/folder.toml"),
        &format!("id = \"{}\"\nname = \"{slug}\"\n", id_text(4)),
    );
}

fn request(root: &Path, slug: &str, in_folder: bool, url: &str) {
    request_with(root, slug, in_folder, url, "");
}

fn request_with(root: &Path, slug: &str, in_folder: bool, url: &str, extra: &str) {
    let directory = if in_folder {
        "collections/api/f/".to_owned()
    } else {
        "collections/api/".to_owned()
    };
    write(
        root,
        &format!("{directory}{slug}.req.toml"),
        &format!(
            "id = \"{}\"\nname = \"{slug}\"\nmethod = \"GET\"\nurl = \"{url}\"\n{extra}",
            id_text(2)
        ),
    );
}

fn engine(root: &Path) -> reqflux_engine::Engine {
    match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn execute(engine: &reqflux_engine::Engine, selector: &str) -> reqflux_engine::Executed {
    match engine.execute_blocking(selector, &ExecuteOptions::default()) {
        Ok(executed) => executed,
        Err(error) => panic!("execute: {error}"),
    }
}

/// Executes `selector` and finishes the execution with [`Execution::finish`], as an async
/// front end does.
fn finish(
    rt: &Runtime,
    engine: &reqflux_engine::Engine,
    selector: &str,
) -> reqflux_engine::Executed {
    let execution = match engine.execute(selector, &ExecuteOptions::default()) {
        Ok(execution) => execution,
        Err(error) => panic!("execute: {error}"),
    };
    match rt.block_on(execution.finish()) {
        Ok(executed) => executed,
        Err(error) => panic!("finish: {error}"),
    }
}

/// Blanks what two sends never share: the elapsed time and the `date` header, whose
/// one-second resolution differs whenever the two sends straddle a second boundary.
fn without_timing(executed: &mut reqflux_engine::Executed) {
    executed.response.elapsed = Duration::ZERO;
    executed.response.headers.retain(|(name, _)| name != "date");
}

/// A script that busy-loops for `millis` of wall-clock time, logging the Unix time in
/// milliseconds as it starts and as it ends, so a test can place other events inside or
/// outside the window the script held its thread.
fn busy_script(millis: u64) -> String {
    format!(
        "const start = Date.now();\nconsole.log(String(start));\nwhile (Date.now() - start < {millis}) {{}}\nconsole.log(String(Date.now()));\n"
    )
}

/// The `(start, end)` Unix milliseconds a [`busy_script`] logged, from the console of the
/// execution that ran it.
fn busy_window(executed: &reqflux_engine::Executed) -> (u128, u128) {
    let logged: Vec<u128> = executed
        .scripts
        .console
        .iter()
        .map(|entry| {
            entry
                .message
                .parse()
                .unwrap_or_else(|error| panic!("{:?}: {error}", entry.message))
        })
        .collect();
    match logged.as_slice() {
        [start, end] => (*start, *end),
        _ => panic!(
            "the busy script did not run to its end: {:?}",
            executed.scripts
        ),
    }
}

/// The current Unix time in milliseconds, on the clock `Date.now()` reads in a script.
fn unix_millis() -> u128 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => since.as_millis(),
        Err(error) => panic!("clock: {error}"),
    }
}

/// S14: finishing an execution never blocks the executor that polls it. The test script
/// busy-loops for [`BUSY_MS`] while `finish` is awaited on a current-thread runtime that
/// also runs a 10 ms ticker. A tick can only run while the script holds its thread if
/// `finish` yields, so at least one tick must land strictly inside the script's window:
/// had `finish` joined the script thread inside its future, none could, however slow the
/// machine is.
#[test]
fn finish_does_not_block_its_executor() {
    const BUSY_MS: u64 = 1000;
    const TICK: Duration = Duration::from_millis(10);
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/finished")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/finished", server.uri()));
    write(root, "collections/api/r.test.js", &busy_script(BUSY_MS));
    let engine = engine(root);
    let execution = match engine.execute("api/r", &ExecuteOptions::default()) {
        Ok(execution) => execution,
        Err(error) => panic!("execute: {error}"),
    };
    // A current-thread runtime only makes progress when `finish` yields: a blocked poll
    // freezes the ticker with it.
    let single = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    };
    let ticks = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let finished = single.block_on(async {
        let ticker = tokio::spawn({
            let ticks = Arc::clone(&ticks);
            let done = Arc::clone(&done);
            async move {
                while !done.load(Ordering::Relaxed) {
                    tokio::time::sleep(TICK).await;
                    ticks
                        .lock()
                        .unwrap_or_else(|error| panic!("{error}"))
                        .push(unix_millis());
                }
            }
        });
        let finished = match execution.finish().await {
            Ok(executed) => executed,
            Err(error) => panic!("finish: {error}"),
        };
        done.store(true, Ordering::Relaxed);
        match ticker.await {
            Ok(()) => {}
            Err(error) => panic!("ticker: {error}"),
        }
        finished
    });
    assert_eq!(finished.response.status, 201, "{:?}", finished.scripts);
    let (start, end) = busy_window(&finished);
    let ticks = ticks.lock().unwrap_or_else(|error| panic!("{error}"));
    assert!(
        ticks.iter().any(|tick| start < *tick && *tick < end),
        "no tick ran while the test script held its thread ({start}..{end}): {ticks:?}"
    );
}

/// S14: a pre-request script that runs for [`SCRIPT_MS`] on its own thread must not delay
/// another request on the engine. The scripted request signals through `/started` once
/// its script is running; only then does the other request start, and it must come back
/// before the script ends. Had it waited for the script, it could only end after it.
#[test]
fn a_slow_pre_request_script_does_not_delay_another_request() {
    const SCRIPT_MS: u64 = 3000;
    let rt = runtime();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let started_tx = Mutex::new(started_tx);
    let started = server(
        &rt,
        Mock::given(path("/started")).respond_with(move |_: &wiremock::Request| {
            let sent = started_tx
                .lock()
                .unwrap_or_else(|error| panic!("{error}"))
                .send(());
            if let Err(error) = sent {
                panic!("signal: {error}");
            }
            ResponseTemplate::new(200)
        }),
    );
    let slow = server(&rt, Mock::given(path("/slow")).respond_with(ok()));
    let quick = server(&rt, Mock::given(path("/quick")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "slow", false, &format!("{}/slow", slow.uri()));
    request(root, "quick", false, &format!("{}/quick", quick.uri()));
    // `pm.sendRequest` returns once `/started` answered, so the busy loop starts after the
    // signal and its window covers the whole quick request.
    write(
        root,
        "collections/api/slow.pre.js",
        &format!(
            "pm.sendRequest(\"{}/started\", function () {{}});\n{}",
            started.uri(),
            busy_script(SCRIPT_MS)
        ),
    );
    let engine = Arc::new(engine(root));
    let scripted = thread::spawn({
        let engine = Arc::clone(&engine);
        move || execute(&engine, "api/slow")
    });
    if let Err(error) = started_rx.recv_timeout(Duration::from_secs(30)) {
        panic!("the pre-request script never started: {error}");
    }
    let executed = execute(&engine, "api/quick");
    let quick_end = unix_millis();
    assert_eq!(executed.response.status, 201, "{:?}", executed.scripts);
    let scripted = scripted.join().unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(scripted.response.status, 201, "{:?}", scripted.scripts);
    let (_, script_end) = busy_window(&scripted);
    assert!(
        quick_end < script_end,
        "the quick request ended at {quick_end}, after the script it ran beside ended at {script_end}"
    );
}

#[test]
fn finish_returns_the_same_executed_as_execute_blocking() {
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/finished")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/finished", server.uri()));
    write(
        root,
        "collections/api/r.test.js",
        "pm.test(\"status\", function () {\n  pm.expect(pm.response.code).to.eql(201);\n});\nconsole.log(\"from the test\");\n",
    );
    let engine = engine(root);
    let mut expected = execute(&engine, "api/r");
    let mut executed = finish(&rt, &engine, "api/r");
    without_timing(&mut expected);
    without_timing(&mut executed);
    assert_eq!(executed, expected);
    assert!(executed.scripts.tests.iter().all(|test| test.passed));
}

#[test]
fn finish_keeps_a_failing_test_beside_a_successful_response() {
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/failing")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/failing", server.uri()));
    write(
        root,
        "collections/api/r.test.js",
        "pm.test(\"wrong code\", function () {\n  pm.expect(201, \"bad code\").to.eql(404);\n});\n",
    );
    let engine = engine(root);
    let mut expected = execute(&engine, "api/r");
    let mut executed = finish(&rt, &engine, "api/r");
    without_timing(&mut expected);
    without_timing(&mut executed);
    assert_eq!(executed, expected);
    assert_eq!(executed.response.status, 201);
    let test = &executed.scripts.tests[0];
    assert!(!test.passed);
    assert_eq!(
        test.message.as_deref(),
        Some("bad code: expected 201 to deeply equal 404")
    );
}

#[test]
fn a_pre_request_script_write_resolves_in_the_url() {
    let rt = runtime();
    let server = server(
        &rt,
        Mock::given(method("GET"))
            .and(path("/pre/from-script"))
            .respond_with(ok()),
    );
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(
        root,
        "r",
        false,
        &format!("{}/pre/{{{{token}}}}", server.uri()),
    );
    write(
        root,
        "collections/api/r.pre.js",
        "pm.variables.set(\"token\", \"from-script\");\n",
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    assert_eq!(executed.url, format!("{}/pre/from-script", server.uri()));
    assert_eq!(executed.response.status, 201);
}

#[test]
fn the_most_specific_pre_request_script_wins() {
    let rt = runtime();
    let server = server(&rt, Mock::given(method("GET")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    let base = server.uri();
    manifest(root);
    collection(root, "");
    write(
        root,
        "collections/api/collection.pre.js",
        "pm.collectionVariables.set(\"who\", \"collection\");\n",
    );
    folder(root, "f");
    write(
        root,
        "collections/api/f/folder.pre.js",
        "pm.collectionVariables.set(\"who\", \"folder\");\n",
    );
    // The collection-only request, the folder one and the request-script one share `who`.
    request(
        root,
        "only_collection",
        false,
        &format!("{base}/c?who={{{{who}}}}"),
    );
    request(
        root,
        "only_folder",
        true,
        &format!("{base}/f?who={{{{who}}}}"),
    );
    request(root, "with_own", true, &format!("{base}/q?who={{{{who}}}}"));
    write(
        root,
        "collections/api/f/with_own.pre.js",
        "pm.collectionVariables.set(\"who\", \"request\");\n",
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/only_collection");
    assert!(
        executed.url.ends_with("?who=collection"),
        "{}",
        executed.url
    );
    let executed = execute(&engine, "api/f/only_folder");
    assert!(executed.url.ends_with("?who=folder"), "{}", executed.url);
    let executed = execute(&engine, "api/f/with_own");
    assert!(executed.url.ends_with("?who=request"), "{}", executed.url);
}

#[test]
fn a_test_script_sees_the_response() {
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/tested")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/tested", server.uri()));
    write(
        root,
        "collections/api/r.test.js",
        "pm.test(\"status is 201\", function () {\n  pm.expect(pm.response.code).to.eql(201);\n});\npm.test(\"body ok\", function () {\n  pm.expect(pm.response.json().ok).to.eql(true);\n});\n",
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    let names: Vec<&str> = executed
        .scripts
        .tests
        .iter()
        .map(|test| test.name.as_str())
        .collect();
    assert_eq!(names, ["status is 201", "body ok"]);
    assert!(
        executed.scripts.tests.iter().all(|test| test.passed),
        "{:?}",
        executed.scripts.tests
    );
}

#[test]
fn a_failing_test_does_not_fail_the_request() {
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/failing")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/failing", server.uri()));
    write(
        root,
        "collections/api/r.test.js",
        "pm.test(\"wrong code\", function () {\n  pm.expect(201, \"bad code\").to.eql(404);\n});\n",
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    assert_eq!(executed.response.status, 201);
    let test = &executed.scripts.tests[0];
    assert!(!test.passed);
    assert_eq!(
        test.message.as_deref(),
        Some("bad code: expected 201 to deeply equal 404")
    );
}

#[test]
fn a_script_error_is_reported_in_the_outcome() {
    let rt = runtime();
    let server = server(&rt, Mock::given(path("/broken")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/broken", server.uri()));
    write(
        root,
        "collections/api/r.pre.js",
        "this is not ((javascript\n",
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    // The request still went out and came back.
    assert_eq!(executed.response.status, 201);
    assert_eq!(executed.scripts.tests, Vec::new());
    let errors: Vec<&str> = executed
        .scripts
        .console
        .iter()
        .map(|entry| entry.message.as_str())
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("pre-request script"), "{errors:?}");
}

// S13: a pre-request `pm.sendRequest` fetches a token through the host and the main request
// carries it; the send goes through the same transport, against a mock that only answers
// when the header is there.
#[test]
fn a_pre_request_send_request_fetches_a_token_the_request_carries() {
    let rt = runtime();
    let token = server(
        &rt,
        Mock::given(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"token":"abc"}"#)),
    );
    let main = server(
        &rt,
        Mock::given(method("GET"))
            .and(path("/main"))
            .and(header("authorization", "Bearer abc"))
            .respond_with(ok()),
    );
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request_with(
        root,
        "r",
        false,
        &format!("{}/main", main.uri()),
        "\n[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{token}}\"\n",
    );
    write(
        root,
        "collections/api/r.pre.js",
        &format!(
            "pm.sendRequest(\"{}/token\", function (error, response) {{\n  if (error) {{ throw error; }}\n  pm.variables.set(\"token\", response.json().token);\n}});\n",
            token.uri()
        ),
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    assert_eq!(executed.response.status, 201, "{:?}", executed.scripts);
    assert!(
        executed.scripts.console.is_empty(),
        "{:?}",
        executed.scripts
    );
}

// S13: a test script sends its own request and asserts on the JSON.
#[test]
fn a_test_script_send_request_asserts_on_the_json() {
    let rt = runtime();
    let probe = server(
        &rt,
        Mock::given(path("/probe"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#)),
    );
    let main = server(&rt, Mock::given(path("/tested")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/tested", main.uri()));
    write(
        root,
        "collections/api/r.test.js",
        &format!(
            "pm.sendRequest(\"{}/probe\", function (error, response) {{\n  pm.test(\"probe ok\", function () {{\n    if (error) {{ throw error; }}\n    pm.expect(response.json().ok).to.eql(true);\n  }});\n}});\n",
            probe.uri()
        ),
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    assert_eq!(executed.response.status, 201);
    assert_eq!(executed.scripts.tests.len(), 1);
    assert!(executed.scripts.tests[0].passed, "{:?}", executed.scripts);
    assert_eq!(executed.scripts.tests[0].name, "probe ok");
}

// S13: a send to a closed port is a callback error recorded in the console; the request
// itself still succeeds.
#[test]
fn a_send_request_to_a_closed_port_records_an_error_and_the_request_still_succeeds() {
    let rt = runtime();
    let main = server(&rt, Mock::given(path("/main")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "r", false, &format!("{}/main", main.uri()));
    // A port that was listening a moment ago and is closed now: connecting to it fails
    // immediately, without depending on which ports a machine keeps free.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|error| panic!("bind: {error}"));
    let port = closed
        .local_addr()
        .unwrap_or_else(|error| panic!("addr: {error}"))
        .port();
    drop(closed);
    write(
        root,
        "collections/api/r.pre.js",
        &format!(
            "pm.sendRequest(\"http://127.0.0.1:{port}/x\", function (error) {{\n  console.error(\"send failed: \" + error);\n}});\n"
        ),
    );
    let engine = engine(root);
    let executed = execute(&engine, "api/r");
    assert_eq!(executed.response.status, 201);
    let send_errors: Vec<&str> = executed
        .scripts
        .console
        .iter()
        .filter(|entry| entry.level == ConsoleLevel::Error)
        .map(|entry| entry.message.as_str())
        .collect();
    assert_eq!(send_errors.len(), 1, "{:?}", executed.scripts);
    assert!(
        send_errors[0].starts_with("send failed: "),
        "{send_errors:?}"
    );
}

// S14: a pre-request script that waits for its `pm.sendRequest` does not stop the engine's
// other requests: one started first is still running when this one comes back.
#[test]
fn a_pre_request_send_request_does_not_block_another_request() {
    let rt = runtime();
    let slow = server(
        &rt,
        Mock::given(path("/slow"))
            .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_millis(1500))),
    );
    let fast = server(&rt, Mock::given(path("/fast")).respond_with(ok()));
    let token = server(
        &rt,
        Mock::given(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"token":"t"}"#)),
    );
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "");
    request(root, "slow", false, &format!("{}/slow", slow.uri()));
    request(root, "fast", false, &format!("{}/fast", fast.uri()));
    write(
        root,
        "collections/api/fast.pre.js",
        &format!(
            "pm.sendRequest(\"{}/token\", function (error, response) {{\n  if (error) {{ throw error; }}\n  pm.variables.set(\"token\", response.json().token);\n}});\n",
            token.uri()
        ),
    );
    let engine = std::sync::Arc::new(engine(root));
    let other = std::thread::spawn({
        let engine = std::sync::Arc::clone(&engine);
        move || execute(&engine, "api/slow")
    });
    // The other request is in flight inside its slow response before this one starts.
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    let executed = execute(&engine, "api/fast");
    let elapsed = started.elapsed();
    assert_eq!(executed.response.status, 201, "{:?}", executed.scripts);
    assert!(
        elapsed < Duration::from_millis(1200),
        "the fast request waited {elapsed:?} for the slow one"
    );
    let other = other.join().unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(other.response.status, 201);
}

#[test]
fn pre_request_writes_do_not_persist() {
    let rt = runtime();
    let server = server(&rt, Mock::given(method("GET")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(
        root,
        "\n[[variables]]\nkey = \"who\"\nvalue = \"original\"\n",
    );
    request(
        root,
        "r",
        false,
        &format!("{}/p?who={{{{who}}}}", server.uri()),
    );
    // The script only writes while the stored value is still the original one, so a second
    // execution only reports the write again when the first one was not persisted.
    write(
        root,
        "collections/api/r.pre.js",
        "if (pm.collectionVariables.get(\"who\") === \"original\") {\n  pm.collectionVariables.set(\"who\", \"scripted\");\n}\n",
    );
    let engine = engine(root);
    let first = execute(&engine, "api/r");
    assert_eq!(
        first.scripts.writes,
        vec![VariableWrite {
            scope: VariableScope::CollectionVariables,
            key: "who".to_owned(),
            value: Some("scripted".to_owned()),
        }]
    );
    assert!(first.url.ends_with("?who=scripted"), "{}", first.url);
    let second = execute(&engine, "api/r");
    assert!(
        !second.scripts.writes.is_empty(),
        "the second execution rebuilt the scopes from the stored values"
    );
    let stored = &engine.workspace().collections[0].collection.variables;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].key, "who");
    assert_eq!(stored[0].value.as_deref(), Some("original"));
}

#[test]
fn a_secret_set_by_a_pre_request_script_is_masked_in_the_sent_headers() {
    let rt = runtime();
    let server = server(&rt, Mock::given(method("GET")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(root, "\n[[variables]]\nkey = \"token\"\nsecret = true\n");
    request_with(
        root,
        "r",
        false,
        &format!("{}/r", server.uri()),
        "\n[[headers]]\nkey = \"X-Token\"\nvalue = \"{{token}}\"\n",
    );
    write(
        root,
        "collections/api/r.pre.js",
        "pm.collectionVariables.set('token', 'fetched-tok');\n",
    );

    let executed = execute(&engine(root), "api/r");

    assert_eq!(executed.response.status, 201);
    assert!(
        executed
            .sent
            .headers
            .iter()
            .all(|(key, value)| !key.contains("fetched-tok") && !value.contains("fetched-tok")),
        "{:?}",
        executed.sent.headers
    );
}

#[test]
fn an_authorization_header_transformed_by_a_script_is_masked() {
    // QuickJS has no `btoa` and scripts do not read secret values; a credential assembled
    // by the script stands in for any transform that text matching cannot follow.
    let rt = runtime();
    let server = server(&rt, Mock::given(method("GET")).respond_with(ok()));
    let dir = temp_dir();
    let root = dir.path();
    manifest(root);
    collection(
        root,
        "\n[[variables]]\nkey = \"token\"\nvalue = \"user:pass\"\nsecret = true\n",
    );
    request_with(
        root,
        "r",
        false,
        &format!("{}/r", server.uri()),
        "\n[[headers]]\nkey = \"Authorization\"\nvalue = \"{{auth}}\"\n",
    );
    write(
        root,
        "collections/api/r.pre.js",
        "pm.variables.set('auth', 'Basic ' + ['dXNlcj', 'pwYXNz'].join(''));\n",
    );

    let executed = execute(&engine(root), "api/r");

    assert_eq!(
        executed.sent.headers,
        vec![(
            "Authorization".to_owned(),
            format!("Basic {}", reqflux_engine::MASK)
        )]
    );
}
