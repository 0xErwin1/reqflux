//! The send history table of the app state (history S1, S2, S4, S5).

use rusqlite::{OptionalExtension, Row, params};

use super::{AppState, StateError, query_error};
use crate::sent::{Masker, SentBody, SentRequest};

/// The history table of schema version 2. Request headers and response headers are JSON
/// arrays of `[name, value]` pairs in order; `request_body_kind` is `none`, `text` or
/// `omitted`, with the text or the omission note in `request_body`. A failed send has
/// `error` set and no response columns.
pub(super) const SCHEMA_V2: &str = "
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    sent_at INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    request_name TEXT NOT NULL,
    request_path TEXT NOT NULL,
    document TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    request_headers TEXT NOT NULL,
    request_body_kind TEXT NOT NULL,
    request_body TEXT,
    status INTEGER,
    reason TEXT,
    response_headers TEXT,
    response_body BLOB,
    truncated INTEGER NOT NULL DEFAULT 0,
    size INTEGER,
    elapsed_ms INTEGER NOT NULL,
    error TEXT
);
CREATE INDEX IF NOT EXISTS history_workspace ON history (workspace_id, id);
";

/// How many entries a workspace keeps; recording prunes the older ones.
pub const HISTORY_MAX_ENTRIES: usize = 500;

/// The largest response body stored whole, in bytes (5 MiB); a longer one is cut.
pub const HISTORY_MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// The limits recording applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    /// How many entries a workspace keeps.
    pub max_entries: usize,
    /// The largest response body stored whole, in bytes.
    pub max_body_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_entries: HISTORY_MAX_ENTRIES,
            max_body_bytes: HISTORY_MAX_BODY_BYTES,
        }
    }
}

/// One send as history records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    /// Milliseconds since the Unix epoch when the request was sent.
    pub sent_at: i64,
    /// The id of the request that was sent.
    pub request_id: String,
    /// The request's name when it was sent.
    pub request_name: String,
    /// The request's slug path when it was sent.
    pub request_path: String,
    /// The request document that was sent, as TOML, the same text a draft stores; its
    /// `{{references}}` stay as written.
    pub document: String,
    /// The resolved request as sent, secrets masked.
    pub sent: SentRequest,
    /// What came back.
    pub outcome: HistoryOutcome,
}

/// The result of a recorded send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// A response arrived.
    Response(RecordedResponse),
    /// The send failed before a response.
    Failed {
        /// The error message.
        error: String,
        /// Milliseconds from sending to the failure.
        elapsed_ms: u64,
    },
}

/// A recorded response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedResponse {
    /// The status code.
    pub status: u16,
    /// The reason phrase, when there is one.
    pub reason: Option<String>,
    /// The headers in received order.
    pub headers: Vec<(String, String)>,
    /// The body, cut at the body limit when `truncated`.
    pub body: Vec<u8>,
    /// Whether `body` was cut.
    pub truncated: bool,
    /// Milliseconds from sending to reading the whole body.
    pub elapsed_ms: u64,
    /// The size of the whole body in bytes, before any cut.
    pub size: u64,
}

impl RecordedResponse {
    /// The record of `response`, whole; recording applies the body limit.
    #[must_use]
    pub fn from_response(response: &reqflux_http::Response) -> Self {
        Self {
            status: response.status,
            reason: response.reason.clone(),
            headers: response.headers.clone(),
            body: response.body.clone(),
            truncated: false,
            elapsed_ms: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
            size: u64::try_from(response.size()).unwrap_or(u64::MAX),
        }
    }

    /// The record of `response` with its secrets masked by `masker`, the send's
    /// [`crate::Masker`], so a secret the server echoes back never reaches history in
    /// clear: every header goes through [`crate::Masker::header`] (a credential header such
    /// as `Set-Cookie` masked whole), and the reason and a UTF-8 text body through
    /// [`crate::Masker::mask`]. A body that is not UTF-8 is stored as received, since the
    /// masker works on text. `size` stays the size received.
    #[must_use]
    pub fn from_response_masked(response: &reqflux_http::Response, masker: &Masker) -> Self {
        let mut recorded = Self::from_response(response);
        recorded.reason = recorded.reason.map(|reason| masker.mask(&reason));
        recorded.headers = recorded
            .headers
            .iter()
            .map(|(name, value)| masker.header(name, value))
            .collect();
        if let Ok(text) = std::str::from_utf8(&recorded.body) {
            recorded.body = masker.mask(text).into_bytes();
        }
        recorded
    }
}

/// A stored history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The entry id, unique in the state file and increasing with each record.
    pub id: i64,
    /// The workspace the send belongs to.
    pub workspace_id: String,
    /// What was recorded.
    pub record: HistoryRecord,
}

const COLUMNS: &str = "id, workspace_id, sent_at, request_id, request_name, request_path, \
     document, method, url, request_headers, request_body_kind, request_body, status, reason, \
     response_headers, response_body, truncated, size, elapsed_ms, error";

impl AppState {
    /// Records `record` for `workspace_id` with the default [`HistoryLimits`] and returns
    /// the new entry id. See [`AppState::record_history_with_limits`].
    ///
    /// # Errors
    ///
    /// The write failed.
    pub fn record_history(
        &mut self,
        workspace_id: &str,
        record: HistoryRecord,
    ) -> Result<i64, StateError> {
        self.record_history_with_limits(workspace_id, record, HistoryLimits::default())
    }

    /// Records `record` for `workspace_id` and returns the new entry id. A response body
    /// longer than `limits.max_body_bytes` is stored cut with `truncated` set; then the
    /// workspace is pruned to its newest `limits.max_entries` entries, in one transaction.
    ///
    /// # Errors
    ///
    /// The write failed; nothing is recorded or pruned.
    pub fn record_history_with_limits(
        &mut self,
        workspace_id: &str,
        mut record: HistoryRecord,
        limits: HistoryLimits,
    ) -> Result<i64, StateError> {
        if let HistoryOutcome::Response(response) = &mut record.outcome
            && response.body.len() > limits.max_body_bytes
        {
            let cut = text_boundary(&response.body, limits.max_body_bytes);
            response.body.truncate(cut);
            response.truncated = true;
        }
        let corrupt = |reason: String| StateError::Corrupt {
            path: self.path.clone(),
            reason,
        };
        let request_headers = encode_headers(&record.sent.headers).map_err(corrupt)?;
        let (body_kind, body_text) = match &record.sent.body {
            SentBody::None => ("none", None),
            SentBody::Text(text) => ("text", Some(text.as_str())),
            SentBody::Omitted(note) => ("omitted", Some(note.as_str())),
        };
        let (response, error, elapsed_ms) = match &record.outcome {
            HistoryOutcome::Response(response) => (Some(response), None, response.elapsed_ms),
            HistoryOutcome::Failed { error, elapsed_ms } => (None, Some(error), *elapsed_ms),
        };
        let response_headers = response
            .map(|response| encode_headers(&response.headers))
            .transpose()
            .map_err(corrupt)?;
        let keep = i64::try_from(limits.max_entries).unwrap_or(i64::MAX);

        let path = self.path.clone();
        let transaction = self.conn.transaction().map_err(query_error(&path))?;
        transaction
            .execute(
                "INSERT INTO history (workspace_id, sent_at, request_id, request_name,
                     request_path, document, method, url, request_headers, request_body_kind,
                     request_body, status, reason, response_headers, response_body, truncated,
                     size, elapsed_ms, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19)",
                params![
                    workspace_id,
                    record.sent_at,
                    record.request_id,
                    record.request_name,
                    record.request_path,
                    record.document,
                    record.sent.method,
                    record.sent.url,
                    request_headers,
                    body_kind,
                    body_text,
                    response.map(|response| response.status),
                    response.and_then(|response| response.reason.as_deref()),
                    response_headers,
                    response.map(|response| response.body.as_slice()),
                    response.is_some_and(|response| response.truncated),
                    response.map(|response| to_sql_int(response.size)),
                    to_sql_int(elapsed_ms),
                    error,
                ],
            )
            .map_err(query_error(&path))?;
        let id = transaction.last_insert_rowid();
        transaction
            .execute(
                "DELETE FROM history WHERE workspace_id = ?1 AND id NOT IN (
                     SELECT id FROM history WHERE workspace_id = ?1 ORDER BY id DESC LIMIT ?2
                 )",
                params![workspace_id, keep],
            )
            .map_err(query_error(&path))?;
        transaction.commit().map_err(query_error(&path))?;
        Ok(id)
    }

    /// The newest `limit` entries of `workspace_id`, newest first.
    ///
    /// # Errors
    ///
    /// The read failed or an entry is unreadable.
    pub fn history(
        &self,
        workspace_id: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StateError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM history WHERE workspace_id = ?1 ORDER BY id DESC LIMIT ?2"
            ))
            .map_err(query_error(&self.path))?;
        let rows = statement
            .query_map(params![workspace_id, limit], entry_from_row)
            .map_err(query_error(&self.path))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(self.decoded(row)?);
        }
        Ok(entries)
    }

    /// The entry with id `id`, if any.
    ///
    /// # Errors
    ///
    /// The read failed or the entry is unreadable.
    pub fn history_entry(&self, id: i64) -> Result<Option<HistoryEntry>, StateError> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM history WHERE id = ?1"),
                params![id],
                entry_from_row,
            )
            .optional();
        match row {
            Ok(Some(row)) => self.decoded(Ok(row)).map(Some),
            Ok(None) => Ok(None),
            Err(source) => Err(query_error(&self.path)(source)),
        }
    }

    /// Deletes every history entry of `workspace_id`, reporting how many there were.
    ///
    /// # Errors
    ///
    /// The write failed.
    pub fn clear_history(&mut self, workspace_id: &str) -> Result<usize, StateError> {
        self.conn
            .execute(
                "DELETE FROM history WHERE workspace_id = ?1",
                params![workspace_id],
            )
            .map_err(query_error(&self.path))
    }

    /// Turns a read row into an entry, decoding its JSON columns.
    fn decoded(&self, row: Result<RawEntry, rusqlite::Error>) -> Result<HistoryEntry, StateError> {
        let raw = row.map_err(query_error(&self.path))?;
        raw.decode().map_err(|reason| StateError::Corrupt {
            path: self.path.clone(),
            reason,
        })
    }
}

/// A history row as stored, before its JSON columns are decoded.
struct RawEntry {
    id: i64,
    workspace_id: String,
    sent_at: i64,
    request_id: String,
    request_name: String,
    request_path: String,
    document: String,
    method: String,
    url: String,
    request_headers: String,
    request_body_kind: String,
    request_body: Option<String>,
    status: Option<u16>,
    reason: Option<String>,
    response_headers: Option<String>,
    response_body: Option<Vec<u8>>,
    truncated: bool,
    size: Option<i64>,
    elapsed_ms: i64,
    error: Option<String>,
}

fn entry_from_row(row: &Row<'_>) -> Result<RawEntry, rusqlite::Error> {
    Ok(RawEntry {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        sent_at: row.get(2)?,
        request_id: row.get(3)?,
        request_name: row.get(4)?,
        request_path: row.get(5)?,
        document: row.get(6)?,
        method: row.get(7)?,
        url: row.get(8)?,
        request_headers: row.get(9)?,
        request_body_kind: row.get(10)?,
        request_body: row.get(11)?,
        status: row.get(12)?,
        reason: row.get(13)?,
        response_headers: row.get(14)?,
        response_body: row.get(15)?,
        truncated: row.get(16)?,
        size: row.get(17)?,
        elapsed_ms: row.get(18)?,
        error: row.get(19)?,
    })
}

impl RawEntry {
    fn decode(self) -> Result<HistoryEntry, String> {
        let body = match (self.request_body_kind.as_str(), self.request_body) {
            ("none", _) => SentBody::None,
            ("text", Some(text)) => SentBody::Text(text),
            ("omitted", Some(note)) => SentBody::Omitted(note),
            (kind, _) => return Err(format!("unknown request body kind `{kind}`")),
        };
        let elapsed_ms = from_sql_int(self.elapsed_ms);
        let outcome = match (self.error, self.status) {
            (Some(error), _) => HistoryOutcome::Failed { error, elapsed_ms },
            (None, Some(status)) => HistoryOutcome::Response(RecordedResponse {
                status,
                reason: self.reason,
                headers: decode_headers(self.response_headers.as_deref().unwrap_or("[]"))?,
                body: self.response_body.unwrap_or_default(),
                truncated: self.truncated,
                elapsed_ms,
                size: self.size.map(from_sql_int).unwrap_or_default(),
            }),
            (None, None) => return Err("an entry has neither a status nor an error".to_owned()),
        };
        Ok(HistoryEntry {
            id: self.id,
            workspace_id: self.workspace_id,
            record: HistoryRecord {
                sent_at: self.sent_at,
                request_id: self.request_id,
                request_name: self.request_name,
                request_path: self.request_path,
                document: self.document,
                sent: SentRequest {
                    method: self.method,
                    url: self.url,
                    headers: decode_headers(&self.request_headers)?,
                    body,
                },
                outcome,
            },
        })
    }
}

/// Where to cut `body` to at most `limit` bytes: at `limit`, backed off to the last
/// character boundary when the bytes up to there are UTF-8 text cut inside a character.
/// A body that is not UTF-8 is cut at `limit`.
fn text_boundary(body: &[u8], limit: usize) -> usize {
    let Some(prefix) = body.get(..limit) else {
        return body.len();
    };
    match std::str::from_utf8(prefix) {
        Ok(_) => limit,
        // An incomplete character at the end has no `error_len`; anything else is binary.
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => limit,
    }
}

/// SQLite integers are signed; a count past `i64::MAX` saturates.
fn to_sql_int(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The reverse of [`to_sql_int`]; a negative value, which this module never writes, reads as 0.
fn from_sql_int(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn encode_headers(headers: &[(String, String)]) -> Result<String, String> {
    serde_json::to_string(headers).map_err(|error| error.to_string())
}

fn decode_headers(text: &str) -> Result<Vec<(String, String)>, String> {
    serde_json::from_str(text).map_err(|error| format!("unreadable headers: {error}"))
}
