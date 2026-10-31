//! S6: the workspace format version.

mod common;

use common::*;
use pretty_assertions::assert_eq;

#[test]
fn a_newer_format_fails_with_the_update_message() {
    let dir = workspace();
    write(
        dir.path(),
        "reqflux.toml",
        &format!("{}future = {{ x = 1 }}\n", manifest(2)),
    );

    assert_eq!(
        load_err(dir.path()).to_string(),
        "workspace format 2 is newer than this version of ReqFlux supports (1); update ReqFlux to open it"
    );
}

#[test]
fn a_missing_format_fails_naming_the_manifest() {
    let dir = workspace();
    write(
        dir.path(),
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\n", id_text(1)),
    );

    assert_eq!(
        load_err(dir.path()).to_string(),
        format!(
            "{}: missing or unsupported workspace format",
            path(dir.path(), "reqflux.toml").display()
        )
    );
}

#[test]
fn a_lower_or_non_integer_format_fails_naming_the_manifest() {
    for format in ["0", "\"1\""] {
        let dir = workspace();
        write(
            dir.path(),
            "reqflux.toml",
            &format!("id = \"{}\"\nname = \"W\"\nformat = {format}\n", id_text(1)),
        );

        assert_eq!(
            load_err(dir.path()).to_string(),
            format!(
                "{}: missing or unsupported workspace format",
                path(dir.path(), "reqflux.toml").display()
            )
        );
    }
}

#[test]
fn a_missing_manifest_fails_naming_it() {
    let dir = workspace();

    let message = load_err(dir.path()).to_string();

    let prefix = format!("{}: ", path(dir.path(), "reqflux.toml").display());
    assert!(message.starts_with(&prefix), "{message}");
}

#[test]
fn format_one_loads() {
    let dir = minimal_workspace();

    assert_eq!(load(dir.path()).workspace.manifest.format, 1);
}

#[test]
fn a_parse_error_names_the_line_and_column_and_never_quotes_the_file() {
    let dir = workspace();
    write(
        dir.path(),
        "reqflux.toml",
        &format!("{}secret = \"pw-SECRET\\q9\"\n", manifest(1)),
    );

    let message = load_err(dir.path()).to_string();
    let path = dir.path().join("reqflux.toml");
    let line = manifest(1).lines().count() + 1;
    assert!(
        message.starts_with(&format!("{}: line {line}, column ", path.display())),
        "{message}"
    );
    assert!(!message.contains("pw-SECRET"), "{message}");
    assert!(!message.contains('|'), "{message}");
}

#[test]
fn a_content_hash_parses_back_from_its_text() {
    let hash = reqflux_store::ContentHash::of(b"x");
    assert_eq!(hash.to_string().parse(), Ok(hash));
    assert_eq!(hash.to_string().to_uppercase().parse(), Ok(hash));
    let signed = format!("+{}", hash.to_string().chars().skip(1).collect::<String>());
    for bad in ["", "zz", signed.as_str()] {
        assert!(bad.parse::<reqflux_store::ContentHash>().is_err(), "{bad}");
    }
}

#[test]
fn a_parse_error_never_quotes_a_value_from_the_document() {
    // A string where a number belongs: serde's message quotes the string itself.
    let text = "timeout_ms = \"val-SECRET-81\"\n";
    let error = match toml::from_str::<reqflux_core::model::Settings>(text) {
        Ok(_) => panic!("a string timeout must not parse"),
        Err(error) => error,
    };
    let shown = reqflux_store::toml_error_location(text, &error);
    assert!(!shown.contains("val-SECRET-81"), "{shown}");
    assert!(shown.starts_with("line 1, column"), "{shown}");
}

#[test]
fn a_parse_error_never_quotes_an_unknown_variant() {
    // An unknown enum value: serde's message quotes it in backticks.
    let text = "read = \"val-SECRET-82\"\n";
    let error = match toml::from_str::<reqflux_core::model::McpOverride>(text) {
        Ok(_) => panic!("an unknown rule must not parse"),
        Err(error) => error,
    };
    let shown = reqflux_store::toml_error_location(text, &error);
    assert!(!shown.contains("val-SECRET-82"), "{shown}");
    assert!(
        shown.contains("`allow`"),
        "the expected values stay: {shown}"
    );
}

#[test]
fn a_parse_error_never_quotes_a_number() {
    // A number of the wrong type: serde quotes integers and floats in backticks.
    for (text, value) in [
        ("follow_redirects = 4111111111111111\n", "4111111111111111"),
        ("timeout_ms = 4111.1111\n", "4111.1111"),
    ] {
        let error = match toml::from_str::<reqflux_core::model::Settings>(text) {
            Ok(_) => panic!("a number of the wrong type must not parse"),
            Err(error) => error,
        };
        let shown = reqflux_store::toml_error_location(text, &error);
        assert!(!shown.contains(value), "{shown}");
        assert!(shown.contains("`…`"), "the value is masked: {shown}");
        assert!(shown.starts_with("line 1, column"), "{shown}");
    }
}
