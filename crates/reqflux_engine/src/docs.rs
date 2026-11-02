//! Local API docs generated from a collection (v1-features S6): `README.md` and a static,
//! self-contained `index.html`, written to a directory the user chooses.
//!
//! Requests are grouped by folder in tree order. Each shows its method and URL with the
//! enabled params, its description, the auth type it uses or inherits (never its fields),
//! its params, headers and body, and its saved examples. Everything is shown as written:
//! `{{variables}}` stay as they are and secret values, which never live in workspace files,
//! are never read. A credential header (`Authorization`, `Proxy-Authorization`, `Cookie`)
//! shows its value only when that value is `{{variables}}`, after an optional scheme such
//! as `Bearer`; a literal value is masked. A secret collection variable shows `(secret)`.

mod html;
mod markdown;

use std::fs;
use std::path::{Path, PathBuf};

use reqflux_core::model::{
    Auth, Body, CollectionNode, Example, ExampleBody, FolderNode, FormPart, Id, Item, KeyValue,
    RawLanguage, RequestNode,
};
use serde_json::Value;

use crate::engine::Engine;
use crate::error::DocsError;
use crate::export::unit::{self, Unit};
use crate::sent::{MASK, is_credential_header};

/// Generated docs, not yet written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDocs {
    /// The collection name.
    pub collection: String,
    /// The files, `README.md` then `index.html`.
    pub files: Vec<DocsFile>,
}

/// One generated file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsFile {
    /// The file name.
    pub name: String,
    /// The file content.
    pub content: String,
}

impl ApiDocs {
    /// Writes the files into `dir`, creating it when missing, and returns their paths. Unless
    /// `force` is set, a file that exists fails the write before anything is written.
    ///
    /// # Errors
    ///
    /// [`DocsError::Exists`] or [`DocsError::Write`].
    pub fn write(&self, dir: &Path, force: bool) -> Result<Vec<PathBuf>, DocsError> {
        let paths: Vec<PathBuf> = self.files.iter().map(|file| dir.join(&file.name)).collect();
        if !force && let Some(path) = paths.iter().find(|path| path.exists()) {
            return Err(DocsError::Exists { path: path.clone() });
        }
        fs::create_dir_all(dir).map_err(|source| DocsError::Write {
            path: dir.to_owned(),
            source,
        })?;
        for (file, path) in self.files.iter().zip(&paths) {
            fs::write(path, &file.content).map_err(|source| DocsError::Write {
                path: path.clone(),
                source,
            })?;
        }
        Ok(paths)
    }
}

impl Engine {
    /// Generates the docs of the collection `selector`, by id or slug.
    ///
    /// # Errors
    ///
    /// [`DocsError::UnknownCollection`] when no collection matches, or a saved example could
    /// not be read.
    pub fn docs(&self, selector: &str) -> Result<ApiDocs, DocsError> {
        let Some(Unit::Collection(node)) = unit::select(self.workspace(), selector) else {
            return Err(DocsError::UnknownCollection(selector.to_owned()));
        };
        let examples = self.examples_of(node)?;
        let blocks = blocks(node, &|id| {
            examples.get(&id).map(Vec::as_slice).unwrap_or_default()
        });
        let title = node.collection.name.clone();
        Ok(ApiDocs {
            files: vec![
                DocsFile {
                    name: "README.md".to_owned(),
                    content: markdown::render(&blocks),
                },
                DocsFile {
                    name: "index.html".to_owned(),
                    content: html::render(&title, &blocks),
                },
            ],
            collection: title,
        })
    }
}

/// One piece of the docs, rendered by both writers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    /// A heading of `level` 1 to 6 with an anchor.
    Heading {
        level: usize,
        anchor: String,
        text: String,
    },
    /// A paragraph of plain text.
    Text(String),
    /// `METHOD url` as code.
    Call(String),
    /// A code block.
    Code(String),
    /// A table with three columns.
    Table {
        title: &'static str,
        head: [&'static str; 3],
        rows: Vec<[String; 3]>,
    },
}

type ExamplesOf<'a> = dyn Fn(Id) -> &'a [Example] + 'a;

fn blocks<'a>(node: &'a CollectionNode, examples: &ExamplesOf<'a>) -> Vec<Block> {
    let collection = &node.collection;
    let mut out = vec![Block::Heading {
        level: 1,
        anchor: format!("c-{}", collection.id),
        text: collection.name.clone(),
    }];
    push_description(&mut out, &collection.extensions);
    if !matches!(collection.auth, Auth::Inherit) {
        out.push(Block::Text(format!("Auth: {}", collection.auth.kind())));
    }
    if !collection.variables.is_empty() {
        out.push(Block::Table {
            title: "Variables",
            head: ["Key", "Value", "Notes"],
            rows: collection
                .variables
                .iter()
                .map(|variable| {
                    let value = if variable.secret {
                        "(secret)".to_owned()
                    } else {
                        variable.value.clone().unwrap_or_default()
                    };
                    [
                        variable.key.clone(),
                        value,
                        notes(variable.description.as_ref(), variable.enabled),
                    ]
                })
                .collect(),
        });
    }
    let mut chain = vec![(collection.name.as_str(), &collection.auth)];
    items(&node.items, 2, &mut chain, examples, &mut out);
    out
}

/// A name and auth of each enclosing container, outermost first.
type Chain<'a> = Vec<(&'a str, &'a Auth)>;

fn items<'a>(
    items: &'a [Item],
    level: usize,
    chain: &mut Chain<'a>,
    examples: &ExamplesOf<'a>,
    out: &mut Vec<Block>,
) {
    for item in items {
        match item {
            Item::Folder(folder) => folder_blocks(folder, level, chain, examples, out),
            Item::Request(request) => request_blocks(request, level, chain, examples, out),
        }
    }
}

fn folder_blocks<'a>(
    node: &'a FolderNode,
    level: usize,
    chain: &mut Chain<'a>,
    examples: &ExamplesOf<'a>,
    out: &mut Vec<Block>,
) {
    out.push(Block::Heading {
        level: level.min(6),
        anchor: format!("f-{}", node.folder.id),
        text: node.folder.name.clone(),
    });
    push_description(out, &node.folder.extensions);
    if !matches!(node.folder.auth, Auth::Inherit) {
        out.push(Block::Text(format!("Auth: {}", node.folder.auth.kind())));
    }
    chain.push((node.folder.name.as_str(), &node.folder.auth));
    items(&node.items, level + 1, chain, examples, out);
    chain.pop();
}

fn request_blocks<'a>(
    node: &'a RequestNode,
    level: usize,
    chain: &Chain<'a>,
    examples: &ExamplesOf<'a>,
    out: &mut Vec<Block>,
) {
    let request = &node.request;
    out.push(Block::Heading {
        level: level.min(6),
        anchor: format!("r-{}", request.id),
        text: format!("{} {}", request.method.as_str(), request.name),
    });
    out.push(Block::Call(format!(
        "{} {}",
        request.method.as_str(),
        url_with_params(&request.url, &request.params)
    )));
    push_description(out, &request.extensions);
    out.push(Block::Text(auth_text(&request.auth, chain)));
    push_rows(out, "Query parameters", &request.params);
    push_rows(out, "Headers", &request.headers);
    push_body(out, &request.body);
    for example in examples(request.id) {
        push_example(out, example, (level + 1).min(6));
    }
}

/// The URL as written with the enabled params appended, as a send would build it.
fn url_with_params(url: &str, params: &[KeyValue]) -> String {
    let query: Vec<String> = params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| format!("{}={}", param.key, param.value))
        .collect();
    if query.is_empty() {
        url.to_owned()
    } else {
        let separator = if url.contains('?') { '&' } else { '?' };
        format!("{url}{separator}{}", query.join("&"))
    }
}

/// `Auth: <type>`, or where an inherited auth comes from; never a field.
fn auth_text(auth: &Auth, chain: &Chain<'_>) -> String {
    if !matches!(auth, Auth::Inherit) {
        return format!("Auth: {}", auth.kind());
    }
    match chain
        .iter()
        .rev()
        .find(|(_, auth)| !matches!(auth, Auth::Inherit))
    {
        Some((name, auth)) => format!("Auth: inherited from {name} ({})", auth.kind()),
        None => "Auth: none".to_owned(),
    }
}

fn push_rows(out: &mut Vec<Block>, title: &'static str, rows: &[KeyValue]) {
    if rows.is_empty() {
        return;
    }
    out.push(Block::Table {
        title,
        head: ["Key", "Value", "Notes"],
        rows: rows
            .iter()
            .map(|row| {
                [
                    row.key.clone(),
                    header_value(&row.key, &row.value),
                    notes(row.description.as_ref(), row.enabled),
                ]
            })
            .collect(),
    });
}

/// A value as written, unless it is a literal credential header value.
fn header_value(key: &str, value: &str) -> String {
    if is_credential_header(key) && !only_variables(value) {
        MASK.to_owned()
    } else {
        value.to_owned()
    }
}

/// Whether `value` is made only of `{{variables}}` and spaces, after an optional scheme word
/// such as `Bearer`.
fn only_variables(value: &str) -> bool {
    let value = value.trim();
    let rest = match value.split_once(' ') {
        Some((scheme, rest)) if !scheme.contains("{{") => rest,
        _ => value,
    };
    let mut rest = rest.trim();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let Some(after) = rest.strip_prefix("{{") else {
            return false;
        };
        let Some(end) = after.find("}}") else {
            return false;
        };
        rest = after.get(end + 2..).unwrap_or_default().trim_start();
    }
    true
}

fn notes(description: Option<&String>, enabled: bool) -> String {
    let mut parts = Vec::new();
    if !enabled {
        parts.push("disabled".to_owned());
    }
    if let Some(description) = description {
        parts.push(description.clone());
    }
    parts.join(", ")
}

fn push_body(out: &mut Vec<Block>, body: &Body) {
    match body {
        Body::None => {}
        Body::Raw { language, text } => {
            out.push(Block::Text(format!(
                "Body: raw ({})",
                language_name(*language)
            )));
            out.push(Block::Code(text.clone()));
        }
        Body::Urlencoded { fields } => {
            out.push(Block::Text("Body: urlencoded".to_owned()));
            push_rows(out, "Fields", fields);
        }
        Body::FormData { parts } => {
            out.push(Block::Text("Body: formdata".to_owned()));
            out.push(Block::Table {
                title: "Parts",
                head: ["Key", "Value", "Notes"],
                rows: parts.iter().map(part_row).collect(),
            });
        }
        Body::Binary { path } => out.push(Block::Text(format!("Body: binary, {path}"))),
        Body::Graphql { query, variables } => {
            out.push(Block::Text("Body: graphql".to_owned()));
            out.push(Block::Code(query.clone()));
            if !variables.is_empty() {
                out.push(Block::Code(variables.clone()));
            }
        }
    }
}

fn part_row(part: &FormPart) -> [String; 3] {
    match part {
        FormPart::Text {
            key,
            value,
            description,
            enabled,
        } => [
            key.clone(),
            value.clone(),
            notes(description.as_ref(), *enabled),
        ],
        FormPart::File {
            key,
            path,
            description,
            enabled,
        } => [
            key.clone(),
            format!("file: {path}"),
            notes(description.as_ref(), *enabled),
        ],
    }
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

/// A saved example: its name and status, the request it was sent as, the response headers
/// and a text body. The file already holds the response masked.
fn push_example(out: &mut Vec<Block>, example: &Example, level: usize) {
    let response = &example.response;
    let status = match &response.reason {
        Some(reason) => format!("{} {reason}", response.status),
        None => response.status.to_string(),
    };
    out.push(Block::Heading {
        level,
        anchor: format!("e-{}", example.id),
        text: format!("Example: {} ({status})", example.name),
    });
    out.push(Block::Call(format!(
        "{} {}",
        example.request.method.as_str(),
        url_with_params(&example.request.url, &example.request.params)
    )));
    push_rows(out, "Response headers", &response.headers);
    if let ExampleBody::Text(text) = &response.body {
        out.push(Block::Code(text.clone()));
    }
}

/// The description the import kept from Postman, a string or `{content}`.
fn push_description(out: &mut Vec<Block>, extensions: &toml::Table) {
    let Some(rest) = extensions
        .get("postman")
        .and_then(toml::Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
    else {
        return;
    };
    let text = match rest.get("description") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(object)) => object
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => return,
    };
    if !text.trim().is_empty() {
        out.push(Block::Text(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_variables_after_an_optional_scheme_show() {
        assert!(only_variables("{{token}}"));
        assert!(only_variables("Bearer {{token}}"));
        assert!(only_variables("Basic {{user}} {{pass}}"));
        assert!(!only_variables("Bearer abc"));
        assert!(!only_variables("Bearer {{token}}x"));
        assert!(!only_variables("{{token"));
        assert!(!only_variables("Bearer"));
        assert!(!only_variables(""));
    }
}
