//! The format-preserving editors of the config file (app-pending S3): the interface font
//! size, the `[mcp]` rules and the `[mcp]` host and environment lists. Each reads the file,
//! changes one value in place and writes it back, so comments, key order and keys this
//! build does not know survive; a file that does not parse is refused and left alone, and
//! the error never quotes it. A missing file is created with its directory.

use std::path::Path;

use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::{ConfigError, Rule, clamp_font_size, parse_document, read_text, write_text};
use crate::policy::ToolClass;

/// A list of `[mcp]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpList {
    /// `hosts`: the hosts `request_execute` may reach.
    Hosts,
    /// `environments`: the environment slugs `request_execute` may use.
    Environments,
}

impl McpList {
    /// The key the file stores the list under.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Hosts => "hosts",
            Self::Environments => "environments",
        }
    }
}

/// The name the file stores `rule` as.
fn rule_name(rule: Rule) -> &'static str {
    match rule {
        Rule::Allow => "allow",
        Rule::Ask => "ask",
        Rule::Deny => "deny",
    }
}

/// Sets `font_size`, clamped as [`super::UserConfig::interface_font_size`] reads it; a
/// whole size is written as an integer.
pub fn set_font_size(path: &Path, size: f32) -> Result<(), ConfigError> {
    let size = clamp_font_size(size);
    let value = if size.fract() == 0.0 {
        // Clamped to 8..=32 and whole, so the cast is exact.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "size is whole and within 8..=32"
        )]
        let whole = size as i64;
        Value::from(whole)
    } else {
        Value::from(f64::from(size))
    };
    edit(path, |document| {
        set_value(document.as_table_mut(), "font_size", value);
        Ok(())
    })
}

/// Sets `theme`, as [`super::set_theme`] does.
pub(super) fn set_theme_value(path: &Path, name: &str) -> Result<(), ConfigError> {
    edit(path, |document| {
        set_value(document.as_table_mut(), "theme", Value::from(name));
        Ok(())
    })
}

/// Sets the `[mcp]` rule of `class`, creating `[mcp]` when the file has none.
pub fn set_rule(path: &Path, class: ToolClass, rule: Rule) -> Result<(), ConfigError> {
    edit(path, |document| {
        let mcp = mcp_table(path, document)?;
        set_value(mcp, class.name(), Value::from(rule_name(rule)));
        Ok(())
    })
}

/// Appends `entry`, trimmed, to the `[mcp]` list `list`, creating the list (and `[mcp]`)
/// when missing. Returns whether it was added: a blank entry or one already in the list is
/// not.
pub fn add_list_entry(path: &Path, list: McpList, entry: &str) -> Result<bool, ConfigError> {
    let entry = entry.trim();
    if entry.is_empty() {
        return Ok(false);
    }
    edit(path, |document| {
        let mcp = mcp_table(path, document)?;
        let Some(item) = mcp.get_mut(list.key()) else {
            mcp.insert(
                list.key(),
                toml_edit::value(Value::Array([entry].into_iter().collect())),
            );
            return Ok(true);
        };
        let array = list_array(path, list, item)?;
        if array.iter().any(|value| value.as_str() == Some(entry)) {
            return Ok(false);
        }
        array.push(entry);
        Ok(true)
    })
}

/// Removes `entry` from the `[mcp]` list `list`; removing the last entry removes the key,
/// which is any host or environment. Returns whether it was there.
pub fn remove_list_entry(path: &Path, list: McpList, entry: &str) -> Result<bool, ConfigError> {
    edit(path, |document| {
        let Some(mcp) = document.get_mut("mcp").and_then(Item::as_table_like_mut) else {
            return Ok(false);
        };
        let Some(item) = mcp.get_mut(list.key()) else {
            return Ok(false);
        };
        let array = list_array(path, list, item)?;
        let Some(index) = array.iter().position(|value| value.as_str() == Some(entry)) else {
            return Ok(false);
        };
        let removed = array.remove(index);
        if array.is_empty() {
            mcp.remove(list.key());
        } else if index == 0
            && let Some(first) = array.get_mut(0)
        {
            // The new first entry takes the removed one's leading space.
            let prefix = removed.decor().prefix().cloned().unwrap_or_default();
            first.decor_mut().set_prefix(prefix);
        }
        Ok(true)
    })
}

/// Reads the file at `path` (empty when missing), applies `change` and writes the result
/// when it differs from what was read.
fn edit<T>(
    path: &Path,
    change: impl FnOnce(&mut DocumentMut) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let text = read_text(path)?.unwrap_or_default();
    let mut document = parse_document(path, &text)?;
    let outcome = change(&mut document)?;
    let edited = document.to_string();
    if edited != text {
        write_text(path, &edited)?;
    }
    Ok(outcome)
}

/// Sets `key` of `table` to `value`, keeping the comment and spacing around a value that
/// was there; a new key goes after the table's other keys.
fn set_value(table: &mut dyn TableLike, key: &str, mut value: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(existing) => {
            *value.decor_mut() = existing.decor().clone();
            *existing = value;
        }
        None => {
            table.insert(key, toml_edit::value(value));
        }
    }
}

/// `[mcp]`, created at the end of the file when missing. An implicit `[mcp]` (only
/// `[mcp.workspaces.…]` tables) becomes explicit so the keys written into it are its own.
fn mcp_table<'a>(
    path: &Path,
    document: &'a mut DocumentMut,
) -> Result<&'a mut dyn TableLike, ConfigError> {
    let has_content = !document.as_table().is_empty();
    let item = document.entry("mcp").or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        if has_content {
            table.decor_mut().set_prefix("\n");
        }
        Item::Table(table)
    });
    if let Some(table) = item.as_table_mut() {
        table.set_implicit(false);
    }
    item.as_table_like_mut()
        .ok_or_else(|| ConfigError::WrongType {
            path: path.to_owned(),
            key: "mcp".to_owned(),
            expected: "a table",
        })
}

/// The array `item` holds, the list `list`.
fn list_array<'a>(
    path: &Path,
    list: McpList,
    item: &'a mut Item,
) -> Result<&'a mut toml_edit::Array, ConfigError> {
    item.as_array_mut().ok_or_else(|| ConfigError::WrongType {
        path: path.to_owned(),
        key: format!("mcp.{}", list.key()),
        expected: "an array",
    })
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
