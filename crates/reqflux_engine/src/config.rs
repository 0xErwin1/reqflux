//! The user's configuration file (user-config-mcp-policy S1-S4):
//! `$XDG_CONFIG_HOME/reqflux/config.toml`, or `$HOME/.config/reqflux/config.toml` when the
//! variable is unset or not absolute.
//!
//! [`load`] reads it: a missing file is [`UserConfig::default`], and a file that does not parse
//! is a [`ConfigError::Parse`] naming the path, line and column, never the source line; the
//! caller then runs on the defaults and says so. [`set_theme`], [`set_font_size`],
//! [`set_rule`], [`add_list_entry`] and [`remove_list_entry`] edit it with a
//! format-preserving editor, so the user's comments, key order and keys this build does not
//! know survive.
//!
//! `font_size` is the interface font size in pixels, 13 by default and bounded to 8..=32 as
//! DBFlux bounds its interface size.
//!
//! The `[mcp]` tables are only parsed here; the policy that evaluates them lives with the
//! MCP tools.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use reqflux_core::model::Id;
pub use reqflux_core::model::{McpOverride, Rule};

mod edit;
pub use edit::{McpList, add_list_entry, remove_list_entry, set_font_size, set_rule};

/// The interface font size when the file sets none, in pixels, as DBFlux's.
pub const DEFAULT_FONT_SIZE: f32 = 13.0;
/// The smallest interface font size; a smaller `font_size` reads as this.
pub const MIN_FONT_SIZE: f32 = 8.0;
/// The largest interface font size; a larger `font_size` reads as this.
pub const MAX_FONT_SIZE: f32 = 32.0;

/// The palette mode the window starts in: `theme = "dark" | "light" | "system"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// The dark palette, the default.
    #[default]
    Dark,
    /// The light palette.
    Light,
    /// Follows the OS appearance.
    System,
}

impl ThemeMode {
    /// The name the file stores.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    /// The mode stored as `name`, or `None` for a name this build does not know.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// The parsed config file. Every field is optional; [`Default`] is a missing file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct UserConfig {
    /// `theme`; `None` when the file does not set it.
    #[serde(default)]
    pub theme: Option<ThemeMode>,
    /// `font_size`, the interface font size in pixels as written; read it through
    /// [`UserConfig::interface_font_size`], which bounds it.
    #[serde(default)]
    pub font_size: Option<f32>,
    /// `[mcp]`, the MCP policy.
    #[serde(default)]
    pub mcp: McpConfig,
}

impl UserConfig {
    /// The interface font size: `font_size` clamped to [`MIN_FONT_SIZE`]..=[`MAX_FONT_SIZE`],
    /// or [`DEFAULT_FONT_SIZE`] when it is unset or not a number.
    #[must_use]
    pub fn interface_font_size(&self) -> f32 {
        self.font_size.map_or(DEFAULT_FONT_SIZE, clamp_font_size)
    }
}

/// `size` within [`MIN_FONT_SIZE`]..=[`MAX_FONT_SIZE`]; a size that is not finite is
/// [`DEFAULT_FONT_SIZE`].
#[must_use]
pub fn clamp_font_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
    } else {
        DEFAULT_FONT_SIZE
    }
}

/// `[mcp]`: the user's MCP policy. `None` means the built-in default for that rule or list.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// The rule for read tools.
    pub read: Option<Rule>,
    /// The rule for write tools.
    pub write: Option<Rule>,
    /// The rule for execute tools.
    pub execute: Option<Rule>,
    /// The rule for destructive tools.
    pub destructive: Option<Rule>,
    /// The hosts `request_execute` may reach; `None` is any host.
    pub hosts: Option<Vec<String>>,
    /// The environment slugs `request_execute` may use; `None` is any environment.
    pub environments: Option<Vec<String>>,
    /// `[mcp.workspaces."<workspace id>"]`: restrictions for one workspace, by workspace id.
    pub workspaces: BTreeMap<String, McpOverride>,
}

impl McpConfig {
    /// The `[mcp.workspaces]` entry of the workspace `id`, with its key as written. A key
    /// matches when it is the same id in any case; a key that is not an id matches nothing.
    #[must_use]
    pub fn workspace(&self, id: &str) -> Option<(&str, &McpOverride)> {
        let id = id.parse::<Id>().ok()?;
        self.workspaces
            .iter()
            .find(|(key, _)| key.parse::<Id>().is_ok_and(|key| key == id))
            .map(|(key, level)| (key.as_str(), level))
    }
}

/// Why the config file could not be located, read, parsed or written. Every variant past
/// the first names the file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set.
    #[error("neither XDG_CONFIG_HOME nor HOME is set; cannot locate the config file")]
    NoConfigDirectory,
    /// The file exists but could not be read.
    #[error("cannot read the config file {path}: {source}")]
    Read {
        /// The config file.
        path: PathBuf,
        /// The IO failure.
        #[source]
        source: io::Error,
    },
    /// The file does not parse. `location` never quotes the file.
    #[error("the config file {path} does not parse: {location}")]
    Parse {
        /// The config file.
        path: PathBuf,
        /// `line <l>, column <c>: <message>`, from [`crate::toml_error_location`].
        location: String,
    },
    /// A key an editor must write into has another type, as `mcp = "…"`; the file is left
    /// alone. Never quotes the value.
    #[error("cannot edit the config file {path}: `{key}` is not {expected}")]
    WrongType {
        /// The config file.
        path: PathBuf,
        /// The dotted key.
        key: String,
        /// What the key must be: `a table` or `an array`.
        expected: &'static str,
    },
    /// The file or its directory could not be written.
    #[error("cannot write the config file {path}: {source}")]
    Write {
        /// The config file.
        path: PathBuf,
        /// The IO failure.
        #[source]
        source: io::Error,
    },
}

/// The config file path from the given variable values. An absolute `config_home` wins;
/// otherwise the file lives under `home`. `None` means there is no home.
#[must_use]
pub fn config_path_in(config_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let config_home = config_home.filter(|candidate| candidate.is_absolute());
    let base = match config_home {
        Some(config_home) => config_home.to_path_buf(),
        None => home?.join(".config"),
    };
    Some(base.join("reqflux/config.toml"))
}

/// The user's config file path, from `XDG_CONFIG_HOME` and `HOME`.
pub fn default_path() -> Result<PathBuf, ConfigError> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    config_path_in(config_home.as_deref(), home.as_deref()).ok_or(ConfigError::NoConfigDirectory)
}

/// Reads the config file at `path`; a missing file is the defaults.
pub fn load(path: &Path) -> Result<UserConfig, ConfigError> {
    let Some(text) = read_text(path)? else {
        return Ok(UserConfig::default());
    };
    toml::from_str(&text).map_err(|error| ConfigError::Parse {
        path: path.to_owned(),
        location: crate::toml_error_location(&text, &error),
    })
}

/// Reads the config file at `path`, as [`load`] does, with a warning for each `[mcp]` key it
/// does not know and each `[mcp.workspaces]` key that is not a workspace id. A warning names
/// the key and the file; it does not change what the file means.
pub fn load_with_warnings(path: &Path) -> Result<(UserConfig, Vec<String>), ConfigError> {
    let Some(text) = read_text(path)? else {
        return Ok((UserConfig::default(), Vec::new()));
    };
    let config: UserConfig = toml::from_str(&text).map_err(|error| ConfigError::Parse {
        path: path.to_owned(),
        location: crate::toml_error_location(&text, &error),
    })?;
    let mut warnings = Vec::new();
    if let Ok(table) = toml::from_str::<toml::Table>(&text)
        && let Some(mcp) = table.get("mcp").and_then(toml::Value::as_table)
    {
        warnings.extend(
            mcp.keys()
                .filter(|key| !MCP_KEYS.contains(&key.as_str()))
                .map(|key| unknown_key(path, key, "[mcp]")),
        );
    }
    for (key, level) in &config.mcp.workspaces {
        let table = format!("[mcp.workspaces.\"{key}\"]");
        if key.parse::<Id>().is_err() {
            warnings.push(format!(
                "{}: {table} is not a workspace id, so it applies to no workspace",
                path.display()
            ));
        }
        warnings.extend(override_warnings(level, path, &table));
    }
    Ok((config, warnings))
}

/// The keys `[mcp]` knows.
const MCP_KEYS: [&str; 7] = [
    "read",
    "write",
    "execute",
    "destructive",
    "hosts",
    "environments",
    "workspaces",
];

/// A warning for each key of `level`, the table `table` of the file `path`, that this build
/// does not know.
#[must_use]
pub fn override_warnings(level: &McpOverride, path: &Path, table: &str) -> Vec<String> {
    level
        .unknown
        .keys()
        .map(|key| unknown_key(path, key, table))
        .collect()
}

fn unknown_key(path: &Path, key: &str, table: &str) -> String {
    format!(
        "{}: unknown key `{key}` in {table}; it is ignored",
        path.display()
    )
}

/// Sets `theme` in the config file at `path`, creating the file and its directory when
/// missing. Everything else in the file is kept as written; a file that does not parse is
/// refused and left alone.
pub fn set_theme(path: &Path, mode: ThemeMode) -> Result<(), ConfigError> {
    edit::set_theme_value(path, mode.name())
}

/// The file's text, or `None` when it does not exist.
fn read_text(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

/// `text` as an editable document. The syntax is checked with the same parser [`load`] uses,
/// so the error has the same shape and never quotes the file.
fn parse_document(path: &Path, text: &str) -> Result<toml_edit::DocumentMut, ConfigError> {
    let parse_error = |location: String| ConfigError::Parse {
        path: path.to_owned(),
        location,
    };
    if let Err(error) = toml::from_str::<toml::Table>(text) {
        return Err(parse_error(crate::toml_error_location(text, &error)));
    }
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| parse_error(error.message().to_owned()))
}

fn write_text(path: &Path, text: &str) -> Result<(), ConfigError> {
    let write_error = |source| ConfigError::Write {
        path: path.to_owned(),
        source,
    };
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(write_error)?;
    }
    fs::write(path, text).map_err(write_error)
}

#[cfg(test)]
mod tests;
