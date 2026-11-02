//! Per-user app state in SQLite (S1, S8): drafts, the send history (history S1-S5,
//! see [`history`](self#history)), current values (variables-editor S4) and the recent
//! workspaces (recent-workspaces S1, see [`AppState::recent_workspaces`]) and small settings
//! (light-theme S4, see [`AppState::setting`]).
//!
//! The state file lives at `$XDG_STATE_HOME/reqflux/state.sqlite3`, or
//! `$HOME/.local/state/reqflux/state.sqlite3` when the variable is unset, and never inside
//! the workspace directory. It is a SQLite database with an explicit, forward-only schema
//! version, so history and other app state can join later without rewriting the drafts
//! table.
//!
//! A [`Draft`] stores the whole entity document as TOML text, keyed by workspace id and
//! entity id, plus the content hash of the file it was made from and an update timestamp.
//! Parsing the payload is the caller's job: what goes in comes out byte-identical.
//!
//! The store is usable from one thread: it opens one connection with `journal_mode=WAL`
//! and `foreign_keys=ON`, and a 250 ms busy timeout so a write held up by another
//! connection's lock fails fast as a [`StateError::Query`] instead of stalling. The file
//! is created owner-only (`0600`) and its directory
//! owner-only too (`0700`, so other users cannot list the draft names) where the platform
//! has file modes.
//!
//! # Schema versions
//!
//! `PRAGMA user_version` holds the schema version and migrations only go forward: version 1
//! is the drafts table; version 2 adds the `history` table and keeps every draft; version 3
//! adds the `current_values` table (see [`AppState::current_values`]) and keeps the drafts
//! and the history; version 4 adds the `recent_workspaces` table and keeps everything
//! before it; version 5 adds the `settings` table and keeps everything before it. A fresh
//! file is created at the latest version, an older one is upgraded in one transaction, and
//! a newer one is refused with [`StateError::UnsupportedVersion`].
//!
//! # History
//!
//! One `history` row per recorded send, keyed by workspace id, newest first by its
//! autoincrement `id`. See [`AppState::record_history`] for the columns.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

mod current;
mod history;
mod recent;
mod settings;

pub use history::{
    HISTORY_MAX_BODY_BYTES, HISTORY_MAX_ENTRIES, HistoryEntry, HistoryLimits, HistoryOutcome,
    HistoryRecord, RecordedResponse,
};
pub use recent::{RECENT_WORKSPACES_MAX, RecentWorkspace};

/// The schema version this build writes. Raising it is a forward-only migration.
const SCHEMA_VERSION: i64 = 5;

/// The drafts table of schema version 1.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS drafts (
    workspace_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    base_hash TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, entity_id)
);
";

/// Why an app state operation failed. Every variant names the state file involved.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The state file could not be opened or configured.
    #[error("could not open the app state at {path}: {source}")]
    Open {
        /// The state file path.
        path: PathBuf,
        /// The underlying SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// The schema migration failed.
    #[error("could not migrate the app state at {path}: {source}")]
    Migrate {
        /// The state file path.
        path: PathBuf,
        /// The underlying SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// The state file was written by a newer ReqFlux; downgrading is not supported.
    #[error(
        "the app state at {path} has schema version {version}, newer than this build's {SCHEMA_VERSION}"
    )]
    UnsupportedVersion {
        /// The state file path.
        path: PathBuf,
        /// The schema version found in the file.
        version: i64,
    },
    /// A stored history entry could not be encoded or decoded.
    #[error("the app state at {path} holds an unreadable history entry: {reason}")]
    Corrupt {
        /// The state file path.
        path: PathBuf,
        /// What was wrong with the entry.
        reason: String,
    },
    /// Reading or writing a draft failed.
    #[error("could not update the app state at {path}: {source}")]
    Query {
        /// The state file path.
        path: PathBuf,
        /// The underlying SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// A path to store is not valid UTF-8; it is refused rather than stored lossily.
    #[error("the path {} is not valid UTF-8 and cannot be stored in the app state at {path}", rejected.display())]
    NonUtf8Path {
        /// The state file path.
        path: PathBuf,
        /// The path that was refused.
        rejected: PathBuf,
    },
    /// The filesystem refused the state file or its directory.
    #[error("could not prepare the app state location {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// Neither `XDG_STATE_HOME` nor `HOME` is set, so there is nowhere to put the state.
    #[error("neither XDG_STATE_HOME nor HOME is set; cannot locate the app state")]
    NoStateDirectory,
}

/// A stored draft of an edited entity (S8).
///
/// The `payload` is the whole entity document as TOML text; the caller owns its shape and
/// parses it. The store's contract is a byte-identical round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// The whole entity document as TOML text.
    pub payload: String,
    /// The content hash of the file the draft was made from, when known.
    pub base_hash: Option<String>,
    /// Milliseconds since the Unix epoch of the last write.
    pub updated_at: i64,
}

impl Draft {
    /// A new draft with the current time as its update timestamp.
    pub fn new(payload: impl Into<String>, base_hash: Option<String>) -> Self {
        Self {
            payload: payload.into(),
            base_hash,
            updated_at: now_millis(),
        }
    }
}

/// The per-user app state: a SQLite database holding the drafts, history, current values,
/// recent workspaces and settings tables.
#[derive(Debug)]
pub struct AppState {
    path: PathBuf,
    conn: Connection,
}

/// The standard per-user state path, from the given variable values. An absolute
/// `XDG_STATE_HOME` wins; otherwise the state lives under `home`. `None` means there is no
/// home to put it in.
pub(crate) fn default_state_path(
    state_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let state_home = state_home.filter(|candidate| candidate.is_absolute());
    let base = match state_home {
        Some(state_home) => state_home.to_path_buf(),
        None => home?.join(".local/state"),
    };
    Some(base.join("reqflux/state.sqlite3"))
}

impl AppState {
    /// Opens (creating if needed) the state file at `path`, creating its directory, and
    /// runs the schema migration. A locked, missing or corrupt file is reported, never
    /// panicked on.
    pub fn open(path: &Path) -> Result<Self, StateError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_state_directory(parent)?;
        }
        let conn = Connection::open(path).map_err(|source| StateError::Open {
            path: path.to_owned(),
            source,
        })?;
        Self::configure(&conn, path)?;
        migrate(&conn, path)?;
        Ok(Self {
            path: path.to_owned(),
            conn,
        })
    }

    /// Opens the existing state file at `path` read-only: reads work, and every write fails
    /// as a [`StateError::Query`]. The file must already be at this build's schema version;
    /// nothing is created or migrated. Front ends use it to check how they report a write
    /// the app state refuses.
    pub fn open_read_only(path: &Path) -> Result<Self, StateError> {
        let open_error = |source| StateError::Open {
            path: path.to_owned(),
            source,
        };
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(open_error)?;
        conn.execute_batch("PRAGMA busy_timeout = 250;")
            .map_err(open_error)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(open_error)?;
        if version != SCHEMA_VERSION {
            return Err(StateError::UnsupportedVersion {
                path: path.to_owned(),
                version,
            });
        }
        Ok(Self {
            path: path.to_owned(),
            conn,
        })
    }

    /// Opens the state file at the standard per-user path:
    /// `$XDG_STATE_HOME/reqflux/state.sqlite3`, or `$HOME/.local/state/reqflux/state.sqlite3`
    /// when the variable is unset.
    pub fn open_default() -> Result<Self, StateError> {
        let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let Some(path) = default_state_path(state_home.as_deref(), home.as_deref()) else {
            return Err(StateError::NoStateDirectory);
        };
        Self::open(&path)
    }

    /// Sets the pragmas every connection needs and the owner-only file mode.
    fn configure(conn: &Connection, path: &Path) -> Result<(), StateError> {
        // The busy timeout bounds how long a write waits for a lock another connection
        // holds (for example an exclusive transaction): without it a draft write can
        // stall for seconds before failing. 250 ms keeps the failure fast and surfaces
        // as the usual `StateError::Query`.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 250;",
        )
        .map_err(|source| StateError::Open {
            path: path.to_owned(),
            source,
        })?;
        set_owner_only(path)
    }

    /// Stores `draft`, keyed by `workspace_id` and `entity_id`, overwriting any previous
    /// draft and stamping the stored record with the current time.
    pub fn put_draft(
        &mut self,
        workspace_id: &str,
        entity_id: &str,
        draft: Draft,
    ) -> Result<(), StateError> {
        self.conn
            .execute(
                "INSERT INTO drafts (workspace_id, entity_id, payload, base_hash, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (workspace_id, entity_id) DO UPDATE SET
                     payload = excluded.payload,
                     base_hash = excluded.base_hash,
                     updated_at = excluded.updated_at",
                rusqlite::params![
                    workspace_id,
                    entity_id,
                    draft.payload,
                    draft.base_hash,
                    now_millis()
                ],
            )
            .map_err(query_error(&self.path))?;
        Ok(())
    }

    /// The draft of `entity_id` in `workspace_id`, if any.
    pub fn draft(&self, workspace_id: &str, entity_id: &str) -> Result<Option<Draft>, StateError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT payload, base_hash, updated_at FROM drafts
                 WHERE workspace_id = ?1 AND entity_id = ?2",
            )
            .map_err(query_error(&self.path))?;
        let mut rows = statement
            .query(rusqlite::params![workspace_id, entity_id])
            .map_err(query_error(&self.path))?;
        match rows.next().map_err(query_error(&self.path))? {
            Some(row) => draft_from_row(row)
                .map(Some)
                .map_err(query_error(&self.path)),
            None => Ok(None),
        }
    }

    /// Deletes the draft of `entity_id` in `workspace_id`, reporting whether there was one.
    pub fn delete_draft(
        &mut self,
        workspace_id: &str,
        entity_id: &str,
    ) -> Result<bool, StateError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM drafts WHERE workspace_id = ?1 AND entity_id = ?2",
                rusqlite::params![workspace_id, entity_id],
            )
            .map_err(query_error(&self.path))?;
        Ok(changed > 0)
    }

    /// Every draft of a workspace, ordered by entity id, for restoring it.
    pub fn drafts(&self, workspace_id: &str) -> Result<Vec<(String, Draft)>, StateError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT entity_id, payload, base_hash, updated_at FROM drafts
                 WHERE workspace_id = ?1 ORDER BY entity_id",
            )
            .map_err(query_error(&self.path))?;
        let rows = statement
            .query_map(rusqlite::params![workspace_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Draft {
                        payload: row.get(1)?,
                        base_hash: row.get(2)?,
                        updated_at: row.get(3)?,
                    },
                ))
            })
            .map_err(query_error(&self.path))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(query_error(&self.path))
    }

    /// Deletes every draft of a workspace, reporting how many there were.
    pub fn delete_workspace_drafts(&mut self, workspace_id: &str) -> Result<usize, StateError> {
        self.conn
            .execute(
                "DELETE FROM drafts WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )
            .map_err(query_error(&self.path))
    }
}

/// Reads a draft from the current row of a `payload, base_hash, updated_at` select.
fn draft_from_row(row: &rusqlite::Row<'_>) -> Result<Draft, rusqlite::Error> {
    Ok(Draft {
        payload: row.get(0)?,
        base_hash: row.get(1)?,
        updated_at: row.get(2)?,
    })
}

/// The error mapper for draft queries.
fn query_error(path: &Path) -> impl Fn(rusqlite::Error) -> StateError + '_ {
    let path = path.to_owned();
    move |source| StateError::Query {
        path: path.clone(),
        source,
    }
}

/// Runs the explicit, forward-only schema migration.
fn migrate(conn: &Connection, path: &Path) -> Result<(), StateError> {
    let migrate_error = |source| StateError::Migrate {
        path: path.to_owned(),
        source,
    };
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(migrate_error)?;
    let steps = match version {
        0 => [
            SCHEMA_V1,
            history::SCHEMA_V2,
            current::SCHEMA_V3,
            recent::SCHEMA_V4,
            settings::SCHEMA_V5,
        ]
        .as_slice(),
        1 => [
            history::SCHEMA_V2,
            current::SCHEMA_V3,
            recent::SCHEMA_V4,
            settings::SCHEMA_V5,
        ]
        .as_slice(),
        2 => [current::SCHEMA_V3, recent::SCHEMA_V4, settings::SCHEMA_V5].as_slice(),
        3 => [recent::SCHEMA_V4, settings::SCHEMA_V5].as_slice(),
        4 => [settings::SCHEMA_V5].as_slice(),
        version if version == SCHEMA_VERSION => return Ok(()),
        version => {
            return Err(StateError::UnsupportedVersion {
                path: path.to_owned(),
                version,
            });
        }
    };
    // Every step and the version bump commit together, so a failed upgrade leaves the
    // file at its old version with its drafts.
    let mut script = String::from("BEGIN IMMEDIATE;");
    for step in steps {
        script.push_str(step);
    }
    script.push_str(&format!("PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"));
    if let Err(source) = conn.execute_batch(&script) {
        if !conn.is_autocommit()
            && let Err(rollback) = conn.execute_batch("ROLLBACK;")
        {
            return Err(migrate_error(rollback));
        }
        return Err(migrate_error(source));
    }
    Ok(())
}

/// Creates the state directory if needed, owner-only where the platform has file modes,
/// so other users cannot list the draft names.
pub(crate) fn create_state_directory(dir: &Path) -> Result<(), StateError> {
    fs::create_dir_all(dir).map_err(|source| StateError::Io {
        path: dir.to_owned(),
        source,
    })?;
    restrict_dir(dir)
}

/// Restricts a directory to its owner, where the platform has file modes.
#[cfg(unix)]
fn restrict_dir(dir: &Path) -> Result<(), StateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(dir)
        .map_err(|source| StateError::Io {
            path: dir.to_owned(),
            source,
        })?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(dir, permissions).map_err(|source| StateError::Io {
        path: dir.to_owned(),
        source,
    })
}

/// Restricts a directory to its owner, where the platform has file modes.
#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) -> Result<(), StateError> {
    Ok(())
}

/// Restricts the state file to its owner, where the platform has file modes.
#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<(), StateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|source| StateError::Io {
            path: path.to_owned(),
            source,
        })?
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|source| StateError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Restricts the state file to its owner, where the platform has file modes.
#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<(), StateError> {
    Ok(())
}

/// The current time in milliseconds since the Unix epoch, or `0` if the clock is before it.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    /// A state file nested two directories deep, to prove `open` creates parents.
    fn state_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("reqflux/state.sqlite3")
    }

    fn draft(payload: &str) -> Draft {
        Draft::new(payload, Some("hash-1".to_owned()))
    }

    #[test]
    fn open_creates_the_directory_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);

        AppState::open(&path).unwrap();

        assert!(path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the state file must be owner-only");
        }
    }

    #[test]
    fn the_state_directory_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);

        AppState::open(&path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let reqflux_dir = path.parent().expect("the state file has a parent");
            let mode = fs::metadata(reqflux_dir).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o700,
                "the state directory must be owner-only"
            );
        }
    }

    #[test]
    fn a_second_open_keeps_the_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);

        {
            let mut store = AppState::open(&path).unwrap();
            store.put_draft("ws", "req", draft("a = 1")).unwrap();
        }
        let store = AppState::open(&path).unwrap();

        let stored = store.draft("ws", "req").unwrap().unwrap();
        assert_eq!(stored.payload, "a = 1");
        assert_eq!(stored.base_hash.as_deref(), Some("hash-1"));
    }

    #[test]
    fn put_get_overwrite_and_delete_one_draft() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();

        assert!(store.draft("ws", "req").unwrap().is_none());

        store.put_draft("ws", "req", draft("a = 1")).unwrap();
        assert_eq!(store.draft("ws", "req").unwrap().unwrap().payload, "a = 1");

        store.put_draft("ws", "req", draft("a = 2")).unwrap();
        assert_eq!(store.draft("ws", "req").unwrap().unwrap().payload, "a = 2");

        assert!(store.delete_draft("ws", "req").unwrap());
        assert!(store.draft("ws", "req").unwrap().is_none());
        assert!(!store.delete_draft("ws", "req").unwrap());
    }

    #[test]
    fn listing_and_deleting_are_per_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();
        store.put_draft("ws-a", "req-1", draft("a = 1")).unwrap();
        store.put_draft("ws-a", "req-2", draft("a = 2")).unwrap();
        store.put_draft("ws-b", "req-1", draft("b = 1")).unwrap();

        let mut listed: Vec<(String, String)> = store
            .drafts("ws-a")
            .unwrap()
            .into_iter()
            .map(|(entity_id, draft)| (entity_id, draft.payload))
            .collect();
        listed.sort();
        assert_eq!(
            listed,
            vec![
                ("req-1".to_owned(), "a = 1".to_owned()),
                ("req-2".to_owned(), "a = 2".to_owned()),
            ]
        );

        store.delete_workspace_drafts("ws-a").unwrap();

        assert!(store.drafts("ws-a").unwrap().is_empty());
        assert!(store.draft("ws-b", "req-1").unwrap().is_some());
    }

    #[test]
    fn the_payload_round_trips_byte_identically() {
        let payload = "\
# A long, multi-line request document with unicode: ünïcødé, 日本語, 😀.
method = \"POST\"
url = \"https://example.com/{{path}}\"

[headers]
[[headers.rows]]
key = \"Content-Type\"
value = \"application/json; charset=utf-8\"
";
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();

        store.put_draft("ws", "req", draft(payload)).unwrap();

        assert_eq!(store.draft("ws", "req").unwrap().unwrap().payload, payload);
    }

    #[test]
    fn updated_at_increases_on_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();

        store.put_draft("ws", "req", draft("a = 1")).unwrap();
        let before = store.draft("ws", "req").unwrap().unwrap().updated_at;

        sleep(Duration::from_millis(10));
        store.put_draft("ws", "req", draft("a = 2")).unwrap();
        let after = store.draft("ws", "req").unwrap().unwrap().updated_at;

        assert!(after > before, "updated_at must increase on overwrite");
    }

    #[test]
    fn a_write_blocked_by_an_exclusive_lock_fails_fast() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);
        let mut store = AppState::open(&path).unwrap();

        // Another connection holds the write lock; the store must give up quickly
        // instead of stalling the caller.
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let started = Instant::now();
        let error = store.put_draft("ws", "req", draft("a = 1")).unwrap_err();
        let elapsed = started.elapsed();

        holder.execute_batch("ROLLBACK;").unwrap();

        assert!(
            matches!(error, StateError::Query { .. }),
            "expected a query error, got: {error}"
        );
        assert!(
            elapsed < Duration::from_millis(1000),
            "the write must fail fast, took {elapsed:?}"
        );
    }

    #[test]
    fn opening_a_file_that_is_not_a_database_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        fs::write(&path, b"this is not a database").unwrap();

        let error = AppState::open(&path).unwrap_err();

        assert!(
            error.to_string().contains(path.to_string_lossy().as_ref()),
            "the error should name the state file: {error}"
        );
    }

    #[test]
    fn a_read_only_state_reads_and_refuses_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);
        AppState::open(&path)
            .unwrap()
            .set_setting("theme", "dark")
            .unwrap();

        let mut store = AppState::open_read_only(&path).unwrap();

        assert_eq!(store.setting("theme").unwrap().as_deref(), Some("dark"));
        let error = store.set_setting("theme", "light").unwrap_err();
        assert!(matches!(error, StateError::Query { .. }), "{error}");
        assert_eq!(
            AppState::open(&path)
                .unwrap()
                .setting("theme")
                .unwrap()
                .as_deref(),
            Some("dark")
        );
    }

    #[test]
    fn a_read_only_open_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);

        assert!(AppState::open_read_only(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn the_default_path_prefers_xdg_state_home() {
        let path = default_state_path(Some(Path::new("/tmp/state")), Some(Path::new("/home/u")));
        assert_eq!(
            path,
            Some(PathBuf::from("/tmp/state/reqflux/state.sqlite3"))
        );
    }

    #[test]
    fn the_default_path_falls_back_to_home() {
        let path = default_state_path(None, Some(Path::new("/home/u")));
        assert_eq!(
            path,
            Some(PathBuf::from("/home/u/.local/state/reqflux/state.sqlite3"))
        );
        assert_eq!(default_state_path(None, None), None);
    }
}
