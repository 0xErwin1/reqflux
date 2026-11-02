//! `reqflux run <collection|folder>` end to end (v1-features S1): the built binary runs every
//! request against a local mock server and prints a plain or JSON report with a CI exit
//! code.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NO_BUS: &str = "unix:path=/nonexistent/reqflux-test-bus";

fn id_text(n: u32) -> String {
    format!("01J{n:023}")
}

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

/// Answers `GET /ok` with 200 `ok` and `GET /gone` with 404.
fn server(runtime: &Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/gone"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        server
    })
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create {}: {error}", parent.display());
    }
    if let Err(error) = fs::write(&path, content) {
        panic!("write {}: {error}", path.display());
    }
}

/// A workspace with the environment `dev` (`path = ok`) and the collection `api` holding
/// `first` (`GET {base}/ok`) and the folder `more` with `second` (`GET {base}/{{path}}`).
fn workspace(base: &str) -> TempDir {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{}\"\nname = \"Dev\"\n\n[[variables]]\nkey = \"path\"\nvalue = \"ok\"\n",
            id_text(1)
        ),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\norder = [\"first\", \"more\"]\n",
            id_text(3)
        ),
    );
    write(
        root,
        "collections/api/more/folder.toml",
        &format!("id = \"{}\"\nname = \"More\"\n", id_text(4)),
    );
    write(
        root,
        "collections/api/first.req.toml",
        &format!(
            "id = \"{}\"\nname = \"First\"\nmethod = \"GET\"\nurl = \"{base}/ok\"\n",
            id_text(10)
        ),
    );
    write(
        root,
        "collections/api/more/second.req.toml",
        &format!(
            "id = \"{}\"\nname = \"Second\"\nmethod = \"GET\"\nurl = \"{base}/{{{{path}}}}\"\n",
            id_text(11)
        ),
    );
    write(
        root,
        "collections/api/first.test.js",
        "pm.test('is ok', () => pm.expect(pm.response.code).to.equal(200));\n",
    );
    dir
}

/// Runs `reqflux run --workspace <dir> <args>` with no keyring and the app state in a temp
/// directory, returning the output and that directory.
fn run(dir: &TempDir, args: &[&str]) -> (Output, TempDir) {
    let state = match TempDir::new() {
        Ok(state) => state,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path().to_string_lossy().into_owned();
    let output = match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(["run", "--workspace", root.as_str()])
        .args(args)
        .env_remove("RUST_LOG")
        .env("DBUS_SESSION_BUS_ADDRESS", NO_BUS)
        .env("XDG_STATE_HOME", state.path())
        .env("XDG_CONFIG_HOME", state.path())
        .env("XDG_DATA_HOME", state.path())
        .env("HOME", state.path())
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    };
    (output, state)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The report with every `<n> ms` replaced by `<t> ms`: times vary.
fn normalized(report: &str) -> String {
    let mut out = String::new();
    for line in report.lines() {
        let words: Vec<&str> = line.split(' ').collect();
        let mut fixed = Vec::new();
        for (index, word) in words.iter().enumerate() {
            let next = words.get(index + 1).copied();
            if matches!(next, Some("ms" | "ms," | "ms;"))
                && word.bytes().all(|byte| byte.is_ascii_digit())
            {
                fixed.push("<t>");
                continue;
            }
            fixed.push(word);
        }
        out.push_str(&fixed.join(" "));
        out.push('\n');
    }
    out
}

fn json(output: &Output) -> Value {
    match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(error) => panic!("report is not JSON: {error}\n{}", text(&output.stdout)),
    }
}

#[test]
fn a_collection_run_prints_one_line_per_request_and_exits_zero() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    let (output, state) = run(&dir, &["api", "--environment", "dev"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(
        normalized(&text(&output.stdout)),
        "GET First -> 200 OK in <t> ms, 2 B, tests 1/1\n\
         GET Second -> 200 OK in <t> ms, 2 B, tests 0/0\n\
         2 requests, 0 failed; tests 1 passed, 0 failed; <t> ms\n"
    );
    assert_eq!(output.status.code(), Some(0));
    let created = fs::read_dir(state.path()).map_or(0, Iterator::count);
    assert_eq!(created, 0, "reqflux run created app state");
}

#[test]
fn a_folder_runs_alone_and_env_is_still_accepted() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    let (output, _state) = run(&dir, &["api/more", "--env", "dev"]);
    assert_eq!(
        normalized(&text(&output.stdout)),
        "GET Second -> 200 OK in <t> ms, 2 B, tests 0/0\n\
         1 requests, 0 failed; tests 0 passed, 0 failed; <t> ms\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_failing_test_or_request_exits_one_and_lists_the_failures() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    // Without the environment `second` asks for `/{{path}}`, which answers 404; the test of
    // `first` fails against a 404 too once its URL points there.
    write(
        dir.path(),
        "collections/api/first.req.toml",
        &format!(
            "id = \"{}\"\nname = \"First\"\nmethod = \"GET\"\nurl = \"{}/gone\"\n",
            id_text(10),
            server.uri()
        ),
    );
    write(
        dir.path(),
        "collections/api/more/second.req.toml",
        &format!(
            "id = \"{}\"\nname = \"Second\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:9/x\"\n",
            id_text(11)
        ),
    );
    let (output, _state) = run(&dir, &["api"]);
    let stdout = normalized(&text(&output.stdout));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines[0],
        "GET First -> 404 Not Found in <t> ms, 0 B, tests 0/1"
    );
    assert!(lines[1].starts_with("  fail is ok: "), "{stdout}");
    assert!(
        lines[2].starts_with("GET Second -> error: could not connect to 127.0.0.1"),
        "{stdout}"
    );
    assert_eq!(
        lines[3],
        "2 requests, 1 failed; tests 0 passed, 1 failed; <t> ms"
    );
    assert_eq!(lines.len(), 4, "{stdout}");
    assert_eq!(output.status.code(), Some(1));

    let (bailed, _state) = run(&dir, &["api", "--bail"]);
    let stdout = normalized(&text(&bailed.stdout));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines[0],
        "GET First -> 404 Not Found in <t> ms, 0 B, tests 0/1"
    );
    assert_eq!(
        lines[2],
        "1 requests, 0 failed; tests 0 passed, 1 failed; <t> ms; stopped at the first failure (--bail)"
    );
    assert_eq!(lines.len(), 3, "{stdout}");
    assert_eq!(bailed.status.code(), Some(1));
}

#[test]
fn json_prints_a_machine_report() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    let (output, _state) = run(&dir, &["api", "--environment", "dev", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let mut report = json(&output);
    for result in report["results"].as_array_mut().into_iter().flatten() {
        result["elapsed_ms"] = Value::from(0);
        let timings = result
            .as_object_mut()
            .and_then(|result| result.remove("timings"))
            .unwrap_or_else(|| panic!("a result without timings: {result}"));
        let hops = timings.as_array().cloned().unwrap_or_default();
        assert_eq!(hops.len(), 1, "{timings}");
        let hop = &hops[0];
        // The server is an IP address: no lookup. TLS is reported inside connect.
        assert_eq!(hop["dns_ms"], Value::Null, "{hop}");
        assert_eq!(hop["tls_ms"], Value::Null, "{hop}");
        assert!(
            hop["connect_ms"].is_number() || hop["connect_ms"].is_null(),
            "{hop}"
        );
        assert!(hop["first_byte_ms"].is_number(), "{hop}");
        assert!(hop["download_ms"].is_number(), "{hop}");
    }
    report["elapsed_ms"] = Value::from(0);
    let base = server.uri();
    assert_eq!(
        report,
        serde_json::json!({
            "target": "api",
            "passed": true,
            "end": "completed",
            "requests": 2,
            "failed_requests": 0,
            "tests_passed": 1,
            "tests_failed": 0,
            "elapsed_ms": 0,
            "results": [
                {
                    "step": 0,
                    "id": id_text(10),
                    "name": "First",
                    "path": "api/first",
                    "method": "GET",
                    "url": format!("{base}/ok"),
                    "status": 200,
                    "reason": "OK",
                    "elapsed_ms": 0,
                    "size": 2,
                    "tests": [{ "name": "is ok", "passed": true, "message": null }],
                    "error": null,
                },
                {
                    "step": 1,
                    "id": id_text(11),
                    "name": "Second",
                    "path": "api/more/second",
                    "method": "GET",
                    "url": format!("{base}/ok"),
                    "status": 200,
                    "reason": "OK",
                    "elapsed_ms": 0,
                    "size": 2,
                    "tests": [],
                    "error": null,
                },
            ],
        })
    );
}

#[test]
fn json_reports_how_a_jump_loop_ended_and_exits_one() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    write(
        dir.path(),
        "collections/api/first.test.js",
        "pm.setNextRequest('First');\n",
    );
    let (output, _state) = run(&dir, &["api", "--json"]);
    let report = json(&output);
    assert_eq!(report["end"], "step_limit");
    assert_eq!(report["requests"], 1000);
    assert_eq!(report["passed"], false);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn an_unknown_target_is_a_usage_error() {
    let dir = workspace("http://127.0.0.1:9");
    let (output, _state) = run(&dir, &["api/nope"]);
    assert_eq!(
        text(&output.stderr),
        "error: no collection, folder or request matches api/nope\n"
    );
    assert_eq!(text(&output.stdout), "");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_single_request_still_writes_its_body() {
    let rt = runtime();
    let server = server(&rt);
    let dir = workspace(&server.uri());
    let (output, _state) = run(&dir, &["api/first"]);
    assert_eq!(text(&output.stdout), "ok");
    assert_eq!(output.status.code(), Some(0));
    let (json_single, _state) = run(&dir, &["api/first", "--json"]);
    assert_eq!(
        text(&json_single.stderr),
        "error: --json and --bail apply to a collection or folder run\n"
    );
    assert_eq!(json_single.status.code(), Some(2));
}

#[test]
fn run_help_documents_collection_runs() {
    let output = match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(["run", "--help"])
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    };
    let stdout = text(&output.stdout);
    for expected in [
        "<TARGET>",
        "--environment <SLUG>",
        "--bail",
        "--json",
        "pm.setNextRequest",
        "a request, test",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in\n{stdout}"
        );
    }
}
