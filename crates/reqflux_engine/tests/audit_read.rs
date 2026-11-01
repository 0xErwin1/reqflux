//! Reading the MCP audit log for the audit view (app-pending S2): entries newest first, a line
//! that does not parse reported by number and never returned, a missing file empty.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::fs;

use reqflux_engine::audit::{self, AuditEntry, Decision, read_log};
use reqflux_engine::policy::ToolClass;
use tempfile::TempDir;

const EXECUTE: &str = r#"{"time":"2026-11-01T14:01:58.310Z","tool":"request_execute","class":"execute","decision":"allowed","workspace_id":"01W","targets":["01R"],"method":"POST","url":"https://api.test/v1?key=*****"}"#;
const REFUSED: &str = r#"{"time":"2026-11-01T14:00:40.019Z","tool":"item_delete","class":"destructive","decision":"refused","reason":"destructive calls are denied","workspace_id":null,"targets":["01A","01B"]}"#;

fn log_with(lines: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.log");
    fs::write(&path, lines).unwrap();
    (dir, path)
}

#[test]
fn entries_come_newest_first_with_every_field() {
    let (_dir, path) = log_with(&format!("{REFUSED}\n{EXECUTE}\n"));
    let log = read_log(&path).unwrap();
    assert!(log.skipped.is_empty());
    assert_eq!(log.entries.len(), 2);
    let newest = &log.entries[0];
    assert_eq!(newest.time, "2026-11-01T14:01:58.310Z");
    assert_eq!(newest.millis, Some(1_793_541_718_310));
    assert_eq!(newest.tool, "request_execute");
    assert_eq!(newest.class, "execute");
    assert_eq!(newest.decision, Decision::Allowed);
    assert_eq!(newest.reason, None);
    assert_eq!(newest.workspace_id.as_deref(), Some("01W"));
    assert_eq!(newest.targets, ["01R"]);
    assert_eq!(newest.method.as_deref(), Some("POST"));
    assert_eq!(newest.url.as_deref(), Some("https://api.test/v1?key=*****"));
    let oldest = &log.entries[1];
    assert_eq!(oldest.decision, Decision::Refused);
    assert_eq!(
        oldest.reason.as_deref(),
        Some("destructive calls are denied")
    );
    assert_eq!(oldest.workspace_id, None);
    assert_eq!(oldest.targets, ["01A", "01B"]);
    assert_eq!(oldest.method, None);
}

#[test]
fn a_bad_line_is_reported_by_number_and_skipped() {
    let missing_tool =
        r#"{"time":"2026-11-01T14:00:00.000Z","class":"write","decision":"allowed","targets":[]}"#;
    let unknown_decision = r#"{"time":"2026-11-01T14:00:00.000Z","tool":"t","class":"write","decision":"maybe","targets":[]}"#;
    let (_dir, path) = log_with(&format!(
        "{REFUSED}\nnot json at all\n\n{missing_tool}\n{unknown_decision}\n{EXECUTE}\n"
    ));
    let log = read_log(&path).unwrap();
    assert_eq!(log.skipped, [2, 4, 5]);
    let tools: Vec<_> = log
        .entries
        .iter()
        .map(|entry| entry.tool.as_str())
        .collect();
    assert_eq!(tools, ["request_execute", "item_delete"]);
}

#[test]
fn a_time_that_does_not_parse_keeps_the_entry_without_millis() {
    let odd =
        r#"{"time":"yesterday","tool":"t","class":"write","decision":"allowed","targets":[]}"#;
    let (_dir, path) = log_with(&format!("{odd}\n"));
    let log = read_log(&path).unwrap();
    assert!(log.skipped.is_empty());
    assert_eq!(log.entries[0].millis, None);
    assert_eq!(log.entries[0].time, "yesterday");
}

#[test]
fn a_missing_log_is_empty() {
    let dir = TempDir::new().unwrap();
    let log = read_log(&dir.path().join("audit.log")).unwrap();
    assert!(log.entries.is_empty());
    assert!(log.skipped.is_empty());
}

#[test]
fn a_log_that_cannot_be_read_is_an_error() {
    let dir = TempDir::new().unwrap();
    assert!(read_log(dir.path()).is_err());
}

#[test]
fn what_append_writes_reads_back() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reqflux/audit.log");
    let targets = vec!["01J".to_owned()];
    audit::append(
        &path,
        &AuditEntry {
            tool: "request_execute",
            class: ToolClass::Execute,
            decision: Decision::Refused,
            reason: Some("no"),
            workspace_id: Some("w"),
            targets: &targets,
            method: Some("GET"),
            url: Some("https://x.test/"),
        },
    )
    .unwrap();
    let log = read_log(&path).unwrap();
    assert!(log.skipped.is_empty());
    let entry = &log.entries[0];
    assert_eq!(entry.tool, "request_execute");
    assert_eq!(entry.class, "execute");
    assert_eq!(entry.decision, Decision::Refused);
    assert!(entry.millis.is_some());
    // The raw reader the MCP tests use still reads the same line.
    assert_eq!(audit::read(&path).unwrap()[0]["tool"], "request_execute");
}
