//! The History panel's filter and day groups, without GPUI.

use super::{HistoryRow, groups};

fn row(entry: i64, name: &str, url: &str, status: Option<u16>, day: &str) -> HistoryRow {
    HistoryRow {
        entry,
        method: "GET".to_owned(),
        name: name.to_owned(),
        url: url.to_owned(),
        status,
        day: day.to_owned(),
        time: "14:32".to_owned(),
        deleted: false,
        selected: false,
    }
}

fn rows() -> Vec<HistoryRow> {
    vec![
        row(
            4,
            "Create invoice",
            "https://api/v1/invoices",
            Some(201),
            "Today",
        ),
        row(
            3,
            "List invoices",
            "https://api/v1/invoices",
            Some(422),
            "Today",
        ),
        row(2, "Token", "https://auth/token", None, "Yesterday"),
        row(
            1,
            "Get invoice",
            "https://api/v1/invoices/1",
            Some(200),
            "Thu, Oct 29",
        ),
    ]
}

fn entries(rows: &[HistoryRow], query: &str) -> Vec<(String, Vec<i64>)> {
    groups(rows, query)
        .into_iter()
        .map(|group| {
            (
                group.day.to_owned(),
                group.rows.iter().map(|row| row.entry).collect(),
            )
        })
        .collect()
}

#[test]
fn rows_group_by_consecutive_day_in_order() {
    assert_eq!(
        entries(&rows(), ""),
        vec![
            ("Today".to_owned(), vec![4, 3]),
            ("Yesterday".to_owned(), vec![2]),
            ("Thu, Oct 29".to_owned(), vec![1]),
        ]
    );
}

#[test]
fn the_filter_matches_name_url_or_status_prefix() {
    let rows = rows();
    assert_eq!(
        entries(&rows, "INVOICE"),
        vec![
            ("Today".to_owned(), vec![4, 3]),
            ("Thu, Oct 29".to_owned(), vec![1]),
        ]
    );
    assert_eq!(
        entries(&rows, "auth/"),
        vec![("Yesterday".to_owned(), vec![2])]
    );
    assert_eq!(entries(&rows, "4"), vec![("Today".to_owned(), vec![3])]);
    assert_eq!(
        entries(&rows, "failed"),
        vec![("Yesterday".to_owned(), vec![2])]
    );
    assert_eq!(
        entries(&rows, "  token "),
        vec![("Yesterday".to_owned(), vec![2])]
    );
    assert!(entries(&rows, "404").is_empty());
}

#[test]
fn a_row_reads_method_name_and_status() {
    let failed = row(1, "Token", "https://auth/token", None, "Today");
    assert_eq!(failed.text(), "GET Token · failed");
    assert_eq!(
        rows().first().map(HistoryRow::text).as_deref(),
        Some("GET Create invoice · 201")
    );
}
