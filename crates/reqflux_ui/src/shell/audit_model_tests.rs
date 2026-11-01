//! The audit view's filters, timeline and labels (app-pending S2).

use reqflux_engine::audit::{AuditRecord, Decision};

use super::{CLASSES, Filters, OUTCOMES, Range, target_text, timeline, visible};

const NOW: i64 = 1_793_541_718_310;
const MINUTE: i64 = 60_000;

fn record(tool: &str, class: &str, decision: Decision, ago: i64) -> AuditRecord {
    AuditRecord {
        time: String::new(),
        millis: Some(NOW - ago),
        tool: tool.to_owned(),
        class: class.to_owned(),
        decision,
        reason: (decision == Decision::Refused).then(|| "denied by policy".to_owned()),
        workspace_id: Some("01W".to_owned()),
        targets: vec!["01R".to_owned()],
        method: None,
        url: None,
    }
}

fn tools(entries: &[AuditRecord], filters: &Filters) -> Vec<String> {
    visible(entries, filters, NOW)
        .into_iter()
        .map(|(_, entry)| entry.tool.clone())
        .collect()
}

fn log() -> Vec<AuditRecord> {
    vec![
        record("request_execute", "execute", Decision::Allowed, MINUTE),
        record("item_delete", "destructive", Decision::Refused, 30 * MINUTE),
        record(
            "request_update",
            "write",
            Decision::Allowed,
            3 * 60 * MINUTE,
        ),
        record(
            "environment_set",
            "write",
            Decision::Refused,
            2 * 24 * 60 * MINUTE,
        ),
    ]
}

#[test]
fn the_default_range_is_the_last_24_hours() {
    let filters = Filters::default();
    assert_eq!(filters.range, Range::Day);
    assert_eq!(
        tools(&log(), &filters),
        ["request_execute", "item_delete", "request_update"]
    );
}

#[test]
fn each_range_keeps_what_falls_inside_it() {
    let cases = [
        (Range::Quarter, 1),
        (Range::Hour, 2),
        (Range::SixHours, 3),
        (Range::Day, 3),
        (Range::Week, 4),
    ];
    for (range, count) in cases {
        let filters = Filters {
            range,
            ..Filters::default()
        };
        assert_eq!(tools(&log(), &filters).len(), count, "{range:?}");
    }
}

#[test]
fn an_entry_without_a_time_is_always_shown() {
    let mut entries = log();
    entries.push(AuditRecord {
        millis: None,
        ..record("odd", "write", Decision::Allowed, 0)
    });
    let filters = Filters {
        range: Range::Quarter,
        ..Filters::default()
    };
    assert_eq!(tools(&entries, &filters), ["request_execute", "odd"]);
}

#[test]
fn class_and_outcome_narrow_the_rows() {
    let filters = Filters {
        range: Range::Week,
        class: vec!["write"],
        ..Filters::default()
    };
    assert_eq!(
        tools(&log(), &filters),
        ["request_update", "environment_set"]
    );
    let filters = Filters {
        outcome: vec![Decision::Refused],
        ..filters
    };
    assert_eq!(tools(&log(), &filters), ["environment_set"]);
}

#[test]
fn several_classes_or_outcomes_keep_each_of_them() {
    let filters = Filters {
        range: Range::Week,
        class: vec!["execute", "destructive"],
        ..Filters::default()
    };
    assert_eq!(tools(&log(), &filters), ["request_execute", "item_delete"]);
    let filters = Filters {
        range: Range::Week,
        outcome: vec![Decision::Allowed, Decision::Refused],
        ..Filters::default()
    };
    assert_eq!(tools(&log(), &filters).len(), 4);
    let filters = Filters {
        class: vec!["write", "destructive"],
        outcome: vec![Decision::Refused],
        ..filters
    };
    assert_eq!(tools(&log(), &filters), ["item_delete", "environment_set"]);
}

#[test]
fn the_search_matches_tool_target_url_and_reason_ignoring_case() {
    let mut entries = log();
    if let Some(first) = entries.first_mut() {
        first.method = Some("POST".to_owned());
        first.url = Some("https://api.test/v1/Invoices".to_owned());
    }
    let search = |query: &str| {
        tools(
            &entries,
            &Filters {
                query: query.to_owned(),
                range: Range::Week,
                ..Filters::default()
            },
        )
    };
    assert_eq!(search("DELETE"), ["item_delete"]);
    assert_eq!(search("invoices"), ["request_execute"]);
    assert_eq!(search("denied"), ["item_delete", "environment_set"]);
    assert_eq!(search("01r").len(), 4);
    assert!(search("nothing").is_empty());
}

#[test]
fn the_filters_are_cleared_back_to_their_defaults() {
    let filters = Filters {
        query: "x".to_owned(),
        range: Range::Week,
        class: vec!["read"],
        outcome: vec![Decision::Allowed],
    };
    assert!(filters.is_filtered());
    assert!(!Filters::default().is_filtered());
}

#[test]
fn the_selects_offer_each_class_and_outcome() {
    let classes: Vec<&str> = CLASSES.iter().map(|&(_, class)| class).collect();
    assert_eq!(classes, ["read", "write", "execute", "destructive"]);
    assert_eq!(
        OUTCOMES.get(1),
        Some(&("Refused", "refused", Decision::Refused))
    );
}

#[test]
fn the_timeline_counts_calls_and_refusals_per_bucket() {
    let entries = log();
    let shown = visible(&entries, &Filters::default(), NOW);
    let buckets = timeline(&shown, Range::Day, NOW);
    assert_eq!(buckets.len(), 48);
    let total: usize = buckets.iter().map(|bucket| bucket.total).sum();
    let refused: usize = buckets.iter().map(|bucket| bucket.refused).sum();
    assert_eq!((total, refused), (3, 1));
    // Half-hour bars: the call a minute ago is in the last one, the refusal half an hour ago
    // in the one before.
    assert_eq!(buckets.last().map(|bucket| bucket.total), Some(1));
    assert_eq!(
        buckets.get(46).copied().map(|bucket| bucket.refused),
        Some(1)
    );
}

#[test]
fn the_target_is_the_request_line_else_the_targets() {
    let mut entry = record("item_delete", "destructive", Decision::Refused, 0);
    entry.targets = vec!["01A".to_owned(), "01B".to_owned()];
    assert_eq!(target_text(&entry, |id| id.to_owned()), "01A · 01B");
    assert_eq!(
        target_text(&entry, |id| if id == "01A" {
            "Ping".to_owned()
        } else {
            id.to_owned()
        }),
        "Ping · 01B"
    );
    entry.method = Some("GET".to_owned());
    entry.url = Some("https://x.test/?key=*****".to_owned());
    assert_eq!(
        target_text(&entry, |id| id.to_owned()),
        "GET https://x.test/?key=*****"
    );
}
