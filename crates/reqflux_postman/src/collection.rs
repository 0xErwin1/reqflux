//! Collection v2.1 mapping: items to the tree (S2), requests (S3), auth (S4), scripts
//! (S5) and fresh ids with the Postman fields kept as text (S1, S7).

use std::collections::{BTreeMap, BTreeSet};

use reqflux_core::model::{
    ApiKeyLocation, Auth, Body, Collection, CollectionNode, Example, Folder, FolderNode, FormPart,
    Id, Item, KeyValue, Method, RawLanguage, Request, RequestNode, Scripts, Settings, Variable,
    postman_description,
};
use serde_json::{Map, Value};

use crate::error::PostmanError;
use crate::slug::{slug, unique};

/// The imported collection tree (S1), plus every slug it used, so an environment imported
/// with it can take a slug unique against them (S6).
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The collection node with its children and scripts.
    pub collection: CollectionNode,
    /// Every slug the import generated: the collection's and its folders' and requests'.
    pub slugs: BTreeSet<String>,
    /// The saved examples mapped from each request's `response` entries, by request id, in
    /// document order (v1-features S4). A request without any is absent.
    pub examples: BTreeMap<Id, Vec<Example>>,
}

/// What the walk over the items collects besides the tree.
#[derive(Default)]
struct Collected {
    /// Every slug generated so far.
    slugs: BTreeSet<String>,
    /// The examples by request id.
    examples: BTreeMap<Id, Vec<Example>>,
}

/// Maps a Postman collection v2.1 document to the ReqFlux model (S1).
///
/// # Errors
///
/// Fails when the document is not a collection object or an item is malformed (S8).
pub fn collection(value: &Value) -> Result<Imported, PostmanError> {
    let object = value.as_object().ok_or(PostmanError::NotAnObject)?;
    let info = object.get("info").and_then(Value::as_object);
    let name = info
        .and_then(|info| info.get("name"))
        .and_then(Value::as_str)
        .ok_or(PostmanError::MissingName)?;
    let mut all = Collected::default();
    let items = items(object.get("item"), "", &mut all)?;
    let node_slug = unique(&slug(name), &mut all.slugs);
    let node = CollectionNode {
        slug: node_slug,
        collection: Collection {
            id: Id::new(),
            name: name.to_owned(),
            description: info.and_then(|info| postman_description(info.get("description"))),
            environments: Vec::new(),
            order: order(&items),
            auth: auth(object.get("auth")),
            variables: variables(object.get("variable")),
            extensions: extensions(
                object,
                &["info", "item", "auth", "variable", "event"],
                &postman_id(info),
            ),
        },
        scripts: events(object.get("event")),
        items,
    };
    Ok(Imported {
        collection: node,
        slugs: all.slugs,
        examples: all.examples,
    })
}

/// The `_postman_id` of the collection, kept in `extensions` instead of an id (S7).
fn postman_id(info: Option<&Map<String, Value>>) -> Vec<(&'static str, Value)> {
    info.and_then(|info| info.get("_postman_id"))
        .map(|id| vec![("_postman_id", id.clone())])
        .unwrap_or_default()
}

/// Maps the `item` array of a collection or folder (S2).
fn items(
    value: Option<&Value>,
    parent: &str,
    all: &mut Collected,
) -> Result<Vec<Item>, PostmanError> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut siblings = BTreeSet::new();
    let mut out = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let path = child_path(parent, index);
        out.push(item_node(item, &path, &mut siblings, all)?);
    }
    Ok(out)
}

/// The path of the child at `index`, like `item[3]` at the root or `item[0].item[3]`
/// inside the first folder (S8).
fn child_path(parent: &str, index: usize) -> String {
    if parent.is_empty() {
        format!("item[{index}]")
    } else {
        format!("{parent}.item[{index}]")
    }
}

fn item_node(
    value: &Value,
    path: &str,
    siblings: &mut BTreeSet<String>,
    all: &mut Collected,
) -> Result<Item, PostmanError> {
    let object = value.as_object().ok_or(PostmanError::UnknownItem {
        path: path.to_owned(),
    })?;
    if object.contains_key("item") {
        folder_node(object, path, siblings, all).map(Item::Folder)
    } else if object.contains_key("request") {
        request_node(object, path, siblings, all).map(Item::Request)
    } else {
        Err(PostmanError::UnknownItem {
            path: path.to_owned(),
        })
    }
}

fn folder_node(
    object: &Map<String, Value>,
    path: &str,
    siblings: &mut BTreeSet<String>,
    all: &mut Collected,
) -> Result<FolderNode, PostmanError> {
    let name = item_name(object, path)?;
    let items = items(object.get("item"), path, all)?;
    let description = postman_description(object.get("description"));
    let has_description = description.is_some();
    Ok(FolderNode {
        slug: take_slug(name, siblings, &mut all.slugs),
        folder: Folder {
            id: Id::new(),
            name: name.to_owned(),
            description,
            order: order(&items),
            auth: auth(object.get("auth")),
            extensions: extensions(
                object,
                &consumed(&["name", "item", "auth", "event"], has_description),
                &[],
            ),
        },
        scripts: events(object.get("event")),
        items,
    })
}

/// Maps a request item; its `response` entries that map become examples in `all`, and
/// leave the Postman text only when every one maps (v1-features S4).
fn request_node(
    object: &Map<String, Value>,
    path: &str,
    siblings: &mut BTreeSet<String>,
    all: &mut Collected,
) -> Result<RequestNode, PostmanError> {
    let name = item_name(object, path)?;
    let request =
        object
            .get("request")
            .and_then(Value::as_object)
            .ok_or(PostmanError::UnknownItem {
                path: format!("{path}.request"),
            })?;
    let request_path = format!("{path}.request");
    let method = method(request, &request_path)?;
    let (url, params) = url_and_params(request, &request_path)?;
    let id = Id::new();
    // Postman puts the text on the request, sometimes on the item; the request's wins.
    let description = postman_description(request.get("description"))
        .or_else(|| postman_description(object.get("description")));
    let has_description = description.is_some();
    let (examples, unmapped) = crate::examples::responses(object.get("response"), request);
    let node = RequestNode {
        slug: take_slug(name, siblings, &mut all.slugs),
        request: Request {
            id,
            name: name.to_owned(),
            description,
            method,
            url,
            params,
            headers: key_values(request.get("header")),
            auth: auth(request.get("auth")),
            body: body(request, &request_path)?,
            settings: Settings::default(),
            extensions: {
                let mut rest = rest_map(
                    object,
                    &consumed(
                        &[
                            "name", "request", "method", "url", "header", "body", "auth", "event",
                        ],
                        has_description,
                    ),
                );
                if !examples.is_empty() {
                    if unmapped.is_empty() {
                        rest.remove("response");
                    } else {
                        rest.insert("response".to_owned(), Value::Array(unmapped));
                    }
                }
                merge_rest(
                    &mut rest,
                    request,
                    &consumed(
                        &["method", "url", "header", "body", "auth"],
                        has_description,
                    ),
                );
                postman_table(rest)
            },
        },
        scripts: events(object.get("event")),
    };
    if !examples.is_empty() {
        all.examples.insert(id, examples);
    }
    Ok(node)
}

fn item_name<'a>(object: &'a Map<String, Value>, path: &str) -> Result<&'a str, PostmanError> {
    object
        .get("name")
        .and_then(Value::as_str)
        .ok_or(PostmanError::MissingItemName {
            path: path.to_owned(),
        })
}

pub(crate) fn method(request: &Map<String, Value>, path: &str) -> Result<Method, PostmanError> {
    let verb =
        request
            .get("method")
            .and_then(Value::as_str)
            .ok_or(PostmanError::MissingMethod {
                path: path.to_owned(),
            })?;
    Method::new(verb).map_err(|_| PostmanError::BadMethod {
        path: path.to_owned(),
        verb: verb.to_owned(),
    })
}

/// The URL without its query string, and `url.query` as params (S3). The URL comes from
/// `url.raw` when present, else is assembled from `url.host` and `url.path`.
pub(crate) fn url_and_params(
    request: &Map<String, Value>,
    path: &str,
) -> Result<(String, Vec<KeyValue>), PostmanError> {
    let missing = || PostmanError::MissingUrl {
        path: path.to_owned(),
    };
    let Some(url) = request.get("url") else {
        return Err(missing());
    };
    let (base, query) = match url {
        Value::String(raw) => (strip_query(raw), None),
        Value::Object(fields) => match fields.get("raw").and_then(Value::as_str) {
            Some(raw) if !raw.is_empty() => (strip_query(raw), fields.get("query")),
            _ => {
                let host = joined(fields.get("host"), '.').ok_or_else(missing)?;
                let path_text = joined(fields.get("path"), '/').unwrap_or_default();
                let mut base = host;
                if !path_text.is_empty() {
                    base.push('/');
                    base.push_str(&path_text);
                }
                (base, fields.get("query"))
            }
        },
        _ => return Err(missing()),
    };
    if base.is_empty() {
        return Err(missing());
    }
    Ok((base, key_values(query)))
}

/// Drops the query string of a raw URL; the query lives in `params` (S3).
fn strip_query(raw: &str) -> String {
    raw.split('?').next().unwrap_or(raw).to_owned()
}

/// Joins a Postman host or path, given as a string or an array of strings.
fn joined(value: Option<&Value>, separator: char) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Array(parts)) => {
            let mut out = String::new();
            for part in parts {
                let text = part.as_str()?;
                if !out.is_empty() {
                    out.push(separator);
                }
                out.push_str(text);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Maps rows of `{key, value, description, disabled}` to key/value entries (S3).
#[must_use]
pub fn key_values(value: Option<&Value>) -> Vec<KeyValue> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|row| {
            let object = row.as_object()?;
            Some(KeyValue {
                key: text_or_empty(object.get("key")),
                value: text_or_empty(object.get("value")),
                description: optional_text(object.get("description")),
                enabled: !object
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// Maps the body of a request (S3). A mode without its payload fails (S8).
pub(crate) fn body(request: &Map<String, Value>, path: &str) -> Result<Body, PostmanError> {
    let Some(object) = request.get("body").and_then(Value::as_object) else {
        return Ok(Body::None);
    };
    let Some(mode) = object.get("mode").and_then(Value::as_str) else {
        return Ok(Body::None);
    };
    let missing = |field| PostmanError::MissingBodyPayload {
        path: path.to_owned(),
        mode: mode.to_owned(),
        field,
    };
    match mode {
        "raw" => {
            let raw = object
                .get("raw")
                .and_then(Value::as_str)
                .ok_or_else(|| missing("raw"))?;
            Ok(Body::Raw {
                language: raw_language(object),
                text: raw.to_owned(),
            })
        }
        "urlencoded" => {
            let value = object
                .get("urlencoded")
                .ok_or_else(|| missing("urlencoded"))?;
            Ok(Body::Urlencoded {
                fields: key_values(Some(value)),
            })
        }
        "formdata" => {
            let value = object.get("formdata").ok_or_else(|| missing("formdata"))?;
            Ok(Body::FormData {
                parts: form_parts(value),
            })
        }
        "graphql" => {
            let graphql = object
                .get("graphql")
                .and_then(Value::as_object)
                .ok_or_else(|| missing("graphql"))?;
            Ok(Body::Graphql {
                query: text_or_empty(graphql.get("query")),
                variables: graphql
                    .get("variables")
                    .map(variables_text)
                    .unwrap_or_default(),
            })
        }
        "file" => {
            let src = object
                .get("file")
                .and_then(|file| file.get("src"))
                .and_then(Value::as_str)
                .ok_or_else(|| missing("src"))?;
            Ok(Body::Binary {
                path: src.to_owned(),
            })
        }
        _ => Ok(Body::None),
    }
}

/// The language hint of a raw body, from the body's `options.raw.language` (S3).
fn raw_language(body: &Map<String, Value>) -> RawLanguage {
    match body
        .get("options")
        .and_then(|options| options.get("raw"))
        .and_then(|raw| raw.get("language"))
        .and_then(Value::as_str)
    {
        Some("json") => RawLanguage::Json,
        Some("xml") => RawLanguage::Xml,
        Some("html") => RawLanguage::Html,
        Some("javascript") => RawLanguage::Javascript,
        _ => RawLanguage::Text,
    }
}

fn form_parts(value: &Value) -> Vec<FormPart> {
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|row| {
            let object = row.as_object()?;
            let common = (
                text_or_empty(object.get("key")),
                optional_text(object.get("description")),
                !object
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            let part = if object.get("type").and_then(Value::as_str) == Some("file") {
                FormPart::File {
                    key: common.0,
                    path: text_or_empty(object.get("src")),
                    description: common.1,
                    enabled: common.2,
                }
            } else {
                FormPart::Text {
                    key: common.0,
                    value: text_or_empty(object.get("value")),
                    description: common.1,
                    enabled: common.2,
                }
            };
            Some(part)
        })
        .collect()
}

fn variables_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Maps the auth of a collection, folder or request (S4). Missing auth and `noauth`
/// inherit from the parent; any other type becomes `Auth::Other`.
#[must_use]
pub fn auth(value: Option<&Value>) -> Auth {
    let Some(object) = value.and_then(Value::as_object) else {
        return Auth::Inherit;
    };
    let Some(kind) = object.get("type").and_then(Value::as_str) else {
        return Auth::Inherit;
    };
    match kind {
        "noauth" | "inherit" => Auth::Inherit,
        "basic" => {
            let fields = auth_fields(object, kind);
            Auth::Basic {
                username: text_or_empty(fields.get("username")),
                password: text_or_empty(fields.get("password")),
            }
        }
        "bearer" => Auth::Bearer {
            token: text_or_empty(auth_fields(object, kind).get("token")),
        },
        "apikey" => {
            let fields = auth_fields(object, kind);
            Auth::ApiKey {
                key: text_or_empty(fields.get("key")),
                value: text_or_empty(fields.get("value")),
                location: match fields.get("in").and_then(Value::as_str) {
                    Some("query") => ApiKeyLocation::Query,
                    _ => ApiKeyLocation::Header,
                },
            }
        }
        other => Auth::Other {
            kind: other.to_owned(),
            fields: scalar_fields(&auth_fields(object, kind)),
        },
    }
}

/// Auth credentials are an object in Postman v2.0 and an array of `{key, value}` rows in
/// v2.1 exports; both normalize into a map.
fn auth_fields(object: &Map<String, Value>, kind: &str) -> Map<String, Value> {
    match object.get(kind) {
        Some(Value::Object(fields)) => fields.clone(),
        Some(Value::Array(rows)) => rows
            .iter()
            .filter_map(|row| {
                let row = row.as_object()?;
                let key = row.get("key").and_then(Value::as_str)?.to_owned();
                Some((key, row.get("value").cloned().unwrap_or(Value::Null)))
            })
            .collect(),
        _ => Map::new(),
    }
}

/// Keeps the fields of an unsupported auth type as scalars (S4); anything nested becomes
/// its JSON text so nothing is lost.
fn scalar_fields(fields: &Map<String, Value>) -> toml::Table {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let scalar = match value {
                Value::String(text) => toml::Value::String(text.clone()),
                Value::Number(number) => toml::Value::try_from(number).ok()?,
                Value::Bool(flag) => toml::Value::Boolean(*flag),
                other => toml::Value::String(serde_json::to_string(other).unwrap_or_default()),
            };
            Some((key.clone(), scalar))
        })
        .collect()
}

/// Maps `event` entries to sidecar scripts (S5): `prerequest` to the pre-request script,
/// `test` to the test script, `script.exec` lines joined with newlines.
#[must_use]
pub fn events(value: Option<&Value>) -> Scripts {
    let Some(array) = value.and_then(Value::as_array) else {
        return Scripts::default();
    };
    let mut scripts = Scripts::default();
    for event in array {
        let Some(listen) = event.get("listen").and_then(Value::as_str) else {
            continue;
        };
        let lines = event
            .get("script")
            .and_then(|script| script.get("exec"))
            .and_then(Value::as_array);
        match listen {
            "prerequest" => append_line(&mut scripts.pre_request, join_lines(lines)),
            "test" => append_line(&mut scripts.test, join_lines(lines)),
            _ => {}
        }
    }
    scripts
}

fn join_lines(lines: Option<&Vec<Value>>) -> String {
    lines
        .map(|lines| {
            lines
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn append_line(slot: &mut Option<String>, text: String) {
    if text.is_empty() {
        return;
    }
    match slot {
        Some(existing) => {
            existing.push('\n');
            existing.push_str(&text);
        }
        None => *slot = Some(text),
    }
}

/// Maps collection variables (S5); `type = "secret"` becomes a secret whose value is
/// dropped, the same rule environments follow (S6).
#[must_use]
pub fn variables(value: Option<&Value>) -> Vec<Variable> {
    row_variables(value, "disabled")
}

/// Maps environment `values` rows, which carry `enabled` instead of `disabled` (S6).
#[must_use]
pub(crate) fn environment_variables(value: Option<&Value>) -> Vec<Variable> {
    row_variables(value, "enabled")
}

fn row_variables(value: Option<&Value>, flag: &str) -> Vec<Variable> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|row| {
            let object = row.as_object()?;
            let flagged = object.get(flag).and_then(Value::as_bool);
            let enabled = if flag == "disabled" {
                !flagged.unwrap_or(false)
            } else {
                flagged.unwrap_or(true)
            };
            let secret = object.get("type").and_then(Value::as_str) == Some("secret");
            Some(Variable {
                key: text_or_empty(object.get("key")),
                value: if secret {
                    None
                } else {
                    optional_text(object.get("value"))
                },
                description: optional_text(object.get("description")),
                secret,
                enabled,
            })
        })
        .collect()
}

/// Keeps every key the mapping did not consume as JSON text under `postman` (S1, L3),
/// plus the `extra` pairs, like the collection `_postman_id` (S7). Empty when nothing is
/// left, so `extensions` stays out of the document.
pub(crate) fn extensions(
    object: &Map<String, Value>,
    consumed: &[&str],
    extra: &[(&str, Value)],
) -> toml::Table {
    let mut rest = rest_map(object, consumed);
    for (key, value) in extra {
        rest.insert((*key).to_owned(), value.clone());
    }
    postman_table(rest)
}

/// The keys an import consumed, plus `description` once it was read into the document: a
/// description that was not (blank, or not text) stays in the kept Postman text.
fn consumed<'a>(keys: &[&'a str], description_read: bool) -> Vec<&'a str> {
    let mut out = keys.to_vec();
    if description_read {
        out.push("description");
    }
    out
}

fn rest_map(object: &Map<String, Value>, consumed: &[&str]) -> Map<String, Value> {
    let mut rest = Map::new();
    for (key, value) in object {
        if !consumed.contains(&key.as_str()) {
            rest.insert(key.clone(), value.clone());
        }
    }
    rest
}

/// Merges the keys of another object, like the unknown keys inside `request`, into the
/// same `postman` payload (S1).
pub(crate) fn merge_rest(
    rest: &mut Map<String, Value>,
    object: &Map<String, Value>,
    consumed: &[&str],
) {
    for (key, value) in object {
        if !consumed.contains(&key.as_str()) {
            rest.insert(key.clone(), value.clone());
        }
    }
}

fn postman_table(rest: Map<String, Value>) -> toml::Table {
    if rest.is_empty() {
        return toml::Table::new();
    }
    let text = serde_json::to_string(&Value::Object(rest)).unwrap_or_default();
    let mut table = toml::Table::new();
    table.insert("postman".to_owned(), toml::Value::String(text));
    table
}

fn text_or_empty(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn optional_text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToOwned::to_owned)
}

/// The child slugs of a folder or collection, in document order (S2).
fn order(items: &[Item]) -> Vec<String> {
    items.iter().map(|item| item.slug().to_owned()).collect()
}

/// Slugs a name, unique among its siblings (S2) and recorded among every slug of the
/// import (S6). The collection uses one set for both, since its slug cannot collide with
/// itself.
fn take_slug(name: &str, siblings: &mut BTreeSet<String>, all: &mut BTreeSet<String>) -> String {
    let candidate = unique(&slug(name), siblings);
    all.insert(candidate.clone());
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqflux_core::model::FormPart;

    fn formdata() -> Value {
        serde_json::json!({
            "mode": "formdata",
            "formdata": [
                { "key": "note", "value": "hello", "type": "text" },
                { "key": "doc", "type": "file", "src": "/tmp/doc.pdf", "disabled": true }
            ]
        })
    }

    #[test]
    fn raw_without_options_defaults_to_text() {
        let request = serde_json::json!({
            "body": { "mode": "raw", "raw": "plain" }
        });
        assert_eq!(
            body(request.as_object().expect("object"), "item[0].request"),
            Ok(Body::Raw {
                language: RawLanguage::Text,
                text: "plain".to_owned()
            })
        );
    }

    #[test]
    fn formdata_maps_text_and_file_parts() {
        let request = serde_json::json!({ "body": formdata() });
        assert_eq!(
            body(request.as_object().expect("object"), "item[0].request"),
            Ok(Body::FormData {
                parts: vec![
                    FormPart::Text {
                        key: "note".to_owned(),
                        value: "hello".to_owned(),
                        description: None,
                        enabled: true,
                    },
                    FormPart::File {
                        key: "doc".to_owned(),
                        path: "/tmp/doc.pdf".to_owned(),
                        description: None,
                        enabled: false,
                    },
                ]
            })
        );
    }

    #[test]
    fn file_mode_takes_the_src_path_and_unknown_modes_stay_none() {
        let file = serde_json::json!({ "body": { "mode": "file", "file": { "src": "a.bin" } } });
        assert_eq!(
            body(file.as_object().expect("object"), "item[0].request"),
            Ok(Body::Binary {
                path: "a.bin".to_owned()
            })
        );
        let weird = serde_json::json!({ "body": { "mode": "aurora" } });
        assert_eq!(
            body(weird.as_object().expect("object"), "item[0].request"),
            Ok(Body::None)
        );
    }

    #[test]
    fn auth_maps_the_supported_types() {
        assert_eq!(auth(None), Auth::Inherit);
        assert_eq!(
            auth(Some(&serde_json::json!({ "type": "noauth" }))),
            Auth::Inherit
        );
        assert_eq!(
            auth(Some(&serde_json::json!({
                "type": "basic",
                "basic": [{ "key": "username", "value": "ada" }, { "key": "password", "value": "s3cret" }]
            }))),
            Auth::Basic {
                username: "ada".to_owned(),
                password: "s3cret".to_owned()
            }
        );
        assert_eq!(
            auth(Some(&serde_json::json!({
                "type": "apikey",
                "apikey": [
                    { "key": "key", "value": "X-Api-Key" },
                    { "key": "value", "value": "42" },
                    { "key": "in", "value": "query" }
                ]
            }))),
            Auth::ApiKey {
                key: "X-Api-Key".to_owned(),
                value: "42".to_owned(),
                location: ApiKeyLocation::Query,
            }
        );
    }

    #[test]
    fn events_join_exec_lines_and_skip_unknown_listens() {
        let scripts = events(Some(&serde_json::json!([
            { "listen": "prerequest", "script": { "exec": ["a();", "b();"] } },
            { "listen": "test", "script": { "exec": ["t();"] } },
            { "listen": "other", "script": { "exec": ["x();"] } }
        ])));
        assert_eq!(scripts.pre_request.as_deref(), Some("a();\nb();"));
        assert_eq!(scripts.test.as_deref(), Some("t();"));
    }

    #[test]
    fn extensions_keep_unknown_keys_and_drop_consumed_ones() {
        let object = serde_json::json!({ "name": "R", "request": {}, "response": [] });
        let kept = extensions(
            object.as_object().expect("object"),
            &["name", "request"],
            &[],
        );
        assert_eq!(
            kept.get("postman").and_then(toml::Value::as_str),
            Some("{\"response\":[]}")
        );
        assert!(
            extensions(
                object.as_object().expect("object"),
                &["name", "request", "response"],
                &[]
            )
            .is_empty()
        );
    }
}
