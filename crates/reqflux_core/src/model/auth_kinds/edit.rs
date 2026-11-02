//! Writing an edited typed view back into the `[auth]` table it was read from: only the keys
//! whose value changed are written, each under the spelling the table already has (a
//! Postman alias stays an alias), and every other key, known or not, stays as it was.

use super::{AWS_V4, Auth, OAUTH2, TypedAuth};

/// The other spellings [`Auth::typed`] reads for the canonical key `key` of `kind`.
fn aliases(kind: &str, key: &str) -> &'static [&'static str] {
    match (kind, key) {
        (AWS_V4, "access_key") => &["accessKey"],
        (AWS_V4, "secret_key") => &["secretKey"],
        (AWS_V4, "session_token") => &["sessionToken"],
        (OAUTH2, "grant_type") => &["grantType"],
        (OAUTH2, "token_url") => &["accessTokenUrl"],
        (OAUTH2, "auth_url") => &["authUrl"],
        (OAUTH2, "client_id") => &["clientId"],
        (OAUTH2, "client_secret") => &["clientSecret"],
        (OAUTH2, "refresh_token") => &["refreshToken"],
        (OAUTH2, "client_auth") => &["client_authentication"],
        (OAUTH2, "token_location") => &["addTokenTo"],
        (OAUTH2, "header_prefix") => &["headerPrefix"],
        _ => &[],
    }
}

/// The canonical table of a typed view, without its `type`.
fn canonical(typed: TypedAuth) -> toml::Table {
    match Auth::from(typed) {
        Auth::Other { fields, .. } => fields,
        _ => toml::Table::new(),
    }
}

impl Auth {
    /// This auth with its typed view replaced by `typed`. When this auth is the same typed
    /// kind and reads, only the fields `typed` changes are written, under the key spelling
    /// the table already uses, and every other key is kept; otherwise `typed` is written in
    /// the canonical keys.
    #[must_use]
    pub fn with_typed(&self, typed: TypedAuth) -> Self {
        let (Self::Other { kind, fields }, Some(Ok(before))) = (self, self.typed()) else {
            return typed.into();
        };
        if std::mem::discriminant(&before) != std::mem::discriminant(&typed) {
            return typed.into();
        }
        let before = canonical(before);
        let after = canonical(typed);
        let mut fields = fields.clone();
        let keys: Vec<&String> = before.keys().chain(after.keys()).collect();
        for key in keys {
            let value = after.get(key);
            if before.get(key) == value {
                continue;
            }
            let spelling = std::iter::once(key.as_str())
                .chain(aliases(kind, key).iter().copied())
                .find(|spelling| fields.contains_key(*spelling));
            match (spelling, value) {
                (Some(spelling), Some(value)) => {
                    fields.insert(spelling.to_owned(), value.clone());
                }
                // An emptied optional field keeps its key, written empty.
                (Some(spelling), None) => {
                    fields.insert(spelling.to_owned(), toml::Value::String(String::new()));
                }
                (None, Some(value)) => {
                    fields.insert(key.clone(), value.clone());
                }
                (None, None) => {}
            }
        }
        Self::Other {
            kind: kind.clone(),
            fields,
        }
    }
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
