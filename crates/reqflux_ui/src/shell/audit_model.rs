//! What the audit view shows of the log (app-pending S2): the filters of the toolbar, the
//! timeline strip and the text of a row's target. Pure, so it is tested without a window.

use reqflux_engine::audit::{AuditRecord, Decision};

/// The time range of the toolbar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Range {
    Quarter,
    Hour,
    SixHours,
    #[default]
    Day,
    Week,
}

/// The ranges in toolbar order.
pub(crate) const RANGES: [Range; 5] = [
    Range::Quarter,
    Range::Hour,
    Range::SixHours,
    Range::Day,
    Range::Week,
];

impl Range {
    /// The segment label, which is also its id.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Quarter => "15m",
            Self::Hour => "1h",
            Self::SixHours => "6h",
            Self::Day => "24h",
            Self::Week => "7d",
        }
    }

    /// The range of the label `label`.
    pub(crate) fn of(label: &str) -> Option<Self> {
        RANGES.into_iter().find(|range| range.label() == label)
    }

    /// The length in milliseconds.
    pub(crate) fn millis(self) -> i64 {
        const MINUTE: i64 = 60_000;
        match self {
            Self::Quarter => 15 * MINUTE,
            Self::Hour => 60 * MINUTE,
            Self::SixHours => 6 * 60 * MINUTE,
            Self::Day => 24 * 60 * MINUTE,
            Self::Week => 7 * 24 * 60 * MINUTE,
        }
    }
}

/// The Class multi-select: its items' labels and the class each keeps, which is also the
/// item's value.
pub(crate) const CLASSES: [(&str, &str); 4] = [
    ("Read", "read"),
    ("Write", "write"),
    ("Execute", "execute"),
    ("Destructive", "destructive"),
];

/// The Outcome multi-select: its items' labels, values and the decision each keeps.
pub(crate) const OUTCOMES: [(&str, &str, Decision); 2] = [
    ("Allowed", "allowed", Decision::Allowed),
    ("Refused", "refused", Decision::Refused),
];

/// The filters of the toolbar. An empty class or outcome set keeps every entry, as the
/// multi-select's `Class · all` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Filters {
    pub(crate) query: String,
    pub(crate) range: Range,
    pub(crate) class: Vec<&'static str>,
    pub(crate) outcome: Vec<Decision>,
}

impl Filters {
    /// Whether any filter differs from its default.
    pub(crate) fn is_filtered(&self) -> bool {
        *self != Self::default()
    }

    /// Whether `entry` passes every filter at `now`. An entry whose time did not parse
    /// cannot be placed in a range, so the range keeps it.
    fn matches(&self, entry: &AuditRecord, now: i64) -> bool {
        let in_range = entry
            .millis
            .is_none_or(|millis| now - millis <= self.range.millis());
        in_range
            && (self.class.is_empty() || self.class.contains(&entry.class.as_str()))
            && (self.outcome.is_empty() || self.outcome.contains(&entry.decision))
            && searched(entry, &self.query)
    }
}

/// The entries `filters` keep at `now`, with their index in `entries`, in order.
pub(crate) fn visible<'a>(
    entries: &'a [AuditRecord],
    filters: &Filters,
    now: i64,
) -> Vec<(usize, &'a AuditRecord)> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| filters.matches(entry, now))
        .collect()
}

/// One bar of the timeline: the calls in it and how many were refused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Bucket {
    pub(crate) total: usize,
    pub(crate) refused: usize,
}

/// The bars of the timeline.
pub(crate) const BUCKETS: usize = 48;

/// The [`BUCKETS`] bars of `shown` over `range` ending at `now`, oldest first.
pub(crate) fn timeline(shown: &[(usize, &AuditRecord)], range: Range, now: i64) -> Vec<Bucket> {
    let mut buckets = vec![Bucket::default(); BUCKETS];
    let width = (range.millis() / BUCKETS as i64).max(1);
    for (_, entry) in shown {
        let Some(ago) = entry.millis.map(|millis| now - millis) else {
            continue;
        };
        let Ok(back) = usize::try_from(ago / width) else {
            continue;
        };
        let Some(bucket) = (BUCKETS - 1)
            .checked_sub(back)
            .and_then(|index| buckets.get_mut(index))
        else {
            continue;
        };
        bucket.total += 1;
        if entry.decision == Decision::Refused {
            bucket.refused += 1;
        }
    }
    buckets
}

/// The target of a row: the method and masked URL of an execute, else the targets, each named
/// by `name`, joined by ` · `.
pub(crate) fn target_text(entry: &AuditRecord, name: impl Fn(&str) -> String) -> String {
    match (&entry.method, &entry.url) {
        (Some(method), Some(url)) => format!("{method} {url}"),
        _ => entry
            .targets
            .iter()
            .map(|id| name(id))
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// Whether `entry`'s tool, class, targets, method, URL, reason or workspace contains `query`,
/// ignoring case; an empty query matches.
fn searched(entry: &AuditRecord, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let contains = |text: &str| text.to_lowercase().contains(&query);
    contains(&entry.tool)
        || contains(&entry.class)
        || entry.targets.iter().any(|target| contains(target))
        || [
            &entry.method,
            &entry.url,
            &entry.reason,
            &entry.workspace_id,
        ]
        .into_iter()
        .flatten()
        .any(|text| contains(text))
}

#[cfg(test)]
#[path = "audit_model_tests.rs"]
mod tests;
