//! The `document_drafts` table of schema v8 (v1-close S5b): the unsaved text of a folder's
//! `.http` document in the Document layout, keyed by workspace id and folder id. It is a
//! table of its own, not a row of `drafts`, so the request-draft loaders never meet a
//! payload that is not a request document.
//!
//! The stored [`Draft`]'s `payload` is the document text as typed and its `base_hash` the
//! content hash of the canonical document it was edited from, so a front end can tell when
//! the folder changed underneath the text.

use super::{AppState, Draft, StateError, draft_from_row, now_millis, query_error};

/// The table schema version 8 adds.
pub(super) const SCHEMA_V8: &str = "
CREATE TABLE IF NOT EXISTS document_drafts (
    workspace_id TEXT NOT NULL,
    folder_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    base_hash TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, folder_id)
);
";

impl AppState {
    /// Stores the document text `draft` of the folder `folder_id` in `workspace_id`,
    /// overwriting any previous one and stamping it with the current time.
    pub fn put_document_draft(
        &mut self,
        workspace_id: &str,
        folder_id: &str,
        draft: Draft,
    ) -> Result<(), StateError> {
        self.conn
            .execute(
                "INSERT INTO document_drafts (workspace_id, folder_id, payload, base_hash, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (workspace_id, folder_id) DO UPDATE SET
                     payload = excluded.payload,
                     base_hash = excluded.base_hash,
                     updated_at = excluded.updated_at",
                rusqlite::params![
                    workspace_id,
                    folder_id,
                    draft.payload,
                    draft.base_hash,
                    now_millis()
                ],
            )
            .map_err(query_error(&self.path))?;
        Ok(())
    }

    /// The document text draft of the folder `folder_id` in `workspace_id`, if any.
    pub fn document_draft(
        &self,
        workspace_id: &str,
        folder_id: &str,
    ) -> Result<Option<Draft>, StateError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT payload, base_hash, updated_at FROM document_drafts
                 WHERE workspace_id = ?1 AND folder_id = ?2",
            )
            .map_err(query_error(&self.path))?;
        let mut rows = statement
            .query(rusqlite::params![workspace_id, folder_id])
            .map_err(query_error(&self.path))?;
        match rows.next().map_err(query_error(&self.path))? {
            Some(row) => draft_from_row(row)
                .map(Some)
                .map_err(query_error(&self.path)),
            None => Ok(None),
        }
    }

    /// Deletes the document text draft of the folder `folder_id` in `workspace_id`,
    /// reporting whether there was one.
    pub fn delete_document_draft(
        &mut self,
        workspace_id: &str,
        folder_id: &str,
    ) -> Result<bool, StateError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM document_drafts WHERE workspace_id = ?1 AND folder_id = ?2",
                rusqlite::params![workspace_id, folder_id],
            )
            .map_err(query_error(&self.path))?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::{AppState, Draft};

    fn state_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("state.sqlite3")
    }

    #[test]
    fn a_document_draft_round_trips_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();
        let text = "### List\nGET https://api.test/\n";

        store
            .put_document_draft("ws", "folder", Draft::new(text, Some("base".to_owned())))
            .unwrap();
        let stored = store.document_draft("ws", "folder").unwrap().unwrap();
        assert_eq!(stored.payload, text);
        assert_eq!(stored.base_hash.as_deref(), Some("base"));
        assert!(store.document_draft("other", "folder").unwrap().is_none());

        assert!(store.delete_document_draft("ws", "folder").unwrap());
        assert!(store.document_draft("ws", "folder").unwrap().is_none());
        assert!(!store.delete_document_draft("ws", "folder").unwrap());
    }

    #[test]
    fn a_document_draft_is_not_a_request_draft() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppState::open(&state_path(&dir)).unwrap();

        store
            .put_document_draft("ws", "folder", Draft::new("### A\nGET x\n", None))
            .unwrap();

        assert!(store.drafts("ws").unwrap().is_empty());
        assert!(store.draft("ws", "folder").unwrap().is_none());
    }

    #[test]
    fn a_version_7_file_upgrades_and_keeps_its_request_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(&dir);
        AppState::open(&path)
            .unwrap()
            .put_draft("ws", "req", Draft::new("a = 1", Some("hash".to_owned())))
            .unwrap();
        // Back to the schema before this table: version 7 had no `document_drafts`.
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE document_drafts; PRAGMA user_version = 7;")
            .unwrap();
        drop(conn);

        let mut store = AppState::open(&path).unwrap();

        let kept = store.draft("ws", "req").unwrap().unwrap();
        assert_eq!(kept.payload, "a = 1");
        assert_eq!(kept.base_hash.as_deref(), Some("hash"));
        store
            .put_document_draft("ws", "folder", Draft::new("### A\n", None))
            .unwrap();
        assert!(store.document_draft("ws", "folder").unwrap().is_some());
        let version: i64 = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 8);
    }
}
