//! Reading the audit log back for the app's audit view (app-pending S2): entries newest first.
//! A line that does not parse is reported by its number and never returned, so the view can
//! say so without showing it raw. The log holds no secret, so neither does what it returns.

use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;

use super::Decision;

/// One line of the audit log as read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// The time as written.
    pub time: String,
    /// The time in milliseconds since the Unix epoch.
    pub millis: Option<i64>,
    /// The tool name.
    pub tool: String,
    /// The tool's class name.
    pub class: String,
    /// The policy's decision.
    pub decision: Decision,
    /// Why it was refused.
    pub reason: Option<String>,
    /// The workspace id.
    pub workspace_id: Option<String>,
    /// The ids the call targets.
    pub targets: Vec<String>,
    /// The method of an execute.
    pub method: Option<String>,
    /// The masked URL of an execute.
    pub url: Option<String>,
}

/// The audit log as read back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditLog {
    /// The entries, newest first.
    pub entries: Vec<AuditRecord>,
    /// The 1-based numbers of the lines that did not parse.
    pub skipped: Vec<usize>,
}

/// One line as written by [`super::append`].
#[derive(Deserialize)]
struct Line {
    time: String,
    tool: String,
    class: String,
    decision: Decision,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

impl From<Line> for AuditRecord {
    fn from(line: Line) -> Self {
        Self {
            millis: parse_millis(&line.time),
            time: line.time,
            tool: line.tool,
            class: line.class,
            decision: line.decision,
            reason: line.reason,
            workspace_id: line.workspace_id,
            targets: line.targets,
            method: line.method,
            url: line.url,
        }
    }
}

/// Reads the audit log at `path`, newest first; a missing file is empty and a blank line is
/// ignored.
///
/// # Errors
///
/// The file could not be read.
pub fn read_log(path: &Path) -> io::Result<AuditLog> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(AuditLog::default()),
        Err(error) => return Err(error),
    };
    let mut log = AuditLog::default();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Line>(line) {
            Ok(line) => log.entries.push(line.into()),
            Err(_) => log.skipped.push(index + 1),
        }
    }
    log.entries.reverse();
    Ok(log)
}

/// The milliseconds since the Unix epoch of a `YYYY-MM-DDTHH:MM:SS.mmmZ` time.
fn parse_millis(time: &str) -> Option<i64> {
    let (date, clock) = time.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, millis) = clock.split_once('.').unwrap_or((clock, "0"));
    let mut clock = clock.splitn(3, ':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let millis: i64 = millis.parse().ok()?;
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hour)
        && (0..60).contains(&minute)
        && (0..61).contains(&second)
        && (0..1000).contains(&millis);
    if !valid {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some(seconds * 1000 + millis)
}

/// The days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm), the
/// inverse of [`super::civil_from_days`].
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_invert_the_writer() {
        for days in [-1, 0, 59, 60, 11_016, 19_782, 20_758] {
            let (year, month, day) = super::super::civil_from_days(days);
            assert_eq!(
                days_from_civil(year, i64::from(month), i64::from(day)),
                days
            );
        }
    }

    #[test]
    fn times_parse_back_to_millis() {
        assert_eq!(parse_millis("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_millis("2024-02-29T23:59:59.123Z"),
            Some(1_709_251_199_123)
        );
        assert_eq!(parse_millis("2024-13-01T00:00:00.000Z"), None);
        assert_eq!(parse_millis("2024-02-29 23:59:59"), None);
    }
}
