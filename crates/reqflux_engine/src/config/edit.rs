//! The format-preserving editors of the config file (app-pending S3): the interface font
//! size, the `[mcp]` rules, the `[mcp]` host and environment lists and the
//! `[network.proxy]` mode and URL. Each reads the file,
//! changes one value in place and writes it back, so comments, key order and keys this
//! build does not know survive; a file that does not parse is refused and left alone, and
//! the error never quotes it. A missing file is created with its directory.

use std::path::Path;

use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::{ConfigError, ProxyMode, Rule, clamp_font_size, parse_document, read_text, write_text};
use crate::network::NetworkSecret;
use crate::policy::ToolClass;
use crate::secrets::SecretStore;

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

/// Sets `layout`, as [`super::set_layout`] does.
pub(super) fn set_layout_value(path: &Path, name: &str) -> Result<(), ConfigError> {
    edit(path, |document| {
        set_value(document.as_table_mut(), "layout", Value::from(name));
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

/// Sets `mode` of `[network.proxy]`, creating the table (and `[network]`) when the file has
/// none. Every other key, `[network]` entry and table stays as written.
pub fn set_proxy_mode(path: &Path, mode: ProxyMode) -> Result<(), ConfigError> {
    let name = match mode {
        ProxyMode::System => "system",
        ProxyMode::None => "none",
        ProxyMode::Manual => "manual",
    };
    edit(path, |document| {
        let proxy = proxy_table(path, document)?;
        set_value(proxy, "mode", Value::from(name));
        Ok(())
    })
}

/// Sets `url` of `[network.proxy]` to `url`, trimmed; a blank `url` removes the key. The
/// proxy password is never here: it lives in the keyring, so a `url` that carries one is
/// refused with [`ConfigError::PasswordInUrl`] and nothing is written. See
/// [`set_proxy_url_with_secret`] for the caller that takes the password out first.
pub fn set_proxy_url(path: &Path, url: &str) -> Result<(), ConfigError> {
    let url = url.trim();
    if split_url(url).password.is_some() {
        return Err(ConfigError::PasswordInUrl {
            path: path.to_owned(),
        });
    }
    write_proxy_url(path, url, None)
}

/// Like [`set_proxy_url`], but a `url` with a password is split: the password goes to
/// `secrets` under [`NetworkSecret::ProxyPassword`] and the file gets the URL without it,
/// plus the user name as `username` (the engine sends the keyring password with a user
/// name). The keyring is written first and the file is checked before that, so a keyring
/// failure writes nothing; no error carries the password.
pub fn set_proxy_url_with_secret(
    path: &Path,
    url: &str,
    secrets: &dyn SecretStore,
) -> Result<(), ConfigError> {
    let url = url.trim();
    let parts = split_url(url);
    let Some(password) = parts.password else {
        return write_proxy_url(path, url, None);
    };
    let text = read_text(path)?.unwrap_or_default();
    parse_document(path, &text)?;
    if !password.is_empty() {
        let account = NetworkSecret::ProxyPassword { workspace: None }.account();
        secrets
            .set_account(&account, &password)
            .map_err(ConfigError::Keyring)?;
    }
    write_proxy_url(path, &parts.without_password, parts.username.as_deref())
}

/// `url` as typed with the `:password` of its userinfo removed, the text a front end keeps in
/// a field after the password went to the keyring or was refused. A URL without a password
/// comes back as it is.
#[must_use]
pub fn url_without_password(url: &str) -> String {
    split_url(url).without_password
}

/// Writes `url` (blank removes the key) and, when given, `username` into `[network.proxy]`.
fn write_proxy_url(path: &Path, url: &str, username: Option<&str>) -> Result<(), ConfigError> {
    edit(path, |document| {
        if url.is_empty() {
            let proxy = document
                .get_mut("network")
                .and_then(Item::as_table_like_mut)
                .and_then(|network| network.get_mut("proxy"))
                .and_then(Item::as_table_like_mut);
            if let Some(proxy) = proxy {
                proxy.remove("url");
            }
            return Ok(());
        }
        let proxy = proxy_table(path, document)?;
        set_value(proxy, "url", Value::from(url));
        if let Some(username) = username.filter(|name| !name.is_empty()) {
            set_value(proxy, "username", Value::from(username));
        }
        Ok(())
    })
}

/// A proxy URL taken apart at its userinfo.
struct UrlParts {
    /// The URL with the `:password` of its userinfo removed; the text is otherwise as typed.
    without_password: String,
    /// The user name, percent-decoded.
    username: Option<String>,
    /// The password, percent-decoded; `Some("")` for `user:@host`.
    password: Option<String>,
}

/// Takes `url` apart at the password of its userinfo. A text that is not `scheme://…` has
/// none.
fn split_url(url: &str) -> UrlParts {
    let plain = || UrlParts {
        without_password: url.to_owned(),
        username: None,
        password: None,
    };
    let Some((scheme, rest)) = url.split_once("://") else {
        return plain();
    };
    // The userinfo runs to the last `@`, so a password with an unencoded `/`, `?` or `#` is
    // still taken out whole instead of being cut at the delimiter and left in the URL.
    let Some((userinfo, after)) = rest.rsplit_once('@') else {
        return plain();
    };
    let decode = percent_decode;
    match userinfo.split_once(':') {
        Some((user, password)) => UrlParts {
            without_password: format!("{scheme}://{user}@{after}"),
            username: Some(decode(user)),
            password: Some(decode(password)),
        },
        None => UrlParts {
            username: Some(decode(userinfo)),
            ..plain()
        },
    }
}

/// `text` percent-decoded (a `+` stays a plus, as in a URL's userinfo).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (byte, hex) {
            (b'%', Some(value)) => {
                out.push(value);
                index += 3;
            }
            _ => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

/// Removes the whole `[keymap]` table: every command is back on its default key. Returns
/// whether the table was there.
pub fn clear_keymap(path: &Path) -> Result<bool, ConfigError> {
    edit(path, |document| {
        Ok(document.as_table_mut().remove("keymap").is_some())
    })
}

/// Sets `[keymap]."<slug>"` to `chord`, creating `[keymap]` at the end of the file when
/// missing (v1-features S7). Everything else in the file is kept as written; the keymap
/// table is removed again by [`remove_keymap_binding`] and [`clear_keymap`].
pub fn set_keymap_binding(path: &Path, slug: &str, chord: &str) -> Result<(), ConfigError> {
    edit(path, |document| {
        let keymap = keymap_table(path, document)?;
        set_value(keymap, slug, Value::from(chord));
        Ok(())
    })
}

/// Removes `[keymap]."<slug>"`; removing the last binding removes the `[keymap]` table,
/// so the file keeps no empty section. Returns whether the binding was there.
pub fn remove_keymap_binding(path: &Path, slug: &str) -> Result<bool, ConfigError> {
    edit(path, |document| {
        let Some(keymap) = document.get_mut("keymap").and_then(Item::as_table_like_mut) else {
            return Ok(false);
        };
        if keymap.remove(slug).is_none() {
            return Ok(false);
        }
        if keymap.is_empty() {
            document.as_table_mut().remove("keymap");
        }
        Ok(true)
    })
}

/// `[keymap]`, created at the end of the file when missing, as [`mcp_table`] does.
fn keymap_table<'a>(
    path: &Path,
    document: &'a mut DocumentMut,
) -> Result<&'a mut dyn TableLike, ConfigError> {
    let has_content = !document.as_table().is_empty();
    let item = document.entry("keymap").or_insert_with(|| {
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
            key: "keymap".to_owned(),
            expected: "a table",
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

/// `[network.proxy]`, created when missing: `[network]` stays implicit when only the proxy
/// is in it, so the file gets one `[network.proxy]` header, after what it already holds.
fn proxy_table<'a>(
    path: &Path,
    document: &'a mut DocumentMut,
) -> Result<&'a mut dyn TableLike, ConfigError> {
    let wrong_type = |key: &str| ConfigError::WrongType {
        path: path.to_owned(),
        key: key.to_owned(),
        expected: "a table",
    };
    let has_content = !document.as_table().is_empty();
    let network = document.entry("network").or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    let network = network
        .as_table_like_mut()
        .ok_or_else(|| wrong_type("network"))?;
    let leading = has_content || !network.is_empty();
    let proxy = network.entry("proxy").or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        if leading {
            table.decor_mut().set_prefix("\n");
        }
        Item::Table(table)
    });
    proxy
        .as_table_like_mut()
        .ok_or_else(|| wrong_type("network.proxy"))
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
