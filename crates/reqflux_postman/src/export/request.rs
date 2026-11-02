//! A request, its body and its saved examples as Postman JSON (v1-features S6).

use reqflux_core::model::{
    Body, Example, ExampleBody, ExampleRequest, FormPart, KeyValue, Method, RawLanguage, Request,
    Settings,
};
use serde_json::{Map, Value, json};

use super::auth;

/// A request document: `method`, `header`, `url`, then `body` and `auth` when set.
pub(super) fn request(request: &Request) -> Map<String, Value> {
    document(
        &request.method,
        &request.url,
        &request.params,
        &request.headers,
        &request.body,
        auth::export(&request.auth),
    )
}

fn document(
    method: &Method,
    url: &str,
    params: &[KeyValue],
    headers: &[KeyValue],
    body: &Body,
    auth: Option<Value>,
) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert(
        "method".to_owned(),
        Value::String(method.as_str().to_owned()),
    );
    out.insert("header".to_owned(), rows(headers));
    out.insert("url".to_owned(), url_value(url, params));
    if let Some(body) = body_value(body) {
        out.insert("body".to_owned(), body);
    }
    if let Some(auth) = auth {
        out.insert("auth".to_owned(), auth);
    }
    out
}

/// A Postman URL: `raw` with the enabled params as its query string, and every param in
/// `query`, a disabled one with `disabled = true`.
fn url_value(url: &str, params: &[KeyValue]) -> Value {
    let query: Vec<String> = params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| {
            if param.value.is_empty() {
                param.key.clone()
            } else {
                format!("{}={}", param.key, param.value)
            }
        })
        .collect();
    let mut raw = url.to_owned();
    if !query.is_empty() {
        raw.push('?');
        raw.push_str(&query.join("&"));
    }
    let mut out = Map::new();
    out.insert("raw".to_owned(), Value::String(raw));
    if !params.is_empty() {
        out.insert("query".to_owned(), rows(params));
    }
    Value::Object(out)
}

/// Rows of `{key, value, description?, disabled?}`.
pub(crate) fn rows(rows: &[KeyValue]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                let mut out = Map::new();
                out.insert("key".to_owned(), Value::String(row.key.clone()));
                out.insert("value".to_owned(), Value::String(row.value.clone()));
                described(&mut out, row.description.as_ref(), row.enabled);
                Value::Object(out)
            })
            .collect(),
    )
}

fn described(out: &mut Map<String, Value>, description: Option<&String>, enabled: bool) {
    if let Some(description) = description {
        out.insert("description".to_owned(), Value::String(description.clone()));
    }
    if !enabled {
        out.insert("disabled".to_owned(), Value::Bool(true));
    }
}

/// The Postman `body`, or `None` for no body.
fn body_value(body: &Body) -> Option<Value> {
    let value = match body {
        Body::None => return None,
        Body::Raw { language, text } => json!({
            "mode": "raw",
            "raw": text,
            "options": { "raw": { "language": language_name(*language) } },
        }),
        Body::Urlencoded { fields } => {
            let mut fields = rows(fields);
            if let Value::Array(rows) = &mut fields {
                for row in rows {
                    if let Value::Object(row) = row {
                        row.insert("type".to_owned(), Value::String("text".to_owned()));
                    }
                }
            }
            json!({ "mode": "urlencoded", "urlencoded": fields })
        }
        Body::FormData { parts } => json!({
            "mode": "formdata",
            "formdata": parts.iter().map(form_part).collect::<Vec<_>>(),
        }),
        Body::Binary { path } => json!({ "mode": "file", "file": { "src": path } }),
        Body::Graphql { query, variables } => json!({
            "mode": "graphql",
            "graphql": { "query": query, "variables": variables },
        }),
    };
    Some(value)
}

fn language_name(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Json => "json",
        RawLanguage::Xml => "xml",
        RawLanguage::Text => "text",
        RawLanguage::Html => "html",
        RawLanguage::Javascript => "javascript",
    }
}

fn form_part(part: &FormPart) -> Value {
    let mut out = Map::new();
    match part {
        FormPart::Text {
            key,
            value,
            description,
            enabled,
        } => {
            out.insert("key".to_owned(), Value::String(key.clone()));
            out.insert("value".to_owned(), Value::String(value.clone()));
            out.insert("type".to_owned(), Value::String("text".to_owned()));
            described(&mut out, description.as_ref(), *enabled);
        }
        FormPart::File {
            key,
            path,
            description,
            enabled,
        } => {
            out.insert("key".to_owned(), Value::String(key.clone()));
            out.insert("src".to_owned(), Value::String(path.clone()));
            out.insert("type".to_owned(), Value::String("file".to_owned()));
            described(&mut out, description.as_ref(), *enabled);
        }
    }
    Value::Object(out)
}

/// A saved example as a `response[]` entry: its request as `originalRequest`, the status as
/// `code` and `status`, the headers and a text body. A body kept in a file is left out:
/// Postman holds response bodies as text.
pub(super) fn example(example: &Example) -> Value {
    let mut out = Map::new();
    out.insert("name".to_owned(), Value::String(example.name.clone()));
    out.insert(
        "originalRequest".to_owned(),
        Value::Object(example_request(&example.request)),
    );
    let response = &example.response;
    if let Some(reason) = &response.reason {
        out.insert("status".to_owned(), Value::String(reason.clone()));
    }
    out.insert("code".to_owned(), Value::from(response.status));
    out.insert("header".to_owned(), rows(&response.headers));
    if let ExampleBody::Text(text) = &response.body {
        out.insert("body".to_owned(), Value::String(text.clone()));
    }
    super::extend_missing(&mut out, super::postman_rest(&example.extensions));
    Value::Object(out)
}

fn example_request(request: &ExampleRequest) -> Map<String, Value> {
    document(
        &request.method,
        &request.url,
        &request.params,
        &request.headers,
        &request.body,
        auth::export(&request.auth),
    )
}

/// Writes the settings Postman has a place for into the item's `protocolProfileBehavior`:
/// `followRedirects` and `strictSSL`. Postman has no per-request timeout.
pub(super) fn settings(settings: &Settings, item: &mut Map<String, Value>) {
    let pairs = [
        ("followRedirects", settings.follow_redirects),
        ("strictSSL", settings.verify_tls),
    ];
    if pairs.iter().all(|(_, value)| value.is_none()) {
        return;
    }
    let behavior = item
        .entry("protocolProfileBehavior")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(behavior) = behavior {
        for (key, value) in pairs {
            if let Some(value) = value {
                behavior.insert(key.to_owned(), Value::Bool(value));
            }
        }
    }
}
