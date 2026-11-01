//! Rejects raw design values outside the token files.
//!
//! Colors and pixel sizes belong in `src/tokens/`, so every screen reads a named role. The
//! scan skips test files (`*tests.rs`) and each file from its first `#[cfg(test)]` line on,
//! because unit tests compare against literal values on purpose.

#![allow(clippy::panic, reason = "the helpers fail the calling test")]

use std::fs;
use std::path::{Path, PathBuf};

/// Source paths, relative to `src/`, that may hold raw values.
const TOKEN_PATHS: &[&str] = &["tokens"];

#[test]
fn source_has_no_raw_colors_or_pixels() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let offenders = scan(&src, TOKEN_PATHS);
    assert!(
        offenders.is_empty(),
        "raw design values outside the token files:\n{}",
        offenders.join("\n")
    );
}

/// Returns `file:line: text` for every offending line under `root`.
fn scan(root: &Path, allowed: &[&str]) -> Vec<String> {
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();

    let mut offenders = Vec::new();
    for file in files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        let is_test_file = relative
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("tests.rs"));
        if is_test_file || allowed.iter().any(|path| relative.starts_with(path)) {
            continue;
        }
        let text =
            fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if is_offending(line) {
                offenders.push(format!("{}:{}: {}", file.display(), index + 1, line.trim()));
            }
        }
    }
    offenders
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn is_offending(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    ["rgb(", "rgba(", "hsla(", "px("]
        .iter()
        .any(|call| has_call(code, call))
        || has_hex_color(code)
}

/// `call` appears as a bare call, not as the tail of a longer identifier.
fn has_call(code: &str, call: &str) -> bool {
    code.match_indices(call).any(|(at, _)| {
        !code[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// `0x` followed by exactly 6 or 8 hex digits.
fn has_hex_color(code: &str) -> bool {
    code.match_indices("0x").any(|(at, _)| {
        let digits = code[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '_')
            .filter(|c| *c != '_')
            .count();
        digits == 6 || digits == 8
    })
}

#[test]
fn detects_each_raw_value() {
    for line in [
        "div().bg(rgb(0x101010))",
        "let c = rgba(0x10101080);",
        "hsla(0.0, 0.0, 0.0, 1.0)",
        "const C: u32 = 0xFF6B5E;",
        "const C: u32 = 0xFF6B5E80;",
        "div().w(px(12.0))",
        "gpui::px(1.0)",
    ] {
        assert!(is_offending(line), "{line}");
    }
    for line in [
        "div().px_2()",
        "let mask = 0xFF;",
        "tokens::space::LG",
        "// rgb(0x101010) in a comment",
    ] {
        assert!(!is_offending(line), "{line}");
    }
}
