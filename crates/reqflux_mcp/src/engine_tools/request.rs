//! `request_get` and `request_update` (S3), with the window's auth masking (S7): a basic
//! password, a bearer token, an API key value, every field of another auth type and the
//! value of an `Authorization`, `Proxy-Authorization` or `Cookie` header are masked on the
//! way out, and a field that still holds the mask on the way in keeps the stored value.
//!
//! `request_get` returns the `base_hash` of the file it read and `request_update` saves
//! hash-checked against it, so an edit made after the agent's read is a conflict whatever
//! other tools reloaded in between.

use reqflux_core::model::{Auth, Id, Item, KeyValue, Request, RequestNode, Workspace};
use reqflux_engine::{ContentHash, MASK, is_credential_header, toml_error_location};
use serde_json::json;

use super::policy::Scope;
use super::{EngineState, ItemArgs, UpdateArgs, encode, id_result, parse_id};
use crate::tools::ToolError;

/// A request found by id, with the collection and folders that hold it.
pub(super) struct Found<'a> {
    /// The ids and slugs of the collection and the enclosing folders, outermost first.
    pub(super) parents: Vec<(Id, &'a str)>,
    /// The request.
    pub(super) request: &'a RequestNode,
}

/// The request `id` in `workspace`, if any.
pub(super) fn find_request(workspace: &Workspace, id: Id) -> Option<Found<'_>> {
    workspace.collections.iter().find_map(|collection| {
        let mut parents = vec![(collection.collection.id, collection.slug.as_str())];
        find_in(&collection.items, id, &mut parents).map(|request| Found { parents, request })
    })
}

fn find_in<'a>(
    items: &'a [Item],
    id: Id,
    parents: &mut Vec<(Id, &'a str)>,
) -> Option<&'a RequestNode> {
    for item in items {
        match item {
            Item::Request(request) if request.request.id == id => return Some(request),
            Item::Request(_) => {}
            Item::Folder(folder) => {
                parents.push((folder.folder.id, folder.slug.as_str()));
                if let Some(request) = find_in(&folder.items, id, parents) {
                    return Some(request);
                }
                parents.pop();
            }
        }
    }
    None
}

fn unknown(id: Id) -> ToolError {
    ToolError::Engine(format!("no request has the id {id}"))
}

/// `request_get`: the request document as TOML with its auth secrets and credential header
/// values masked, plus its parent's id and slug path and the `base_hash` `request_update`
/// takes (S3, S7).
///
/// # Errors
///
/// A bad or unknown id, no open workspace, or the document does not encode.
pub fn request_get(state: &mut EngineState, args: ItemArgs) -> Result<String, ToolError> {
    state.authorize("request_get", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    let found = find_request(engine.workspace(), id).ok_or_else(|| unknown(id))?;
    let mut request = found.request.request.clone();
    request.auth = masked(&request.auth);
    request.headers = masked_headers(&request.headers);
    let document = toml::to_string(&request)
        .map_err(|error| ToolError::Engine(format!("could not encode the request: {error}")))?;
    let parent_path: Vec<&str> = found.parents.iter().map(|(_, slug)| *slug).collect();
    let parent_id = found.parents.last().map(|(id, _)| id.to_string());
    let base_hash = engine
        .request_base_hash(&id.to_string())
        .map_err(|error| ToolError::Engine(error.to_string()))?
        .map(|hash| hash.to_string());
    encode(&json!({
        "id": id.to_string(),
        "name": request.name,
        "parent_id": parent_id,
        "parent_path": parent_path.join("/"),
        "document": document,
        "base_hash": base_hash,
    }))
}

/// `request_update`: replaces the request `id` with `document`, hash-checked against
/// `base_hash`, the hash of the file `request_get` read, so an edit made since that read is
/// a conflict and nothing is written (S3). A masked auth secret or credential header keeps
/// the stored value (S7).
///
/// # Errors
///
/// A bad or unknown id, a bad `base_hash`, a document that does not parse or carries
/// another id, a mask with no stored value to keep, or a conflict.
pub fn request_update(state: &mut EngineState, args: UpdateArgs) -> Result<String, ToolError> {
    state.authorize("request_update", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let base: ContentHash = args.base_hash.parse().map_err(|error| {
        ToolError::Invalid(format!(
            "invalid base_hash: {error}; pass the one request_get returned"
        ))
    })?;
    let engine = state.engine()?;
    let mut request: Request = toml::from_str(&args.document).map_err(|error| {
        ToolError::Invalid(format!(
            "the document is not a request: {}",
            toml_error_location(&args.document, &error)
        ))
    })?;
    if request.id != id {
        return Err(ToolError::Invalid(format!(
            "the document's id {} is not {id}; a request keeps its id",
            request.id
        )));
    }
    // The stored values a mask restores come from the workspace as last loaded. When the
    // file changed since the agent's read, the save below is a conflict and nothing restored
    // is written.
    let stored = find_request(engine.workspace(), id).ok_or_else(|| unknown(id))?;
    request.auth = restored(request.auth, &stored.request.request.auth)?;
    request.headers = restored_headers(request.headers, &stored.request.request.headers)?;

    engine
        .save_request(&id.to_string(), &request, Some(base))
        .map_err(|error| ToolError::Engine(error.to_string()))?;
    engine
        .reload()
        .map_err(|error| ToolError::Engine(format!("the workspace does not load: {error}")))?;
    id_result(id)
}

/// `headers` with the value of each credential header masked, unless it is empty.
pub(super) fn masked_headers(headers: &[KeyValue]) -> Vec<KeyValue> {
    headers
        .iter()
        .map(|header| {
            let mut header = header.clone();
            if is_credential_header(&header.key) {
                header.value = mask(&header.value);
            }
            header
        })
        .collect()
}

/// `incoming` with each credential header that holds the mask set back to the stored value
/// of the header with the same name and the same position among headers of that name.
fn restored_headers(
    incoming: Vec<KeyValue>,
    stored: &[KeyValue],
) -> Result<Vec<KeyValue>, ToolError> {
    let mut seen: Vec<String> = Vec::new();
    incoming
        .into_iter()
        .map(|mut header| {
            let name = header.key.to_ascii_lowercase();
            let position = seen.iter().filter(|known| **known == name).count();
            seen.push(name);
            if header.value != MASK || !is_credential_header(&header.key) {
                return Ok(header);
            }
            let original = stored
                .iter()
                .filter(|candidate| candidate.key.eq_ignore_ascii_case(&header.key))
                .nth(position)
                .ok_or_else(|| {
                    ToolError::Invalid(format!(
                        "the header {} holds the mask but the stored request has no such \
                         header to keep",
                        header.key
                    ))
                })?;
            header.value.clone_from(&original.value);
            Ok(header)
        })
        .collect()
}

/// `value` masked unless it is empty, so an unset secret stays visibly unset.
fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        MASK.to_owned()
    }
}

/// `auth` with its secret fields masked, as the window shows them.
pub(super) fn masked(auth: &Auth) -> Auth {
    match auth {
        Auth::Inherit | Auth::None => auth.clone(),
        Auth::Basic { username, password } => Auth::Basic {
            username: username.clone(),
            password: mask(password),
        },
        Auth::Bearer { token } => Auth::Bearer { token: mask(token) },
        Auth::ApiKey {
            key,
            value,
            location,
        } => Auth::ApiKey {
            key: key.clone(),
            value: mask(value),
            location: *location,
        },
        // The engine does not know which fields of other types hold credentials, so every
        // value is masked, as in the window.
        Auth::Other { kind, fields } => Auth::Other {
            kind: kind.clone(),
            fields: fields
                .keys()
                .map(|key| (key.clone(), toml::Value::String(MASK.to_owned())))
                .collect(),
        },
    }
}

/// `incoming` with every field that still holds the mask set back to the stored value.
fn restored(incoming: Auth, stored: &Auth) -> Result<Auth, ToolError> {
    let keep = |value: String, field: &str, stored: Option<&str>| {
        if value != MASK {
            return Ok(value);
        }
        stored.map(str::to_owned).ok_or_else(|| {
            ToolError::Invalid(format!(
                "the auth {field} holds the mask but the stored request has no {field} to keep"
            ))
        })
    };
    Ok(match incoming {
        Auth::Basic { username, password } => {
            let stored = match stored {
                Auth::Basic { password, .. } => Some(password.as_str()),
                _ => None,
            };
            Auth::Basic {
                username,
                password: keep(password, "password", stored)?,
            }
        }
        Auth::Bearer { token } => {
            let stored = match stored {
                Auth::Bearer { token } => Some(token.as_str()),
                _ => None,
            };
            Auth::Bearer {
                token: keep(token, "token", stored)?,
            }
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let stored = match stored {
                Auth::ApiKey { value, .. } => Some(value.as_str()),
                _ => None,
            };
            Auth::ApiKey {
                key,
                value: keep(value, "value", stored)?,
                location,
            }
        }
        Auth::Other { kind, fields } => {
            let stored_fields = match stored {
                Auth::Other {
                    kind: stored_kind,
                    fields,
                } if *stored_kind == kind => Some(fields),
                _ => None,
            };
            let mut restored = toml::Table::new();
            for (key, value) in fields {
                let value = match value {
                    toml::Value::String(text) if text == MASK => stored_fields
                        .and_then(|fields| fields.get(&key))
                        .cloned()
                        .ok_or_else(|| {
                            ToolError::Invalid(format!(
                                "the auth field {key} holds the mask but the stored request \
                                 has no {key} to keep"
                            ))
                        })?,
                    other => other,
                };
                restored.insert(key, value);
            }
            Auth::Other {
                kind,
                fields: restored,
            }
        }
        other @ (Auth::Inherit | Auth::None) => other,
    })
}
