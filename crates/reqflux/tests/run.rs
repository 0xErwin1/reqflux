//! `reqflux run` end to end: the built binary against a local mock server (S11, S12).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::matchers::{header, method, path, query_param};
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

fn server(runtime: &Runtime, mocks: Vec<Mock>) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        for mock in mocks {
            mock.mount(&server).await;
        }
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

/// A workspace with environments `dev` (allowed) and `prod`, and a collection `api`, named
/// `Api`, that only allows `dev` and holds the request `api/<slug>` with `request` as body.
fn workspace(manifest_extra: &str, requests: &[(&str, String)]) -> TempDir {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n{manifest_extra}",
            id_text(900)
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!("id = \"{}\"\nname = \"Dev\"\n", id_text(1)),
    );
    write(
        root,
        "environments/prod.env.toml",
        &format!("id = \"{}\"\nname = \"Prod\"\n", id_text(2)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{}\"\nname = \"Api\"\nenvironments = [\"{}\"]\n",
            id_text(3),
            id_text(1)
        ),
    );
    for (n, (slug, body)) in (10..).zip(requests) {
        write(
            root,
            &format!("collections/api/{slug}.req.toml"),
            &format!("id = \"{}\"\nname = \"{slug}\"\n{body}", id_text(n)),
        );
    }
    dir
}

fn get(url: &str) -> String {
    format!("method = \"GET\"\nurl = \"{url}\"\n")
}

fn reqflux(args: &[&str]) -> Output {
    match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(args)
        .env_remove("RUST_LOG")
        // A bus that does not exist: the Linux keyring is unavailable, so no test reads the
        // real one (keyring-secrets S8). On macOS the Keychain is still read.
        .env("DBUS_SESSION_BUS_ADDRESS", NO_BUS)
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    }
}

fn run(dir: &TempDir, args: &[&str]) -> Output {
    let root = dir.path().to_string_lossy().into_owned();
    let mut all = vec!["run", "--workspace", root.as_str()];
    all.extend_from_slice(args);
    reqflux(&all)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Asserts `line` is `<before><digits><after>`: the elapsed milliseconds vary.
fn assert_summary(line: &str, before: &str, after: &str) {
    let ms = line
        .strip_prefix(before)
        .and_then(|rest| rest.strip_suffix(after))
        .unwrap_or_else(|| panic!("summary {line:?} is not {before:?}<ms>{after:?}"));
    assert!(
        !ms.is_empty() && ms.bytes().all(|byte| byte.is_ascii_digit()),
        "summary {line:?} has no millisecond count"
    );
}

fn assert_failure(output: &Output, code: i32, stderr: &str) {
    assert_eq!(text(&output.stderr), stderr);
    assert_eq!(text(&output.stdout), "");
    assert_eq!(output.status.code(), Some(code));
}

#[test]
fn success_writes_the_raw_body_and_a_summary() {
    let rt = runtime();
    let body = vec![b'o', b'k', 0xff, 0x00, b'\n'];
    let server = server(
        &rt,
        vec![
            Mock::given(method("GET"))
                .and(path("/hello"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())),
        ],
    );
    let dir = workspace("", &[("hello", get(&format!("{}/hello", server.uri())))]);
    let output = run(&dir, &["api/hello"]);
    assert_eq!(output.stdout, body);
    assert_summary(
        &text(&output.stderr),
        &format!("GET {}/hello -> 200 OK in ", server.uri()),
        " ms, 5 B\n",
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn any_status_exits_zero_and_sizes_use_kilobytes() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(path("/missing"))
                .respond_with(ResponseTemplate::new(404).set_body_bytes(vec![b'x'; 2560])),
        ],
    );
    let dir = workspace(
        "",
        &[("missing", get(&format!("{}/missing", server.uri())))],
    );
    let output = run(&dir, &["api/missing"]);
    assert_summary(
        &text(&output.stderr),
        &format!("GET {}/missing -> 404 Not Found in ", server.uri()),
        " ms, 2.5 KB\n",
    );
    assert_eq!(output.stdout.len(), 2560);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn include_prints_the_status_line_and_headers_first() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(path("/inc")).respond_with(
                ResponseTemplate::new(201)
                    .insert_header("x-test", "one")
                    .set_body_string("body"),
            ),
        ],
    );
    let dir = workspace("", &[("inc", get(&format!("{}/inc", server.uri())))]);
    let output = run(&dir, &["api/inc", "--include"]);
    // The date header varies; mask its value and compare the rest exactly.
    let stdout: Vec<String> = text(&output.stdout)
        .split('\n')
        .map(|line| match line.strip_prefix("date: ") {
            Some(_) => "date: <date>".to_owned(),
            None => line.to_owned(),
        })
        .collect();
    assert_eq!(
        stdout.join("\n"),
        "HTTP/1.1 201 Created\nx-test: one\ncontent-type: text/plain\ncontent-length: 4\ndate: <date>\n\nbody"
    );
    assert_eq!(output.status.code(), Some(0));
    let short = run(&dir, &["api/inc", "-i"]);
    assert!(text(&short.stdout).starts_with("HTTP/1.1 201 Created\n"));
}

#[test]
fn vars_resolve_as_local_variables() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(path("/v"))
                .and(query_param("q", "a=b"))
                .and(header("authorization", "Bearer s3cret"))
                .respond_with(ResponseTemplate::new(200).set_body_string("ok")),
        ],
    );
    let request = "method = \"GET\"\nurl = \"{{base}}/v\"\n\n[[params]]\nkey = \"q\"\nvalue = \"{{q}}\"\n\n[auth]\ntype = \"bearer\"\ntoken = \"{{token}}\"\n";
    let dir = workspace("", &[("v", request.to_owned())]);
    let base = format!("base={}", server.uri());
    let output = run(
        &dir,
        &[
            "api/v",
            "--var",
            &base,
            "--var",
            "q=a=b",
            "--var",
            "token=s3cret",
        ],
    );
    assert_eq!(text(&output.stdout), "ok");
    assert_summary(
        &text(&output.stderr),
        &format!("GET {}/v?q=a%3Db -> 200 OK in ", server.uri()),
        " ms, 2 B\n",
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn warnings_come_before_the_summary() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![Mock::given(path("/w")).respond_with(ResponseTemplate::new(200))],
    );
    let request = format!(
        "method = \"GET\"\nurl = \"{}/w\"\n\n[[params]]\nkey = \"q\"\nvalue = \"{{{{a}}}}\"\n\n[[headers]]\nkey = \"X-B\"\nvalue = \"{{{{b}}}}{{{{a}}}}\"\n",
        server.uri()
    );
    let dir = workspace(
        "\n[[globals]]\nkey = \"t\"\nvalue = \"leaked\"\nsecret = true\n",
        &[("w", request)],
    );
    let output = run(&dir, &["api/w"]);
    let stderr = text(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines[..1],
        [format!(
            "warning: {}: secret variable t has a value in the workspace file; it was ignored",
            dir.path().join("reqflux.toml").display()
        )
        .as_str()]
    );
    assert_keyring_warning(lines[1]);
    assert_eq!(
        lines[2..4],
        [
            "warning: unresolved variable {{a}}",
            "warning: unresolved variable {{b}}",
        ]
    );
    assert_eq!(lines.len(), 5);
    assert!(!stderr.contains("leaked"), "{stderr}");
    assert_summary(
        &format!("{}\n", lines[4]),
        &format!("GET {}/w?q=%7B%7Ba%7D%7D -> 200 OK in ", server.uri()),
        " ms, 0 B\n",
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The `warning:` line a run prints when the keyring of the test bus cannot be read.
fn assert_keyring_warning(line: &str) {
    assert!(
        line.starts_with(
            "warning: secret variables were not read from the keyring: the OS keyring is not available"
        ),
        "{line}"
    );
}

/// Without a keyring the run still sends, `--var` still supplies the secret, and the only
/// change to the output is the keyring warning.
#[test]
fn a_secret_from_var_still_sends_without_a_keyring() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(path("/k"))
                .and(header("authorization", "Bearer abc"))
                .respond_with(ResponseTemplate::new(200).set_body_string("ok")),
        ],
    );
    let request = format!(
        "{}\n[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{t}}}}\"\n",
        get(&format!("{}/k", server.uri()))
    );
    let dir = workspace(
        "\n[[globals]]\nkey = \"t\"\nsecret = true\n",
        &[("k", request)],
    );
    let output = run(&dir, &["api/k", "--var", "t=abc"]);
    let stderr = text(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{stderr}");
    assert_keyring_warning(lines[0]);
    assert_summary(
        &format!("{}\n", lines[1]),
        &format!("GET {}/k -> 200 OK in ", server.uri()),
        " ms, 2 B\n",
    );
    assert_eq!(text(&output.stdout), "ok");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn scripts_print_tests_and_console_to_stderr() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(path("/s"))
                .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#)),
        ],
    );
    let dir = workspace("", &[("s", get("{{base}}/s"))]);
    write(
        dir.path(),
        "collections/api/s.pre.js",
        &format!(
            "pm.variables.set(\"base\", \"{}\");\nconsole.log(\"pre ran\");\n",
            server.uri()
        ),
    );
    write(
        dir.path(),
        "collections/api/s.test.js",
        "pm.test(\"status is 200\", function () {\n  pm.expect(pm.response.code).to.eql(200);\n});\npm.test(\"body ok\", function () {\n  pm.expect(pm.response.json().ok).to.eql(true);\n});\npm.test(\"always fails\", function () {\n  pm.expect(1, \"one is not two\").to.eql(2);\n});\nconsole.error(\"boom\");\n",
    );
    let output = run(&dir, &["api/s"]);
    let stderr: Vec<String> = text(&output.stderr).lines().map(str::to_owned).collect();
    let last = stderr.len() - 1;
    if stderr[..last]
        != [
            "test pass status is 200".to_owned(),
            "test pass body ok".to_owned(),
            "test fail always fails: one is not two: expected 1 to deeply equal 2".to_owned(),
            "console log: pre ran".to_owned(),
            "console error: boom".to_owned(),
        ]
    {
        panic!("reqflux stderr was:\n{}", text(&output.stderr));
    }
    assert_summary(
        &format!("{}\n", stderr[last]),
        &format!("GET {}/s -> 200 OK in ", server.uri()),
        " ms, 11 B\n",
    );
    assert_eq!(text(&output.stdout), r#"{"ok":true}"#);
    // A failing test does not change the exit code (S9).
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_transport_failure_still_prints_the_pre_request_output() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    write(
        dir.path(),
        "collections/api/r.pre.js",
        "console.log(\"pre ran\");\n",
    );
    let output = run(&dir, &["api/r"]);
    let stderr = text(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("console log: pre ran"),
        "{stderr}"
    );
    let error = lines.get(1).copied().unwrap_or_default();
    assert!(
        error.starts_with("error: could not connect to 127.0.0.1: "),
        "{stderr}"
    );
    assert_eq!(lines.len(), 2, "{stderr}");
    assert_eq!(text(&output.stdout), "");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn unknown_environment_is_a_usage_error() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    assert_failure(
        &run(&dir, &["api/r", "--env", "nope"]),
        2,
        "error: unknown environment nope\n",
    );
}

#[test]
fn environment_outside_the_allowlist_fails() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    assert_failure(
        &run(&dir, &["api/r", "--env", "prod"]),
        1,
        "error: environment prod is not allowed for collection Api\n",
    );
}

#[test]
fn unknown_request_is_a_usage_error() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    assert_failure(
        &run(&dir, &["api/nope"]),
        2,
        "error: no collection, folder or request matches api/nope\n",
    );
}

#[test]
fn var_without_equals_is_a_usage_error() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    assert_failure(
        &run(&dir, &["api/r", "--var", "novalue"]),
        2,
        "error: invalid --var novalue: expected <key>=<value>\n",
    );
}

#[test]
fn workspace_that_fails_to_load_is_a_usage_error() {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let manifest = dir.path().join("reqflux.toml");
    let output = run(&dir, &["api/r"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with(&format!("error: {}: ", manifest.display()))
            && stderr.lines().count() == 1,
        "{stderr}"
    );
}

#[test]
fn unsupported_auth_fails() {
    let request = "method = \"GET\"\nurl = \"http://127.0.0.1:1/\"\n\n[auth]\ntype = \"hawk\"\nusername = \"u\"\n";
    let dir = workspace("", &[("r", request.to_owned())]);
    assert_failure(
        &run(&dir, &["api/r"]),
        1,
        "error: auth type hawk is not supported yet\n",
    );
}

#[test]
fn connection_failure_fails() {
    let dir = workspace("", &[("r", get("http://127.0.0.1:1/"))]);
    let output = run(&dir, &["api/r"]);
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("error: could not connect to 127.0.0.1: ")
            && stderr.lines().count() == 1,
        "{stderr}"
    );
    assert_eq!(text(&output.stdout), "");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn help_lists_run() {
    let output = reqflux(&["--help"]);
    let stdout = text(&output.stdout);
    assert!(stdout.contains("run "), "{stdout}");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn run_help_describes_arguments_and_exit_codes() {
    let output = reqflux(&["run", "--help"]);
    let stdout = text(&output.stdout);
    for expected in [
        "<TARGET>",
        "--workspace <DIR>",
        "--environment <SLUG>",
        "--env",
        "--var <KEY=VALUE>",
        "-i, --include",
        "Exit codes:",
        "OS keyring",
        "  0  ",
        "  1  ",
        "  2  ",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in\n{stdout}"
        );
    }
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn mcp_exits_cleanly_when_the_client_closes_before_the_handshake() {
    // An empty stdin is the documented clean shutdown ("0 the client closed the connection").
    let output = reqflux(&["mcp"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

/// variables-editor S5: `reqflux run` never persists script writes as current values and
/// never creates or reads the app state; a later run resolves the shared value again.
#[test]
fn run_does_not_persist_script_writes_or_touch_the_app_state() {
    let rt = runtime();
    let server = server(
        &rt,
        vec![
            Mock::given(method("GET"))
                .and(path("/login"))
                .respond_with(ResponseTemplate::new(200).set_body_string("in")),
            Mock::given(method("GET"))
                .and(path("/use/shared"))
                .respond_with(ResponseTemplate::new(200).set_body_string("shared")),
        ],
    );
    let dir = workspace(
        "\n[[globals]]\nkey = \"token\"\nvalue = \"shared\"\n",
        &[
            ("login", get(&format!("{}/login", server.uri()))),
            ("use", get(&format!("{}/use/{{{{token}}}}", server.uri()))),
        ],
    );
    write(
        dir.path(),
        "collections/api/login.pre.js",
        "pm.globals.set('token', 'abc');\n",
    );
    let state = match TempDir::new() {
        Ok(state) => state,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path().to_string_lossy().into_owned();
    let run_with_state = |request: &str| match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(["run", "--workspace", root.as_str(), request])
        .env_remove("RUST_LOG")
        .env("DBUS_SESSION_BUS_ADDRESS", NO_BUS)
        .env("XDG_STATE_HOME", state.path())
        .env("HOME", state.path())
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    };

    let login = run_with_state("api/login");
    assert_eq!(text(&login.stdout), "in");
    assert_eq!(login.status.code(), Some(0));
    let used = run_with_state("api/use");
    assert_eq!(text(&used.stdout), "shared");
    assert_summary(
        &text(&used.stderr),
        &format!("GET {}/use/shared -> 200 OK in ", server.uri()),
        " ms, 6 B\n",
    );
    assert_eq!(used.status.code(), Some(0));

    let entries = match fs::read_dir(state.path()) {
        Ok(entries) => entries.count(),
        Err(error) => panic!("read state dir: {error}"),
    };
    assert_eq!(entries, 0, "reqflux run created app state");
}

#[test]
fn a_workspace_that_does_not_parse_never_echoes_its_source() {
    let dir = workspace(
        "",
        &[(
            "one",
            format!(
                "{}[auth]\ntype = \"bearer\"\ntoken = \"cli-SECRET\\q9\"\n",
                get("http://x/")
            ),
        )],
    );
    let root = dir.path().to_string_lossy().into_owned();
    for output in [
        run(&dir, &["api/one"]),
        reqflux(&["mcp", "--workspace", root.as_str()]),
    ] {
        let stderr = text(&output.stderr);
        assert!(!stderr.contains("cli-SECRET"), "{stderr}");
        assert!(stderr.contains("one.req.toml: line 7, column "), "{stderr}");
        assert_eq!(output.status.code(), Some(2), "{stderr}");
    }
}
