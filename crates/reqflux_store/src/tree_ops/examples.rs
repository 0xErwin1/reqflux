//! Saved examples of a request (v1-features S4): one `<example-slug>.example.toml` per
//! example in the request's `<request-slug>.examples` directory, plus a
//! `<example-slug>.body` file for a body that is not text.
//!
//! The load skips the examples directory, so these functions read it on demand. A request
//! rename, duplicate, move or delete already carries the directory with the request.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use reqflux_core::model::{Example, ExampleBody, Id};

use super::locate::{Entity, find};
use super::naming::{SlugFor, named};
use crate::error::StoreError;
use crate::hash::ContentHash;
use crate::layout::{
    EXAMPLE_BODY_SUFFIX, EXAMPLE_SUFFIX, examples_dir_in, validate_request_slug, validate_slug,
};
use crate::load::Loaded;
use crate::save::{check, io_error, save_document, write_atomic};

/// A saved example as read from disk.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredExample {
    /// The file slug: the example is `<slug>.example.toml`.
    pub slug: String,
    /// The example document.
    pub example: Example,
    /// The response body bytes: the text, or the content of the body file.
    pub body: Vec<u8>,
    /// The hash of the example file as read.
    pub hash: ContentHash,
}

/// The saved examples of the request `request`, ordered by name, then slug. A request
/// without an examples directory has none.
///
/// # Errors
///
/// [`StoreError::NotFound`] or [`StoreError::NotARequest`] for `request`, an unreadable or
/// unparsable example file, or a body file that is missing or not a plain file name.
pub fn examples(
    root: &Path,
    loaded: &Loaded,
    request: Id,
) -> Result<Vec<StoredExample>, StoreError> {
    let dir = examples_dir(root, loaded, request)?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io_error(&dir, source)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| io_error(&dir, source))?;
        let name = entry.file_name();
        let Some(slug) = name
            .to_str()
            .and_then(|name| name.strip_suffix(EXAMPLE_SUFFIX))
            .filter(|slug| validate_slug(slug).is_ok())
        else {
            continue;
        };
        found.push(read_example(&dir, slug)?);
    }
    found.sort_by(|a, b| {
        a.example
            .name
            .cmp(&b.example.name)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    Ok(found)
}

/// Saves `example` as a new example of `request` and returns its slug, taken from its name
/// (`OK`, then `OK 2`, ... when taken). With `body_file`, the body is written to
/// `<slug>.body` and the example's body points at it; otherwise the example's own body is
/// kept as given.
///
/// # Errors
///
/// As [`examples`], plus [`StoreError::EmptyName`] or a write failure.
pub fn create_example(
    root: &Path,
    loaded: &Loaded,
    request: Id,
    mut example: Example,
    body_file: Option<&[u8]>,
) -> Result<String, StoreError> {
    let dir = examples_dir(root, loaded, request)?;
    let (name, slug) = free_name(&dir, &example.name, None)?;
    example.name = name;
    if let Some(bytes) = body_file {
        let file = body_file_name(&slug);
        check(&dir.join(&file), None)?;
        write_atomic(&dir.join(&file), bytes)?;
        example.response.body = ExampleBody::File(file);
    }
    save_document(&example_file(&dir, &slug), &example, None)?;
    Ok(slug)
}

/// Renames the example `example` of `request` to `name`, moving its file (and body file) to
/// the new slug, and returns the slug. The example file must not have changed since it was
/// read here.
///
/// # Errors
///
/// As [`create_example`], plus [`StoreError::UnknownExample`].
pub fn rename_example(
    root: &Path,
    loaded: &Loaded,
    request: Id,
    example: Id,
    name: &str,
) -> Result<String, StoreError> {
    let dir = examples_dir(root, loaded, request)?;
    let stored = find_example(root, loaded, request, example)?;
    let (name, slug) = free_name(&dir, name, Some(&stored.slug))?;
    let mut document = stored.example;
    document.name = name;
    let old_file = example_file(&dir, &stored.slug);
    if slug == stored.slug {
        save_document(&old_file, &document, Some(stored.hash))?;
        return Ok(slug);
    }
    check(&old_file, Some(stored.hash))?;
    if let ExampleBody::File(file) = &document.response.body {
        let renamed = body_file_name(&slug);
        check(&dir.join(&renamed), None)?;
        fs::rename(dir.join(file), dir.join(&renamed))
            .map_err(|source| io_error(&dir.join(file), source))?;
        document.response.body = ExampleBody::File(renamed);
    }
    save_document(&example_file(&dir, &slug), &document, None)?;
    fs::remove_file(&old_file).map_err(|source| io_error(&old_file, source))?;
    Ok(slug)
}

/// Deletes the example `example` of `request` with its body file, and the examples
/// directory once it is empty. The example file must not have changed since it was read
/// here.
///
/// # Errors
///
/// As [`rename_example`].
pub fn delete_example(
    root: &Path,
    loaded: &Loaded,
    request: Id,
    example: Id,
) -> Result<(), StoreError> {
    let dir = examples_dir(root, loaded, request)?;
    let stored = find_example(root, loaded, request, example)?;
    let file = example_file(&dir, &stored.slug);
    check(&file, Some(stored.hash))?;
    fs::remove_file(&file).map_err(|source| io_error(&file, source))?;
    if let ExampleBody::File(body) = &stored.example.response.body {
        let path = dir.join(body);
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Err(io_error(&path, error));
            }
            _ => {}
        }
    }
    let empty = fs::read_dir(&dir)
        .map_err(|source| io_error(&dir, source))?
        .next()
        .is_none();
    if empty {
        fs::remove_dir(&dir).map_err(|source| io_error(&dir, source))?;
    }
    Ok(())
}

/// The examples directory of the request `request`.
fn examples_dir(root: &Path, loaded: &Loaded, request: Id) -> Result<PathBuf, StoreError> {
    match find(&loaded.workspace, request)? {
        Entity::Request { parent, node } => {
            parent.validate()?;
            validate_request_slug(&node.slug)?;
            Ok(examples_dir_in(&parent.dir(root), &node.slug))
        }
        Entity::Collection(_) | Entity::Folder { .. } => {
            Err(StoreError::NotARequest { id: request })
        }
    }
}

fn find_example(
    root: &Path,
    loaded: &Loaded,
    request: Id,
    example: Id,
) -> Result<StoredExample, StoreError> {
    examples(root, loaded, request)?
        .into_iter()
        .find(|stored| stored.example.id == example)
        .ok_or(StoreError::UnknownExample { id: example })
}

fn read_example(dir: &Path, slug: &str) -> Result<StoredExample, StoreError> {
    let path = example_file(dir, slug);
    let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
    let text = String::from_utf8_lossy(&bytes);
    let example: Example = toml::from_str(&text).map_err(|error| StoreError::Parse {
        path: path.clone(),
        location: crate::error::toml_error_location(&text, &error),
    })?;
    let body = match &example.response.body {
        ExampleBody::None => Vec::new(),
        ExampleBody::Text(text) => text.clone().into_bytes(),
        ExampleBody::File(file) => {
            validate_slug(file)?;
            let body = dir.join(file);
            fs::read(&body).map_err(|source| io_error(&body, source))?
        }
    };
    Ok(StoredExample {
        slug: slug.to_owned(),
        example,
        body,
        hash: ContentHash::of(&bytes),
    })
}

/// `name`, then `name 2`, `name 3`, ...: the first whose slug is free in `dir`, with that
/// slug. `own` is the slug of the example being renamed, which its own name may keep.
fn free_name(dir: &Path, name: &str, own: Option<&str>) -> Result<(String, String), StoreError> {
    let (base, _) = named(name, SlugFor::Container)?;
    let mut n: u64 = 1;
    loop {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{base} {n}")
        };
        let (candidate, slug) = named(&candidate, SlugFor::Container)?;
        if own == Some(slug.as_str()) || !taken(dir, &slug)? {
            return Ok((candidate, slug));
        }
        n += 1;
    }
}

fn taken(dir: &Path, slug: &str) -> Result<bool, StoreError> {
    for path in [example_file(dir, slug), dir.join(body_file_name(slug))] {
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error(&path, source)),
        }
    }
    Ok(false)
}

fn example_file(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{slug}{EXAMPLE_SUFFIX}"))
}

fn body_file_name(slug: &str) -> String {
    format!("{slug}{EXAMPLE_BODY_SUFFIX}")
}
