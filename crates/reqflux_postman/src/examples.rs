//! A request's `response` entries to saved examples (v1-features S4).
//!
//! An entry maps when it is an object with a `code` that is an HTTP status and its request
//! (`originalRequest`, else the item's request) maps; anything else stays in the request's
//! Postman text, so nothing is lost. The values are kept as written: masking credentials
//! belongs to the writer.

use reqflux_core::model::{Example, ExampleBody, ExampleRequest, ExampleResponse, Id};
use serde_json::{Map, Value};

use crate::collection::{auth, body, extensions, key_values, method, url_and_params};

/// The `response` entries of a request item: the examples that map, in order, and the
/// entries that do not, as they were.
pub(crate) fn responses(
    value: Option<&Value>,
    request: &Map<String, Value>,
) -> (Vec<Example>, Vec<Value>) {
    let Some(entries) = value.and_then(Value::as_array) else {
        return (Vec::new(), Vec::new());
    };
    let mut mapped = Vec::new();
    let mut unmapped = Vec::new();
    for entry in entries {
        match example(entry, request) {
            Some(example) => mapped.push(example),
            None => unmapped.push(entry.clone()),
        }
    }
    (mapped, unmapped)
}

fn example(entry: &Value, item_request: &Map<String, Value>) -> Option<Example> {
    let object = entry.as_object()?;
    let status = object
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|code| u16::try_from(code).ok())
        .filter(|code| (100..=999).contains(code))?;
    let request = match object.get("originalRequest") {
        Some(original) => example_request(original.as_object()?)?,
        None => example_request(item_request)?,
    };
    let headers = match object.get("header") {
        None | Some(Value::Null) => Vec::new(),
        Some(rows @ Value::Array(_)) => key_values(Some(rows)),
        Some(_) => return None,
    };
    let body = match object.get("body") {
        None | Some(Value::Null) => ExampleBody::None,
        Some(Value::String(text)) if text.is_empty() => ExampleBody::None,
        Some(Value::String(text)) => ExampleBody::Text(text.clone()),
        Some(_) => return None,
    };
    let reason = match object.get("status") {
        None | Some(Value::Null) => None,
        Some(Value::String(reason)) => Some(reason.clone()),
        Some(_) => return None,
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map_or_else(|| status.to_string(), ToOwned::to_owned);
    Some(Example {
        id: Id::new(),
        name,
        request,
        response: ExampleResponse {
            status,
            reason,
            headers,
            body,
        },
        extensions: extensions(
            object,
            &[
                "name",
                "originalRequest",
                "code",
                "status",
                "header",
                "body",
            ],
            &[],
        ),
    })
}

/// A Postman request object as an example's request, or `None` when it does not map.
fn example_request(request: &Map<String, Value>) -> Option<ExampleRequest> {
    let method = method(request, "").ok()?;
    let (url, params) = url_and_params(request, "").ok()?;
    Some(ExampleRequest {
        method,
        url,
        params,
        headers: key_values(request.get("header")),
        auth: auth(request.get("auth")),
        body: body(request, "").ok()?,
    })
}
