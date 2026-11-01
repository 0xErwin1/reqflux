use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

use super::*;

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

fn strings(values: &[&str]) -> Option<Vec<String>> {
    Some(values.iter().map(|value| (*value).to_owned()).collect())
}

#[test]
fn the_path_prefers_an_absolute_xdg_config_home() {
    assert_eq!(
        config_path_in(Some(Path::new("/tmp/conf")), Some(Path::new("/home/u"))),
        Some(PathBuf::from("/tmp/conf/reqflux/config.toml"))
    );
    assert_eq!(
        config_path_in(Some(Path::new("relative")), Some(Path::new("/home/u"))),
        Some(PathBuf::from("/home/u/.config/reqflux/config.toml")),
        "a relative XDG_CONFIG_HOME is ignored"
    );
}

#[test]
fn the_path_falls_back_to_home_and_needs_one() {
    assert_eq!(
        config_path_in(None, Some(Path::new("/home/u"))),
        Some(PathBuf::from("/home/u/.config/reqflux/config.toml"))
    );
    assert_eq!(config_path_in(None, None), None);
}

#[test]
fn a_missing_file_is_the_defaults() {
    let dir = temp_dir();
    let config = loaded(&dir.path().join("reqflux/config.toml"));
    assert_eq!(config, UserConfig::default());
    assert_eq!(config.theme, None);
    assert_eq!(config.mcp, McpConfig::default());
}

#[test]
fn a_full_file_parses_every_field() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(
        &path,
        r#"theme = "light"

[mcp]
read = "allow"
write = "ask"
execute = "deny"
destructive = "deny"
hosts = ["api.example.com", "*.example.com", "localhost:8080"]
environments = ["dev", "staging"]

[mcp.workspaces."01J0000000000000000000000A"]
execute = "deny"
hosts = ["api.example.com"]
environments = ["dev"]
"#,
    );
    let mut workspaces = BTreeMap::new();
    workspaces.insert(
        "01J0000000000000000000000A".to_owned(),
        McpOverride {
            execute: Some(Rule::Deny),
            hosts: strings(&["api.example.com"]),
            environments: strings(&["dev"]),
            ..McpOverride::default()
        },
    );
    assert_eq!(
        loaded(&path),
        UserConfig {
            theme: Some(ThemeMode::Light),
            font_size: None,
            mcp: McpConfig {
                read: Some(Rule::Allow),
                write: Some(Rule::Ask),
                execute: Some(Rule::Deny),
                destructive: Some(Rule::Deny),
                hosts: strings(&["api.example.com", "*.example.com", "localhost:8080"]),
                environments: strings(&["dev", "staging"]),
                workspaces,
            },
        }
    );
}

#[test]
fn every_theme_name_parses() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    for (name, mode) in [
        ("dark", ThemeMode::Dark),
        ("light", ThemeMode::Light),
        ("system", ThemeMode::System),
    ] {
        write(&path, &format!("theme = \"{name}\"\n"));
        assert_eq!(loaded(&path).theme, Some(mode));
        assert_eq!(mode.name(), name);
    }
}

fn parse_error(text: &str) -> String {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, text);
    match load(&path) {
        Ok(config) => panic!("expected a parse error, got {config:?}"),
        Err(error @ ConfigError::Parse { .. }) => {
            let shown = error.to_string();
            assert!(
                shown.contains(path.to_string_lossy().as_ref()),
                "names the path: {shown}"
            );
            shown
        }
        Err(other) => panic!("expected a parse error, got {other}"),
    }
}

#[test]
fn a_syntax_error_names_the_line_and_column_and_never_the_source() {
    let shown = parse_error("theme = \"dark\"\n\n[mcp]\ntoken = sk-live-SECRET123\n");
    assert!(shown.contains("line 4, column 9"), "{shown}");
    assert!(!shown.contains("SECRET123"), "{shown}");
    assert!(!shown.contains("token ="), "{shown}");
}

#[test]
fn a_type_error_never_quotes_the_value() {
    let shown = parse_error("[mcp]\nread = \"sk-live-SECRET123\"\n");
    assert!(shown.contains("line 2"), "{shown}");
    assert!(!shown.contains("SECRET123"), "{shown}");
}

#[test]
fn set_theme_creates_the_file_and_its_directory() {
    let dir = temp_dir();
    let path = dir.path().join("nested/reqflux/config.toml");
    if let Err(error) = set_theme(&path, ThemeMode::System) {
        panic!("set theme: {error}");
    }
    assert_eq!(read(&path), "theme = \"system\"\n");
    assert_eq!(loaded(&path).theme, Some(ThemeMode::System));
}

#[test]
fn set_theme_keeps_comments_order_and_unknown_keys() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    let before = r#"# My ReqFlux settings.
zeta = 1
theme = "dark" # changed by the app
alpha = "kept"

[mcp]
# Writes stay off.
write = "deny"

[future]
key = true
"#;
    write(&path, before);
    if let Err(error) = set_theme(&path, ThemeMode::Light) {
        panic!("set theme: {error}");
    }
    assert_eq!(read(&path), before.replace("\"dark\"", "\"light\""));
}

#[test]
fn set_theme_adds_a_missing_key_before_the_tables() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, "# Mine.\nzeta = 1\n\n[mcp]\nwrite = \"deny\"\n");
    if let Err(error) = set_theme(&path, ThemeMode::Dark) {
        panic!("set theme: {error}");
    }
    assert_eq!(
        read(&path),
        "# Mine.\nzeta = 1\ntheme = \"dark\"\n\n[mcp]\nwrite = \"deny\"\n"
    );
}

#[test]
fn set_theme_refuses_a_file_that_does_not_parse() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    let broken = "theme = \"dark\"\nnope = \n";
    write(&path, broken);
    assert!(matches!(
        set_theme(&path, ThemeMode::Light),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(read(&path), broken, "a broken file is left alone");
}

const WORKSPACE_KEY: &str = "01J0000000000000000000000W";

fn loaded_with_warnings(path: &Path) -> (UserConfig, Vec<String>) {
    match load_with_warnings(path) {
        Ok(loaded) => loaded,
        Err(error) => panic!("load: {error}"),
    }
}

#[test]
fn an_unknown_mcp_key_warns() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(
        &path,
        &format!(
            "[mcp]\nhost = [\"api.example.com\"]\nwrite = \"allow\"\n\n[mcp.workspaces.\"{WORKSPACE_KEY}\"]\nexcute = \"deny\"\n"
        ),
    );
    let (config, warnings) = loaded_with_warnings(&path);
    assert_eq!(config.mcp.write, Some(Rule::Allow));
    assert_eq!(config.mcp.hosts, None, "a misspelled key sets nothing");
    let shown = path.display();
    assert_eq!(
        warnings,
        vec![
            format!("{shown}: unknown key `host` in [mcp]; it is ignored"),
            format!(
                "{shown}: unknown key `excute` in [mcp.workspaces.\"{WORKSPACE_KEY}\"]; it is ignored"
            ),
        ]
    );

    let manifest: McpOverride = match toml::from_str("hosts = [\"a\"]\nwrtie = \"deny\"\n") {
        Ok(manifest) => manifest,
        Err(error) => panic!("parse: {error}"),
    };
    assert_eq!(
        override_warnings(&manifest, Path::new("/ws/reqflux.toml"), "[mcp]"),
        vec!["/ws/reqflux.toml: unknown key `wrtie` in [mcp]; it is ignored".to_owned()]
    );
}

#[test]
fn an_invalid_workspace_key_warns() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(&path, "[mcp.workspaces.billing]\nread = \"deny\"\n");
    let (_, warnings) = loaded_with_warnings(&path);
    assert_eq!(
        warnings,
        vec![format!(
            "{}: [mcp.workspaces.\"billing\"] is not a workspace id, so it applies to no workspace",
            path.display()
        )]
    );
}

#[test]
fn a_clean_config_has_no_warnings() {
    let dir = temp_dir();
    let path = dir.path().join("config.toml");
    write(
        &path,
        &format!(
            "theme = \"light\"\n[mcp]\nread = \"allow\"\n[mcp.workspaces.\"{WORKSPACE_KEY}\"]\nwrite = \"deny\"\n"
        ),
    );
    assert_eq!(loaded_with_warnings(&path).1, Vec::<String>::new());
}
