//! `reqflux export`, `reqflux bundle` and `reqflux docs` end to end (v1-features S6), with a
//! keyring that is not available: every run points D-Bus at a socket that does not exist,
//! so no test touches the real keyring.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;

const NO_BUS: &str = "unix:path=/nonexistent/reqflux-test-bus";
const FIXTURE: &str = "../reqflux_postman/tests/fixtures/collection-roundtrip.json";
const STAGING: &str = "../reqflux_postman/tests/fixtures/environment.json";

fn reqflux_in(args: &[&str], stdin: &str) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(args)
        .env_remove("RUST_LOG")
        .env("DBUS_SESSION_BUS_ADDRESS", NO_BUS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = child.unwrap_or_else(|error| panic!("spawn reqflux: {error}"));
    if let Some(mut input) = child.stdin.take()
        && let Err(error) = input.write_all(stdin.as_bytes())
    {
        panic!("write stdin: {error}");
    }
    child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("wait reqflux: {error}"))
}

fn reqflux(args: &[&str]) -> Output {
    reqflux_in(args, "")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn temp() -> TempDir {
    TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"))
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("non-UTF-8 path"))
}

/// A workspace imported from the round-trip fixture and the staging environment.
fn workspace() -> TempDir {
    let dir = temp();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let staging = Path::new(env!("CARGO_MANIFEST_DIR")).join(STAGING);
    let output = reqflux(&[
        "import",
        path(&fixture),
        "--workspace",
        path(dir.path()),
        "--environment",
        path(&staging),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    dir
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("not JSON ({error}): {text}"))
}

#[test]
fn export_prints_a_collection_and_names_each_secret_left_empty() {
    let dir = workspace();
    let output = reqflux(&["export", "round-trip", "--workspace", path(dir.path())]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let document = json(&text(&output.stdout));
    assert_eq!(document["info"]["name"], "Round Trip");
    assert_eq!(
        text(&output.stderr),
        "warning: secret variable api_secret is exported without its value; pass \
         --include-secrets to read it from the OS keyring\n"
    );

    let environment = reqflux(&["export", "env:staging", "--workspace", path(dir.path())]);
    assert_eq!(environment.status.code(), Some(0));
    assert_eq!(json(&text(&environment.stdout))["name"], "Staging");
}

#[test]
fn export_writes_a_file_and_never_replaces_one_without_force() {
    let dir = workspace();
    let out = temp();
    let file = out.path().join("api.postman.json");
    let args = [
        "export",
        "round-trip/users",
        "--workspace",
        path(dir.path()),
        "--output",
        path(&file),
    ];
    let output = reqflux(&args);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "");
    let written = fs::read_to_string(&file).unwrap_or_default();
    assert_eq!(json(&written)["item"][0]["name"], "Users");

    if let Err(error) = fs::write(&file, "keep") {
        panic!("write: {error}");
    }
    let again = reqflux(&args);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(
        text(&again.stderr).lines().last(),
        Some(
            format!(
                "error: {} already exists; pass --force to replace it",
                file.display()
            )
            .as_str()
        )
    );
    assert_eq!(fs::read_to_string(&file).unwrap_or_default(), "keep");

    let mut forced = args.to_vec();
    forced.push("--force");
    assert_eq!(reqflux(&forced).status.code(), Some(0));
    assert_eq!(
        json(&fs::read_to_string(&file).unwrap_or_default())["item"][0]["name"],
        "Users"
    );
}

#[test]
fn export_with_secrets_fails_when_the_keyring_cannot_be_read() {
    let dir = workspace();
    let output = reqflux(&[
        "export",
        "round-trip",
        "--workspace",
        path(dir.path()),
        "--include-secrets",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("error: secret values could not be read: "),
        "{stderr}"
    );

    let unknown = reqflux(&["export", "nope", "--workspace", path(dir.path())]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        text(&unknown.stderr),
        "error: no collection, folder, request or environment matches nope\n"
    );
}

#[test]
fn a_bundle_goes_out_and_back_in() {
    let dir = workspace();
    let out = temp();
    let file = out.path().join("api.reqflux.json");
    let output = reqflux(&[
        "bundle",
        "create",
        "round-trip",
        "--workspace",
        path(dir.path()),
        "--output",
        path(&file),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stderr),
        "warning: secret variable round-trip api_secret is not in the bundle; pass \
         --encrypt-secrets to include it\n"
    );
    assert_eq!(
        json(&fs::read_to_string(&file).unwrap_or_default())["kind"],
        "collection"
    );

    let target = temp();
    let imported = reqflux(&[
        "bundle",
        "import",
        path(&file),
        "--workspace",
        path(target.path()),
    ]);
    assert_eq!(
        imported.status.code(),
        Some(0),
        "{}",
        text(&imported.stderr)
    );
    assert_eq!(
        text(&imported.stdout),
        format!(
            "workspace {}\ncollection bundle: 1 collections, 2 folders, 6 requests, 1 examples, 0 environments\n",
            target.path().display()
        )
    );
    assert!(
        target
            .path()
            .join("collections/round-trip/collection.toml")
            .exists()
    );

    let again = reqflux(&[
        "bundle",
        "import",
        path(&file),
        "--workspace",
        path(target.path()),
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(text(&again.stderr).contains("already exists; an import never overwrites"));
}

#[test]
fn a_request_bundle_needs_into_and_encryption_needs_the_keyring() {
    let dir = workspace();
    let request = reqflux(&[
        "bundle",
        "create",
        "round-trip/login",
        "--workspace",
        path(dir.path()),
    ]);
    assert_eq!(request.status.code(), Some(0), "{}", text(&request.stderr));
    let out = temp();
    let file = out.path().join("login.json");
    if let Err(error) = fs::write(&file, &request.stdout) {
        panic!("write: {error}");
    }
    let missing = reqflux(&[
        "bundle",
        "import",
        path(&file),
        "--workspace",
        path(dir.path()),
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(
        text(&missing.stderr),
        "error: a request bundle needs a collection or folder to go into\n"
    );
    let into = reqflux(&[
        "bundle",
        "import",
        path(&file),
        "--workspace",
        path(dir.path()),
        "--into",
        "round-trip/users",
    ]);
    assert_eq!(into.status.code(), Some(0), "{}", text(&into.stderr));
    assert!(
        dir.path()
            .join("collections/round-trip/users/login.req.toml")
            .exists()
    );

    let encrypted = reqflux_in(
        &[
            "bundle",
            "create",
            "--workspace",
            path(dir.path()),
            "--encrypt-secrets",
        ],
        "a passphrase\n",
    );
    assert_eq!(encrypted.status.code(), Some(1));
    assert_eq!(text(&encrypted.stdout), "");
    assert!(
        text(&encrypted.stderr).starts_with("error: secret values could not be read or stored: "),
        "{}",
        text(&encrypted.stderr)
    );
    let empty = reqflux_in(
        &[
            "bundle",
            "create",
            "--workspace",
            path(dir.path()),
            "--encrypt-secrets",
        ],
        "\n",
    );
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(text(&empty.stderr), "error: the passphrase is empty\n");
}

#[test]
fn docs_write_markdown_and_html_into_a_directory() {
    let dir = workspace();
    let out = temp();
    let target = out.path().join("docs");
    let args = [
        "docs",
        "round-trip",
        "--workspace",
        path(dir.path()),
        "--output",
        path(&target),
    ];
    let output = reqflux(&args);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!(
            "{}\n{}\n",
            target.join("README.md").display(),
            target.join("index.html").display()
        )
    );
    let again = reqflux(&args);
    assert_eq!(again.status.code(), Some(1));
    let mut forced = args.to_vec();
    forced.push("--force");
    assert_eq!(reqflux(&forced).status.code(), Some(0));
    let unknown = reqflux(&[
        "docs",
        "env:staging",
        "--workspace",
        path(dir.path()),
        "--output",
        path(&target),
    ]);
    assert_eq!(unknown.status.code(), Some(2));
}

#[test]
fn the_new_subcommands_document_their_exit_codes() {
    for args in [
        ["export", "--help"].as_slice(),
        ["bundle", "create", "--help"].as_slice(),
        ["bundle", "import", "--help"].as_slice(),
        ["docs", "--help"].as_slice(),
    ] {
        let output = reqflux(args);
        assert_eq!(output.status.code(), Some(0));
        let help = text(&output.stdout);
        assert!(help.contains("Exit codes:"), "{args:?}: {help}");
        assert!(help.contains("--workspace"), "{args:?}: {help}");
    }
}
