//! Store errors and load warnings.

use std::fmt;
use std::io;
use std::path::PathBuf;

use reqflux_core::model::Id;

use crate::FORMAT;

/// Why a load or save failed. Every variant that concerns a file names it.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Reading, writing or listing a path failed.
    #[error("{}: {source}", path.display())]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A TOML file does not parse as its entity. The message never quotes the file: the
    /// parser's own text shows the offending line, which can hold a literal secret.
    #[error("{}: {location}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// Where and why, from [`toml_error_location`].
        location: String,
    },
    /// An entity does not serialize to TOML.
    #[error("{}: {source}", path.display())]
    Serialize {
        /// The file being written.
        path: PathBuf,
        /// The serializer error.
        source: toml::ser::Error,
    },
    /// `reqflux.toml` declares a format newer than [`FORMAT`] (S6).
    #[error(
        "workspace format {found} is newer than this version of ReqFlux supports ({FORMAT}); update ReqFlux to open it"
    )]
    NewerFormat {
        /// The declared format.
        found: i64,
    },
    /// `reqflux.toml` has no format or one older than [`FORMAT`] (S6).
    #[error("{}: missing or unsupported workspace format", path.display())]
    UnsupportedFormat {
        /// The manifest path.
        path: PathBuf,
    },
    /// The file changed since the caller read it, or exists when creating (S7).
    #[error("{} changed on disk since it was loaded", path.display())]
    Conflict {
        /// The file that was left untouched.
        path: PathBuf,
    },
    /// A slug is not a single, visible path component.
    #[error("invalid slug `{slug}`")]
    InvalidSlug {
        /// The rejected slug.
        slug: String,
    },
    /// A request slug would share its sidecar scripts with the collection or folder that
    /// contains it (`collection.pre.js`, `folder.pre.js`).
    #[error("request slug `{slug}` is reserved")]
    ReservedSlug {
        /// The rejected slug.
        slug: String,
    },
    /// A request and a folder in the same parent would share a slug. `Collection::order`
    /// holds slugs only, so it could not name the two apart (S4).
    #[error(
        "{path}: a {held} already uses the slug `{slug}` in this parent; a request and a folder cannot share a slug"
    )]
    SlugCollision {
        /// The existing request file or folder directory that holds the slug.
        path: PathBuf,
        /// The slug both entities would share.
        slug: String,
        /// What already holds it: `"request"` or `"folder"`.
        held: &'static str,
    },
    /// A tree operation would give an entity a slug a sibling already holds: a request,
    /// folder, collection or a request's sidecar.
    #[error("{}: the slug `{slug}` is already taken in this parent", path.display())]
    SlugTaken {
        /// The existing file or directory that holds the slug.
        path: PathBuf,
        /// The slug that is taken.
        slug: String,
    },
    /// A name is empty or only whitespace.
    #[error("a name cannot be empty")]
    EmptyName,
    /// A folder would move into itself or one of the folders below it.
    #[error("a folder cannot move into itself or one of its descendants")]
    MoveIntoDescendant,
    /// No collection, folder or request of the loaded workspace has the id.
    #[error("no collection, folder or request has the id {id}")]
    NotFound {
        /// The id looked up.
        id: Id,
    },
    /// The id names a request where a collection or folder is needed.
    #[error("{id} is not a collection or folder")]
    NotAContainer {
        /// The id given as parent.
        id: Id,
    },
    /// No environment of the loaded workspace has the id.
    #[error("no environment has the id {id}")]
    UnknownEnvironment {
        /// The id looked up.
        id: Id,
    },
    /// A variable key is empty or only whitespace.
    #[error("a variable key cannot be empty")]
    EmptyVariableKey,
    /// Another variable of the same scope already has the key.
    #[error("the variable `{key}` already exists in this scope")]
    DuplicateVariableKey {
        /// The key that is taken.
        key: String,
    },
    /// The scope has no variable at the index.
    #[error("no variable at index {index} in this scope")]
    NoVariable {
        /// The index given.
        index: usize,
    },
    /// The operation applies to folders and requests only.
    #[error("a collection cannot be {operation}")]
    CollectionOperation {
        /// The refused operation, in past participle: `"moved"`.
        operation: &'static str,
    },
}

/// `error`, found parsing `text`, as `line <l>, column <c>: <message>`, without the source
/// line the parser's `Display` quotes.
#[must_use]
pub fn toml_error_location(text: &str, error: &toml::de::Error) -> String {
    let message = without_backticked_values(&without_quoted_values(error.message()));
    let Some(before) = error.span().and_then(|span| text.get(..span.start)) else {
        return message;
    };
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    format!("line {line}, column {column}: {message}")
}

/// `message` with every backtick-quoted run before `expected` replaced by `` `…` ``: serde
/// quotes the offending value that way (`unknown variant`, `unknown field`, `invalid type:
/// integer`, `floating point`), and it can be a secret. What follows `expected` is this
/// build's own names and types and stays.
fn without_backticked_values(message: &str) -> String {
    let (head, tail) = message
        .find("expected")
        .map_or((message, ""), |at| message.split_at(at));
    let mut out = String::with_capacity(message.len());
    let mut quoted = false;
    for character in head.chars() {
        if character == '`' {
            if quoted {
                out.push_str("…`");
            } else {
                out.push('`');
            }
            quoted = !quoted;
        } else if !quoted {
            out.push(character);
        }
    }
    if quoted {
        out.push_str("…`");
    }
    out.push_str(tail);
    out
}

/// `message` with every double-quoted run replaced by `"…"`: serde quotes the offending value
/// (`invalid type: string "…", expected u64`), and that value can be a secret.
fn without_quoted_values(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut quoted = false;
    for character in message.chars() {
        if character == '"' {
            if quoted {
                out.push_str("…\"");
            }
            quoted = !quoted;
            if quoted {
                out.push('"');
            }
        } else if !quoted {
            out.push(character);
        }
    }
    if quoted {
        out.push('…');
    }
    out
}

/// A non-fatal problem found while loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The file or directory concerned.
    pub path: PathBuf,
    /// What happened.
    pub kind: WarningKind,
}

/// The kinds of load warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    /// The entity id was already taken by an earlier file in load order; a fresh id was
    /// assigned in memory (S8).
    DuplicateId {
        /// The id in the file.
        id: Id,
        /// The earlier file that keeps it.
        other: PathBuf,
    },
    /// A secret variable carried a value, which was dropped (S10).
    SecretValue {
        /// The variable key.
        key: String,
    },
    /// A directory lacks its entity file (`folder.toml` or `collection.toml`) and was skipped.
    MissingEntityFile {
        /// The expected file name.
        file: &'static str,
    },
    /// A request file uses a reserved slug; it was skipped so its sidecar scripts are not
    /// confused with those of its collection or folder.
    ReservedRequestSlug {
        /// The reserved slug.
        slug: String,
    },
    /// A directory is a symlink; it was not followed so a hand-edited workspace cannot
    /// make loading read files outside the root.
    SymlinkedDirectory,
    /// A request file and a folder directory share a slug in the same parent: a workspace
    /// written before saving refused the pair. Both entities still load, but
    /// `Collection::order` holds slugs only, so an entry with the slug matches both and
    /// the two cannot be ordered separately.
    SharedSlug {
        /// The slug both entities share.
        slug: String,
    },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.kind {
            WarningKind::DuplicateId { id, other } => write!(
                f,
                "{path}: id {id} is already used by {}; assigned a new id",
                other.display()
            ),
            WarningKind::SecretValue { key } => write!(
                f,
                "{path}: secret variable {key} has a value in the workspace file; it was ignored"
            ),
            WarningKind::MissingEntityFile { file } => {
                write!(f, "{path}: directory has no {file}; it was ignored")
            }
            WarningKind::ReservedRequestSlug { slug } => write!(
                f,
                "{path}: request slug `{slug}` is reserved; rename the file to load it"
            ),
            WarningKind::SymlinkedDirectory => {
                write!(f, "{path}: directory is a symlink; it was ignored")
            }
            WarningKind::SharedSlug { slug } => write!(
                f,
                "{path}: a request file and a folder directory share the slug `{slug}`; both are loaded, but an `order` entry with this slug matches both, so they cannot be ordered separately"
            ),
        }
    }
}
