use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;

use super::super::*;
use crate::policy::ToolClass;

fn temp_dir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

fn write(path: &Path, text: &str) {
    if let Err(error) = fs::write(path, text) {
        panic!("write {}: {error}", path.display());
    }
}

fn read(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read {}: {error}", path.display()),
    }
}

fn loaded(path: &Path) -> UserConfig {
    match load(path) {
        Ok(config) => config,
        Err(error) => panic!("load: {error}"),
    }
}

fn ok<T>(result: Result<T, ConfigError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("edit: {error}"),
    }
}

/// A file with comments, an unknown top-level key, an unknown `[mcp]` key, a workspace
/// override and an unknown table.
const MINE: &str = r#"# My ReqFlux settings.
zeta = 1
theme = "dark" # changed by the app

[mcp]
# Writes need me.
write = "ask" # keep this note
hosts = ["api.example.com", "localhost:8080"] # reachable
future_key = "kept"

[mcp.workspaces."01J0000000000000000000000W"]
execute = "deny"

[future]
key = true
"#;

#[test]
fn the_font_size_defaults_to_13_and_is_bounded() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    assert_eq!(UserConfig::default().interface_font_size(), 13.0);
    for (text, size) in [
        ("font_size = 15\n", 15.0),
        ("font_size = 14.5\n", 14.5),
        ("font_size = 2\n", MIN_FONT_SIZE),
        ("font_size = 99\n", MAX_FONT_SIZE),
        ("font_size = nan\n", DEFAULT_FONT_SIZE),
    ] {
        write(&path, text);
        assert_eq!(loaded(&path).interface_font_size(), size, "{text}");
    }
    assert_eq!((MIN_FONT_SIZE, MAX_FONT_SIZE), (8.0, 32.0));
}

#[test]
fn set_font_size_keeps_the_file_and_writes_whole_sizes_as_integers() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    ok(set_font_size(&path, 15.0));
    assert_eq!(
        read(&path),
        MINE.replace(
            "theme = \"dark\" # changed by the app\n",
            "theme = \"dark\" # changed by the app\nfont_size = 15\n"
        )
    );
    ok(set_font_size(&path, 14.5));
    assert!(read(&path).contains("font_size = 14.5\n"));
    ok(set_font_size(&path, 100.0));
    assert_eq!(loaded(&path).font_size, Some(MAX_FONT_SIZE), "clamped");
}

#[test]
fn set_font_size_creates_a_missing_file() {
    let dir = temp_dir();
    let path = dir.path().join("reqflux/config.toml");
    ok(set_font_size(&path, 16.0));
    assert_eq!(read(&path), "font_size = 16\n");
}

#[test]
fn set_rule_replaces_a_rule_keeping_its_comment() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    ok(set_rule(&path, ToolClass::Write, Rule::Deny));
    assert_eq!(
        read(&path),
        MINE.replace("write = \"ask\"", "write = \"deny\"")
    );
}

#[test]
fn set_rule_adds_a_missing_rule_to_the_mcp_table() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    ok(set_rule(&path, ToolClass::Destructive, Rule::Allow));
    assert_eq!(
        read(&path),
        MINE.replace(
            "future_key = \"kept\"\n",
            "future_key = \"kept\"\ndestructive = \"allow\"\n"
        )
    );
    let config = loaded(&path);
    assert_eq!(config.mcp.destructive, Some(Rule::Allow));
    assert_eq!(config.mcp.workspaces.len(), 1, "the override is untouched");
}

#[test]
fn set_rule_creates_the_mcp_table() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, "# Mine.\ntheme = \"light\"\n");
    for (class, rule) in [
        (ToolClass::Read, Rule::Deny),
        (ToolClass::Write, Rule::Allow),
        (ToolClass::Execute, Rule::Ask),
    ] {
        ok(set_rule(&path, class, rule));
    }
    assert_eq!(
        read(&path),
        "# Mine.\ntheme = \"light\"\n\n[mcp]\nread = \"deny\"\nwrite = \"allow\"\nexecute = \"ask\"\n"
    );
}

#[test]
fn set_rule_fills_an_implicit_mcp_table_before_its_subtables() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    let before =
        "theme = \"dark\"\n\n[mcp.workspaces.\"01J0000000000000000000000W\"]\nread = \"deny\"\n";
    write(&path, before);
    ok(set_rule(&path, ToolClass::Execute, Rule::Deny));
    let config = loaded(&path);
    assert_eq!(config.mcp.execute, Some(Rule::Deny));
    assert_eq!(
        config
            .mcp
            .workspaces
            .get("01J0000000000000000000000W")
            .and_then(|level| level.read),
        Some(Rule::Deny),
        "the override keeps its own rule: {}",
        read(&path)
    );
}

#[test]
fn adding_a_list_entry_appends_to_the_array_and_keeps_its_comment() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    assert!(ok(add_list_entry(&path, McpList::Hosts, " *.example.com ")));
    assert_eq!(
        read(&path),
        MINE.replace(
            "\"localhost:8080\"]",
            "\"localhost:8080\", \"*.example.com\"]"
        )
    );
    assert!(
        !ok(add_list_entry(&path, McpList::Hosts, "localhost:8080")),
        "an entry already there is not added twice"
    );
    assert!(
        !ok(add_list_entry(&path, McpList::Hosts, "  ")),
        "nor a blank one"
    );
    assert_eq!(
        loaded(&path).mcp.hosts,
        Some(vec![
            "api.example.com".to_owned(),
            "localhost:8080".to_owned(),
            "*.example.com".to_owned()
        ])
    );
}

#[test]
fn adding_the_first_entry_creates_the_list() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    assert!(ok(add_list_entry(&path, McpList::Environments, "dev")));
    assert_eq!(loaded(&path).mcp.environments, Some(vec!["dev".to_owned()]));
    assert!(read(&path).contains("future_key = \"kept\"\nenvironments = [\"dev\"]\n"));
}

#[test]
fn removing_entries_keeps_the_rest_and_the_last_one_removes_the_key() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, MINE);
    assert!(ok(remove_list_entry(
        &path,
        McpList::Hosts,
        "api.example.com"
    )));
    assert_eq!(
        loaded(&path).mcp.hosts,
        Some(vec!["localhost:8080".to_owned()])
    );
    assert!(read(&path).contains("# reachable"), "{}", read(&path));
    assert!(!ok(remove_list_entry(&path, McpList::Hosts, "absent")));
    assert!(ok(remove_list_entry(
        &path,
        McpList::Hosts,
        "localhost:8080"
    )));
    assert_eq!(loaded(&path).mcp.hosts, None, "no list is any host");
    assert_eq!(
        read(&path),
        MINE.replace(
            "hosts = [\"api.example.com\", \"localhost:8080\"] # reachable\n",
            ""
        )
    );
}

#[test]
fn every_editor_refuses_a_file_that_does_not_parse_and_never_quotes_it() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    let broken = "theme = \"dark\"\ntoken = sk-live-SECRET123\n";
    write(&path, broken);
    let results = [
        set_font_size(&path, 14.0),
        set_rule(&path, ToolClass::Read, Rule::Deny),
        add_list_entry(&path, McpList::Hosts, "a").map(|_| ()),
        remove_list_entry(&path, McpList::Environments, "a").map(|_| ()),
    ];
    for result in results {
        match result {
            Err(error @ ConfigError::Parse { .. }) => {
                let shown = error.to_string();
                assert!(shown.contains("line 2"), "{shown}");
                assert!(!shown.contains("SECRET123"), "{shown}");
                assert!(!shown.contains("token ="), "{shown}");
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }
    assert_eq!(read(&path), broken, "a broken file is left alone");
}

#[test]
fn a_key_of_another_type_is_refused_without_quoting_it() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    let odd = "mcp = \"sk-live-SECRET123\"\n";
    write(&path, odd);
    match set_rule(&path, ToolClass::Read, Rule::Deny) {
        Err(error @ ConfigError::WrongType { .. }) => {
            let shown = error.to_string();
            assert!(shown.ends_with("`mcp` is not a table"), "{shown}");
            assert!(!shown.contains("SECRET123"), "{shown}");
        }
        other => panic!("expected a type error, got {other:?}"),
    }
    assert_eq!(read(&path), odd);

    let odd = "[mcp]\nhosts = \"one\"\n";
    write(&path, odd);
    match add_list_entry(&path, McpList::Hosts, "two") {
        Err(error @ ConfigError::WrongType { .. }) => {
            assert!(error.to_string().ends_with("`mcp.hosts` is not an array"));
        }
        other => panic!("expected a type error, got {other:?}"),
    }
    assert_eq!(read(&path), odd);
}
