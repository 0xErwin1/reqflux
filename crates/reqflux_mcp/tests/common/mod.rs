//! Policy paths for the engine tool tests: a temp config file and a temp audit log, never
//! the user's.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use reqflux_mcp::engine_tools::PolicyPaths;

/// A config file that allows every class.
pub(crate) const ALLOW_ALL: &str =
    "[mcp]\nread = \"allow\"\nwrite = \"allow\"\nexecute = \"allow\"\ndestructive = \"allow\"\n";

/// Paths under `dir` with the config file holding `config`, or no config file when `None`.
pub(crate) fn paths(dir: &Path, config: Option<&str>) -> PolicyPaths {
    let paths = PolicyPaths {
        config: dir.join("config/reqflux/config.toml"),
        audit: dir.join("state/reqflux/audit.log"),
    };
    if let Some(text) = config {
        write_config(&paths, text);
    }
    paths
}

/// Paths under `dir` whose config file allows every class.
pub(crate) fn allow_all(dir: &Path) -> PolicyPaths {
    paths(dir, Some(ALLOW_ALL))
}

/// Replaces the config file of `paths` with `text`.
pub(crate) fn write_config(paths: &PolicyPaths, text: &str) {
    std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
    std::fs::write(&paths.config, text).unwrap();
}
