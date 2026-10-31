//! The outbound host guard of an execution (user-config-mcp-policy S3, S6): a script's
//! `pm.sendRequest` to a refused host sends nothing and reports the refusal to the script,
//! and the main request is checked too.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::Path;

use common::{id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::{ConsoleLevel, ExecuteError, ExecuteOptions, HostGuard};
use tokio::runtime::Runtime;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const REFUSAL: &str = "no sends to the probe host";

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

fn server(runtime: &Runtime, at: &str) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .mount(&server)
            .await;
        server
    })
}

fn received(runtime: &Runtime, server: &MockServer) -> usize {
    runtime
        .block_on(server.received_requests())
        .map_or(0, |requests| requests.len())
}

/// A workspace with one request `api/r` to `url`, and `script` as its `stage` script
/// (`pre` or `test`).
fn workspace(root: &Path, url: &str, stage: &str, script: &str) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", id_text(1)),
    );
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\nurl = \"{url}\"\n",
            id_text(2)
        ),
    );
    write(root, &format!("collections/api/r.{stage}.js"), script);
}

/// A script that sends to `url` and logs the callback's error, or `sent` on success.
fn probe(url: &str) -> String {
    format!(
        "pm.sendRequest(\"{url}\", function (error) {{\n  if (error) {{ console.error(\"refused: \" + error); }} else {{ console.log(\"sent\"); }}\n}});\n"
    )
}

/// A guard refusing every URL on `refused`'s port.
fn guard_refusing(refused: &MockServer) -> HostGuard {
    let port = refused.address().port();
    HostGuard::new(move |_method: &str, url: &str| {
        let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
        if parsed.port() == Some(port) {
            Err(REFUSAL.to_owned())
        } else {
            Ok(())
        }
    })
}

fn options(guard: Option<HostGuard>) -> ExecuteOptions {
    ExecuteOptions {
        host_guard: guard,
        ..ExecuteOptions::default()
    }
}

fn console(executed: &reqflux_engine::Executed) -> Vec<(ConsoleLevel, String)> {
    executed
        .scripts
        .console
        .iter()
        .map(|entry| (entry.level, entry.message.clone()))
        .collect()
}

fn run(root: &Path, options: &ExecuteOptions) -> reqflux_engine::Executed {
    let engine = match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    match engine.execute_blocking("api/r", options) {
        Ok(executed) => executed,
        Err(error) => panic!("execute: {error}"),
    }
}

#[test]
fn a_pre_request_send_to_a_refused_host_sends_nothing_and_reports_the_refusal() {
    let rt = runtime();
    let probed = server(&rt, "/probe");
    let main = server(&rt, "/main");
    let dir = temp_dir();
    workspace(
        dir.path(),
        &format!("{}/main", main.uri()),
        "pre",
        &probe(&format!("{}/probe", probed.uri())),
    );
    let executed = run(dir.path(), &options(Some(guard_refusing(&probed))));
    assert_eq!(executed.response.status, 200);
    assert_eq!(received(&rt, &probed), 0, "the refused host saw nothing");
    assert_eq!(
        console(&executed),
        vec![(ConsoleLevel::Error, format!("refused: {REFUSAL}"))]
    );
}

#[test]
fn a_pre_request_send_to_an_allowed_host_still_works() {
    let rt = runtime();
    let probed = server(&rt, "/probe");
    let main = server(&rt, "/main");
    let other = server(&rt, "/unused");
    let dir = temp_dir();
    workspace(
        dir.path(),
        &format!("{}/main", main.uri()),
        "pre",
        &probe(&format!("{}/probe", probed.uri())),
    );
    let executed = run(dir.path(), &options(Some(guard_refusing(&other))));
    assert_eq!(received(&rt, &probed), 1);
    assert_eq!(
        console(&executed),
        vec![(ConsoleLevel::Log, "sent".to_owned())]
    );
}

#[test]
fn a_test_script_send_to_a_refused_host_is_guarded_too() {
    let rt = runtime();
    let probed = server(&rt, "/probe");
    let main = server(&rt, "/main");
    let dir = temp_dir();
    workspace(
        dir.path(),
        &format!("{}/main", main.uri()),
        "test",
        &probe(&format!("{}/probe", probed.uri())),
    );
    let executed = run(dir.path(), &options(Some(guard_refusing(&probed))));
    assert_eq!(received(&rt, &probed), 0);
    assert_eq!(received(&rt, &main), 1);
    assert_eq!(
        console(&executed),
        vec![(ConsoleLevel::Error, format!("refused: {REFUSAL}"))]
    );
}

#[test]
fn the_main_request_to_a_refused_host_is_not_sent() {
    let rt = runtime();
    let main = server(&rt, "/main");
    let dir = temp_dir();
    workspace(dir.path(), &format!("{}/main", main.uri()), "pre", "");
    let engine = match common::open_engine(dir.path()) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    match engine.execute_blocking("api/r", &options(Some(guard_refusing(&main)))) {
        Err(ExecuteError::HostRefused(reason)) => assert_eq!(reason, REFUSAL),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(received(&rt, &main), 0);
}

// Existing behavior: without a guard, a script's send reaches any host.
#[test]
fn without_a_guard_a_script_send_reaches_any_host() {
    let rt = runtime();
    let probed = server(&rt, "/probe");
    let main = server(&rt, "/main");
    let dir = temp_dir();
    workspace(
        dir.path(),
        &format!("{}/main", main.uri()),
        "pre",
        &probe(&format!("{}/probe", probed.uri())),
    );
    let executed = run(dir.path(), &ExecuteOptions::default());
    assert_eq!(received(&rt, &probed), 1);
    assert_eq!(
        console(&executed),
        vec![(ConsoleLevel::Log, "sent".to_owned())]
    );
}

/// A server answering `/start` with a 302 to `target`.
fn redirecting(runtime: &Runtime, target: &str) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", target))
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn a_redirect_to_a_refused_host_is_not_followed() {
    let rt = runtime();
    let landing = server(&rt, "/landing");
    let target = format!("{}/landing", landing.uri());
    let start = redirecting(&rt, &target);
    let dir = temp_dir();
    workspace(dir.path(), &format!("{}/start", start.uri()), "pre", "");
    let engine = match common::open_engine(dir.path()) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    match engine.execute_blocking("api/r", &options(Some(guard_refusing(&landing)))) {
        Err(error) => assert_eq!(
            error.to_string(),
            format!("a redirect to {target} was refused: {REFUSAL}")
        ),
        Ok(executed) => panic!("expected a refusal, got {}", executed.response.status),
    }
    assert_eq!(received(&rt, &start), 1);
    assert_eq!(received(&rt, &landing), 0, "the refused host saw nothing");
}

#[test]
fn a_script_send_redirected_to_a_refused_host_is_refused() {
    let rt = runtime();
    let landing = server(&rt, "/landing");
    let target = format!("{}/landing", landing.uri());
    let start = redirecting(&rt, &target);
    let main = server(&rt, "/main");
    let dir = temp_dir();
    workspace(
        dir.path(),
        &format!("{}/main", main.uri()),
        "pre",
        &probe(&format!("{}/start", start.uri())),
    );
    let executed = run(dir.path(), &options(Some(guard_refusing(&landing))));
    assert_eq!(received(&rt, &landing), 0);
    assert_eq!(
        console(&executed),
        vec![(
            ConsoleLevel::Error,
            format!("refused: a redirect to {target} was refused: {REFUSAL}")
        )]
    );
}

// Existing behavior: without a guard, a redirect is followed to any host.
#[test]
fn without_a_guard_a_redirect_is_followed() {
    let rt = runtime();
    let landing = server(&rt, "/landing");
    let start = redirecting(&rt, &format!("{}/landing", landing.uri()));
    let dir = temp_dir();
    workspace(dir.path(), &format!("{}/start", start.uri()), "pre", "");
    let executed = run(dir.path(), &ExecuteOptions::default());
    assert_eq!(executed.response.status, 200);
    assert_eq!(received(&rt, &landing), 1);
}
