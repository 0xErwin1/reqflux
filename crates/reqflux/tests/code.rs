//! `reqflux code` end to end: the built binary on a temporary workspace (v1-features S2).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use pretty_assertions::assert_eq;
use tempfile::TempDir;

const NO_BUS: &str = "unix:path=/nonexistent/reqflux-test-bus";

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

/// A workspace whose collection `api` uses a bearer `{{token}}`, a secret global, and holds
/// the request `api/me`.
fn workspace() -> TempDir {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        "id = \"01J00000000000000000000900\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"token\"\nsecret = true\n",
    );
    write(
        root,
        "collections/api/collection.toml",
        "id = \"01J00000000000000000000003\"\nname = \"Api\"\n\n[auth]\ntype = \"bearer\"\ntoken = \"{{token}}\"\n",
    );
    write(
        root,
        "collections/api/me.req.toml",
        "id = \"01J00000000000000000000010\"\nname = \"Me\"\nmethod = \"POST\"\nurl = \"{{base}}/me\"\n\n[body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{\"a\":1}'\n",
    );
    dir
}

fn code(dir: &TempDir, args: &[&str]) -> Output {
    let root = dir.path().to_string_lossy().into_owned();
    let mut all = vec!["code", "--workspace", root.as_str()];
    all.extend_from_slice(args);
    match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(&all)
        .env_remove("RUST_LOG")
        .env("DBUS_SESSION_BUS_ADDRESS", NO_BUS)
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn code_prints_curl_by_default_with_the_inherited_auth() {
    let dir = workspace();
    let output = code(&dir, &["api/me"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(
        text(&output.stdout),
        "curl '{{base}}/me' \\\n  -H 'Content-Type: application/json' \\\n  --oauth2-bearer '{{token}}' \\\n  --data-raw '{\"a\":1}'\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn code_takes_a_language() {
    let dir = workspace();
    let output = code(&dir, &["api/me", "--lang", "fetch"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        "const response = await fetch(\"{{base}}/me\", {\n  method: \"POST\",\n  headers: {\n    \"Content-Type\": \"application/json\",\n    \"Authorization\": \"Bearer {{token}}\",\n  },\n  body: \"{\\\"a\\\":1}\",\n});\nconsole.log(await response.text());\n"
    );
    for lang in ["httpie", "python"] {
        let output = code(&dir, &["api/me", "--lang", lang]);
        assert_eq!(output.status.code(), Some(0), "{lang}");
        assert!(text(&output.stdout).contains("{{token}}"), "{lang}");
    }
}

#[test]
fn code_of_an_unknown_request_or_language_is_a_usage_error() {
    let dir = workspace();
    let output = code(&dir, &["api/nope"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert_eq!(text(&output.stderr), "error: no request matches api/nope\n");

    let output = code(&dir, &["api/me", "--lang", "ruby"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(
        text(&output.stderr).contains("ruby"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn code_help_documents_languages_and_exit_codes() {
    let output = match Command::new(env!("CARGO_BIN_EXE_reqflux"))
        .args(["code", "--help"])
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("run reqflux: {error}"),
    };
    let help = text(&output.stdout);
    for expected in [
        "curl",
        "httpie",
        "python",
        "fetch",
        "{{variables}}",
        "Exit codes",
    ] {
        assert!(help.contains(expected), "{expected} missing: {help}");
    }
}
