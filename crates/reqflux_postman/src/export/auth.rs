//! Auth as Postman JSON (v1-features S6): `{type, <type>: [{key, value, type}]}`.
//!
//! An auth the import brought in keeps Postman's key spellings, so it goes back as it came.
//! Digest, AWS SigV4 and OAuth 2.0 written in ReqFlux use the canonical keys; those take the
//! Postman spelling (`access_key` becomes `accessKey`, `token_url` becomes `accessTokenUrl`),
//! as do the values Postman spells differently (`grant_type = "password"`,
//! `token_location = "query"`). A field the import kept as JSON text because it was nested
//! goes back as JSON.

use reqflux_core::model::{ApiKeyLocation, Auth};
use serde_json::{Value, json};

/// The Postman `auth` of an auth, or `None` when it inherits (Postman's default).
pub(crate) fn export(auth: &Auth) -> Option<Value> {
    let (kind, rows) = match auth {
        Auth::Inherit => return None,
        Auth::None => return Some(json!({ "type": "noauth" })),
        Auth::Basic { username, password } => (
            "basic",
            vec![
                row("username", username.as_str()),
                row("password", password.as_str()),
            ],
        ),
        Auth::Bearer { token } => ("bearer", vec![row("token", token.as_str())]),
        Auth::ApiKey {
            key,
            value,
            location,
        } => (
            "apikey",
            vec![
                row("key", key.as_str()),
                row("value", value.as_str()),
                row(
                    "in",
                    match location {
                        ApiKeyLocation::Header => "header",
                        ApiKeyLocation::Query => "query",
                    },
                ),
            ],
        ),
        Auth::Other { kind, fields } => (
            kind.as_str(),
            fields
                .iter()
                .map(|(key, value)| other_row(kind, fields, key, value))
                .collect(),
        ),
    };
    let mut out = serde_json::Map::new();
    out.insert("type".to_owned(), Value::String(kind.to_owned()));
    out.insert(kind.to_owned(), Value::Array(rows));
    Some(Value::Object(out))
}

fn row(key: &str, value: impl Into<Value>) -> Value {
    let value = value.into();
    let kind = match &value {
        Value::Bool(_) => "boolean",
        Value::String(_) => "string",
        _ => "any",
    };
    json!({ "key": key, "value": value, "type": kind })
}

/// A field of another auth type, under its Postman key with its Postman value.
fn other_row(kind: &str, fields: &toml::Table, key: &str, value: &toml::Value) -> Value {
    let name = postman_key(kind, key)
        .filter(|postman| !fields.contains_key(*postman))
        .unwrap_or(key);
    row(name, postman_value(kind, key, json_value(value)))
}

/// The Postman spelling of a canonical key of digest, AWS SigV4 or OAuth 2.0.
fn postman_key(kind: &str, key: &str) -> Option<&'static str> {
    let renamed = match (kind, key) {
        ("awsv4", "access_key") => "accessKey",
        ("awsv4", "secret_key") => "secretKey",
        ("awsv4", "session_token") => "sessionToken",
        ("oauth2", "token_url") => "accessTokenUrl",
        ("oauth2", "auth_url") => "authUrl",
        ("oauth2", "client_id") => "clientId",
        ("oauth2", "client_secret") => "clientSecret",
        ("oauth2", "refresh_token") => "refreshToken",
        ("oauth2", "client_auth") => "client_authentication",
        ("oauth2", "token_location") => "addTokenTo",
        ("oauth2", "header_prefix") => "headerPrefix",
        _ => return None,
    };
    Some(renamed)
}

/// A value Postman spells differently: the password grant and a token sent in the query.
fn postman_value(kind: &str, key: &str, value: Value) -> Value {
    match (kind, key, value.as_str()) {
        ("oauth2", "grant_type" | "grantType", Some("password")) => {
            Value::String("password_credentials".to_owned())
        }
        ("oauth2", "token_location", Some("query")) => Value::String("queryParams".to_owned()),
        _ => value,
    }
}

/// A TOML field as JSON. Text holding a JSON array or object is what the import made of a
/// nested Postman value, so it goes back nested.
fn json_value(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(nested @ (Value::Array(_) | Value::Object(_))) => nested,
            _ => Value::String(text.clone()),
        },
        toml::Value::Integer(number) => Value::from(*number),
        toml::Value::Float(number) => Value::from(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        other => serde_json::to_value(other).unwrap_or_else(|_| Value::String(other.to_string())),
    }
}
