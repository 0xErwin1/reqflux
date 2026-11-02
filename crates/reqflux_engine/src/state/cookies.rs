//! The `cookies` table of schema v7 (v1-features S5): the cookie jar of every workspace, one
//! row per cookie keyed by workspace id, domain, path and name. The payload is the engine's
//! encoding of the whole cookie; the state only stores it.

use super::{AppState, StateError, query_error};

/// The table schema version 7 adds.
pub(super) const SCHEMA_V7: &str = "
CREATE TABLE IF NOT EXISTS cookies (
    workspace_id TEXT NOT NULL,
    domain TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (workspace_id, domain, path, name)
);
";

/// One stored cookie.
#[derive(Clone, PartialEq, Eq)]
pub struct CookieRow {
    /// The domain the cookie is stored under: the host for a host-only cookie, else its
    /// `Domain` attribute.
    pub domain: String,
    /// The cookie path.
    pub path: String,
    /// The cookie name.
    pub name: String,
    /// The whole cookie as the engine encodes it; it holds the value.
    pub payload: String,
}

impl std::fmt::Debug for CookieRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CookieRow")
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// The cookies of `workspace_id`, by domain, path and name.
    pub fn cookies(&self, workspace_id: &str) -> Result<Vec<CookieRow>, StateError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT domain, path, name, payload FROM cookies WHERE workspace_id = ?1
                 ORDER BY domain, path, name",
            )
            .map_err(query_error(&self.path))?;
        let rows = statement
            .query_map(rusqlite::params![workspace_id], |row| {
                Ok(CookieRow {
                    domain: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                    payload: row.get(3)?,
                })
            })
            .map_err(query_error(&self.path))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(query_error(&self.path))
    }

    /// Stores `cookie` in the jar of `workspace_id`, replacing the one with its domain, path
    /// and name.
    pub fn put_cookie(&mut self, workspace_id: &str, cookie: &CookieRow) -> Result<(), StateError> {
        self.conn
            .execute(
                "INSERT INTO cookies (workspace_id, domain, path, name, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (workspace_id, domain, path, name)
                 DO UPDATE SET payload = excluded.payload",
                rusqlite::params![
                    workspace_id,
                    cookie.domain,
                    cookie.path,
                    cookie.name,
                    cookie.payload
                ],
            )
            .map_err(query_error(&self.path))?;
        Ok(())
    }

    /// Deletes one cookie of `workspace_id`, reporting whether there was one.
    pub fn delete_cookie(
        &mut self,
        workspace_id: &str,
        domain: &str,
        path: &str,
        name: &str,
    ) -> Result<bool, StateError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM cookies
                 WHERE workspace_id = ?1 AND domain = ?2 AND path = ?3 AND name = ?4",
                rusqlite::params![workspace_id, domain, path, name],
            )
            .map_err(query_error(&self.path))?;
        Ok(changed > 0)
    }

    /// Deletes every cookie of `workspace_id`, returning how many there were.
    pub fn clear_cookies(&mut self, workspace_id: &str) -> Result<usize, StateError> {
        self.conn
            .execute(
                "DELETE FROM cookies WHERE workspace_id = ?1",
                rusqlite::params![workspace_id],
            )
            .map_err(query_error(&self.path))
    }
}
