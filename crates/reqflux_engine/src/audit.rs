//! The MCP audit log (user-config-mcp-policy S7): one JSON line per write, execute or
//! destructive call, allowed or refused, at `$XDG_STATE_HOME/reqflux/audit.log`, beside
//! the app state.
//!
//! A line holds the time, the tool, its class, the decision, the reason of a refusal, the
//! workspace id, the ids the call targets and, for an execute, the method and the URL masked
//! with the send's masker. It never holds an argument value. Each line is appended with
//! `O_APPEND` and synced before the call goes on, so a caller refuses a call whose line could
//! not be written.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::policy::ToolClass;
use crate::state::{StateError, default_state_path};

mod log;

pub use log::{AuditLog, AuditRecord, read_log};

/// Whether the policy let a call run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The call ran.
    Allowed,
    /// The call was refused before it touched anything.
    Refused,
}

impl Decision {
    /// The decision as the audit log spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Refused => "refused",
        }
    }
}

/// One audit line. Every field is an identifier or a masked value, never an argument value.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    /// The tool name.
    pub tool: &'a str,
    /// The tool's class.
    pub class: ToolClass,
    /// The policy's decision.
    pub decision: Decision,
    /// Why it was refused.
    pub reason: Option<&'a str>,
    /// The open workspace's id, if any.
    pub workspace_id: Option<&'a str>,
    /// The ids of the entities the call targets.
    pub targets: &'a [String],
    /// The method of an execute.
    pub method: Option<&'a str>,
    /// The URL of an execute, masked with the send's masker.
    pub url: Option<&'a str>,
}

/// Why an audit line could not be written.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// The state directory could not be located or created.
    #[error(transparent)]
    Directory(#[from] StateError),
    /// The file could not be opened, written or synced.
    #[error("cannot write the audit log {path}: {source}")]
    Write {
        /// The audit log.
        path: PathBuf,
        /// The IO failure.
        #[source]
        source: io::Error,
    },
}

/// The audit log path from the variable values, beside the app state.
#[must_use]
pub fn audit_path_in(state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    default_state_path(state_home, home).map(|state| state.with_file_name("audit.log"))
}

/// The user's audit log path, from `XDG_STATE_HOME` and `HOME`.
///
/// # Errors
///
/// Neither variable is set.
pub fn default_path() -> Result<PathBuf, AuditError> {
    let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    audit_path_in(state_home.as_deref(), home.as_deref())
        .ok_or(AuditError::Directory(StateError::NoStateDirectory))
}

/// Appends `entry` to the audit log at `path` and syncs it, creating the file owner-only
/// and its directory when missing.
///
/// # Errors
///
/// The directory or the file could not be created, written or synced.
pub fn append(path: &Path, entry: &AuditEntry<'_>) -> Result<(), AuditError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        crate::state::create_state_directory(parent)?;
    }
    let mut line = line(entry, SystemTime::now()).to_string();
    line.push('\n');
    let write_error = |source| AuditError::Write {
        path: path.to_owned(),
        source,
    };
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(write_error)?;
    file.write_all(line.as_bytes()).map_err(write_error)?;
    file.sync_data().map_err(write_error)
}

/// The JSON object of `entry` written at `now`.
fn line(entry: &AuditEntry<'_>, now: SystemTime) -> Value {
    let mut map = Map::new();
    map.insert("time".to_owned(), Value::String(rfc3339(now)));
    map.insert("tool".to_owned(), Value::String(entry.tool.to_owned()));
    map.insert(
        "class".to_owned(),
        Value::String(entry.class.name().to_owned()),
    );
    map.insert(
        "decision".to_owned(),
        Value::String(entry.decision.name().to_owned()),
    );
    if let Some(reason) = entry.reason {
        map.insert("reason".to_owned(), Value::String(reason.to_owned()));
    }
    map.insert(
        "workspace_id".to_owned(),
        entry
            .workspace_id
            .map_or(Value::Null, |id| Value::String(id.to_owned())),
    );
    map.insert(
        "targets".to_owned(),
        Value::Array(entry.targets.iter().cloned().map(Value::String).collect()),
    );
    if let Some(method) = entry.method {
        map.insert("method".to_owned(), Value::String(method.to_owned()));
    }
    if let Some(url) = entry.url {
        map.insert("url".to_owned(), Value::String(url.to_owned()));
    }
    Value::Object(map)
}

/// `time` as `YYYY-MM-DDTHH:MM:SS.mmmZ`, in UTC; [`log`] reads it back.
fn rfc3339(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs();
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// The proleptic Gregorian date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Reads the audit log at `path` as JSON values, one per line; a missing file is empty.
///
/// # Errors
///
/// The file could not be read; a line that is not JSON is skipped.
pub fn read(path: &Path) -> io::Result<Vec<Value>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use std::time::Duration;

    use super::*;

    #[test]
    fn times_are_utc_rfc3339() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        let time = UNIX_EPOCH + Duration::from_millis(1_709_251_199_123);
        assert_eq!(rfc3339(time), "2024-02-29T23:59:59.123Z");
    }

    #[test]
    fn the_audit_log_lives_beside_the_app_state() {
        assert_eq!(
            audit_path_in(Some(Path::new("/s")), Some(Path::new("/h"))),
            Some(PathBuf::from("/s/reqflux/audit.log"))
        );
        assert_eq!(
            audit_path_in(None, Some(Path::new("/h"))),
            Some(PathBuf::from("/h/.local/state/reqflux/audit.log"))
        );
    }

    #[test]
    fn lines_append_and_leave_out_absent_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("reqflux/audit.log");
        let targets = vec!["01J".to_owned()];
        let entry = AuditEntry {
            tool: "item_delete",
            class: ToolClass::Destructive,
            decision: Decision::Refused,
            reason: Some("no"),
            workspace_id: Some("w"),
            targets: &targets,
            method: None,
            url: None,
        };
        append(&path, &entry).unwrap();
        append(
            &path,
            &AuditEntry {
                decision: Decision::Allowed,
                reason: None,
                workspace_id: None,
                ..entry
            },
        )
        .unwrap();
        let lines = read(&path).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["decision"], "refused");
        assert_eq!(lines[0]["reason"], "no");
        assert_eq!(lines[0]["class"], "destructive");
        assert_eq!(lines[0]["targets"][0], "01J");
        assert_eq!(lines[1]["decision"], "allowed");
        assert!(lines[1].get("reason").is_none());
        assert!(lines[1]["workspace_id"].is_null());
        assert!(lines[1].get("url").is_none());
    }
}
