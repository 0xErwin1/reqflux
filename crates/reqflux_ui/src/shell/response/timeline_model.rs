//! The Timeline view without GPUI (response-views/02): the engine's per-hop timings as rows
//! on one time axis. The engine reports TLS inside connect, no DNS for an IP address or a
//! reused connection, no connect for a reused one, one hop per redirect followed and the
//! download on the last hop only, so the rows say so instead of drawing empty bars.

use std::time::Duration;

use reqflux_engine::{HopTimings, Timings, WireRequest};

use crate::shell::request::elapsed_text;

/// The note at the end of the summary bar: the engine never times TLS on its own.
pub(crate) const TLS_NOTE: &str = "TLS is included in connect";

/// What the view says when the send kept no timings: a history entry recorded before
/// timings existed.
pub(crate) const NO_TIMINGS: &str = "No timings were recorded for this send";

/// A phase of a hop, in the order it happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Dns,
    Connect,
    FirstByte,
    Download,
}

impl Phase {
    /// The phase's part of an element id.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::Connect => "connect",
            Self::FirstByte => "first-byte",
            Self::Download => "download",
        }
    }

    /// The row's name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Dns => "DNS lookup",
            Self::Connect => "Connect",
            Self::FirstByte => "First byte",
            Self::Download => "Download",
        }
    }

    /// The muted words after the name.
    pub(crate) fn qualifier(self) -> Option<&'static str> {
        match self {
            Self::FirstByte => Some("send + wait"),
            _ => None,
        }
    }
}

/// One phase row: its bar on the send's time axis, when it was measured, and the value at
/// the right.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) phase: Phase,
    /// Where the phase starts on the axis and how long it took.
    pub(crate) bar: Option<(Duration, Duration)>,
    pub(crate) value: String,
    /// Whether the value is a measure, not a dash or a reason.
    pub(crate) measured: bool,
}

/// The request a hop sent, as the engine recorded it (masked): the line `METHOD url` and the
/// headers in wire order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SentBlock {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
}

impl SentBlock {
    /// The block of `wire`, `None` for a hop that recorded nothing (a history entry from before
    /// the wire was kept).
    fn of(wire: &WireRequest) -> Option<Self> {
        (!wire.url.is_empty() || !wire.headers.is_empty()).then(|| Self {
            method: wire.method.clone(),
            url: wire.url.clone(),
            headers: wire.headers.clone(),
        })
    }

    /// The request line: `<method> <url>`.
    pub(crate) fn line(&self) -> String {
        format!("{} {}", self.method, self.url)
    }
}

/// One hop: what it was and its four rows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hop {
    /// `redirect` or `final response`, with `reused connection` when it had no connect.
    pub(crate) kind: String,
    pub(crate) total: Duration,
    pub(crate) rows: Vec<Row>,
    /// The request the hop sent, `None` when none was recorded.
    pub(crate) request: Option<SentBlock>,
}

/// The timeline of a send.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Timeline {
    /// `1 hop` or `<n> hops`.
    pub(crate) hops_label: String,
    /// `new connection` or `reused connection` for one hop, `<n> redirect(s) followed`
    /// for more.
    pub(crate) detail: String,
    pub(crate) hops: Vec<Hop>,
    /// Every phase of every hop added up.
    pub(crate) total: Duration,
}

impl Timeline {
    /// The timeline of `timings`, `None` when nothing was measured.
    pub(crate) fn new(timings: &Timings) -> Option<Self> {
        if timings.hops.is_empty() {
            return None;
        }
        let total: Duration = timings.hops.iter().map(hop_total).sum();
        let count = timings.hops.len();
        let mut at = Duration::ZERO;
        let hops = timings
            .hops
            .iter()
            .enumerate()
            .map(|(index, hop)| {
                let last = index + 1 == count;
                let kind = match (last, hop.connect.is_none()) {
                    (true, false) => "final response".to_owned(),
                    (true, true) => "final response · reused connection".to_owned(),
                    (false, false) => "redirect".to_owned(),
                    (false, true) => "redirect · reused connection".to_owned(),
                };
                let rows = rows(hop, last, &mut at);
                Hop {
                    kind,
                    total: hop_total(hop),
                    rows,
                    request: SentBlock::of(&hop.wire),
                }
            })
            .collect();
        let detail = match (count, timings.hops.first()) {
            (1, Some(hop)) if hop.connect.is_none() => "reused connection".to_owned(),
            (1, _) => "new connection".to_owned(),
            (2, _) => "1 redirect followed".to_owned(),
            (count, _) => format!("{} redirects followed", count - 1),
        };
        Some(Self {
            hops_label: match count {
                1 => "1 hop".to_owned(),
                count => format!("{count} hops"),
            },
            detail,
            hops,
            total,
        })
    }
}

/// The time `hop` took: its phases added up.
fn hop_total(hop: &HopTimings) -> Duration {
    hop.dns.unwrap_or_default() + hop.connect.unwrap_or_default() + hop.first_byte + hop.download
}

/// The four rows of `hop`, the last hop's when `last`, starting at `at` on the axis; `at`
/// moves past the hop.
fn rows(hop: &HopTimings, last: bool, at: &mut Duration) -> Vec<Row> {
    let dns_missing = match hop.connect {
        Some(_) => "— IP address",
        None => "—",
    };
    let phases = [
        (Phase::Dns, hop.dns, dns_missing),
        (Phase::Connect, hop.connect, "—"),
        (Phase::FirstByte, Some(hop.first_byte), "—"),
        (Phase::Download, last.then_some(hop.download), "not read"),
    ];
    phases
        .into_iter()
        .map(|(phase, measured, missing)| match measured {
            Some(time) => {
                let bar = (*at, time);
                *at += time;
                Row {
                    phase,
                    bar: Some(bar),
                    value: elapsed_text(time),
                    measured: true,
                }
            }
            None => Row {
                phase,
                bar: None,
                value: missing.to_owned(),
                measured: false,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn values(hop: &Hop) -> Vec<&str> {
        hop.rows.iter().map(|row| row.value.as_str()).collect()
    }

    #[test]
    fn no_hops_is_no_timeline() {
        assert_eq!(Timeline::new(&Timings::default()), None);
    }

    #[test]
    fn one_hop_on_a_new_connection_lays_its_phases_end_to_end() {
        let timings = Timings {
            hops: vec![HopTimings {
                dns: Some(ms(4)),
                connect: Some(ms(43)),
                tls: None,
                first_byte: ms(129),
                download: ms(8),
                ..Default::default()
            }],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        assert_eq!(timeline.hops_label, "1 hop");
        assert_eq!(timeline.detail, "new connection");
        assert_eq!(timeline.total, ms(184));
        let hop = &timeline.hops[0];
        assert_eq!(hop.kind, "final response");
        assert_eq!(values(hop), ["4 ms", "43 ms", "129 ms", "8 ms"]);
        let starts: Vec<Duration> = hop
            .rows
            .iter()
            .filter_map(|row| row.bar)
            .map(|b| b.0)
            .collect();
        assert_eq!(starts.len(), 4);
        assert!(
            starts.windows(2).all(|pair| pair[0] < pair[1]),
            "{starts:?}"
        );
        let Some((start, width)) = hop.rows[3].bar else {
            panic!("download bar");
        };
        assert_eq!(start + width, timeline.total, "ends at the total");
    }

    #[test]
    fn a_redirect_then_a_reused_connection() {
        let timings = Timings {
            hops: vec![
                HopTimings {
                    dns: Some(ms(3)),
                    connect: Some(ms(41)),
                    tls: None,
                    first_byte: ms(22),
                    download: Duration::ZERO,
                    ..Default::default()
                },
                HopTimings {
                    dns: None,
                    connect: None,
                    tls: None,
                    first_byte: ms(110),
                    download: ms(8),
                    ..Default::default()
                },
            ],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        assert_eq!(timeline.hops_label, "2 hops");
        assert_eq!(timeline.detail, "1 redirect followed");
        assert_eq!(timeline.total, ms(184));
        let [first, second] = timeline.hops.as_slice() else {
            panic!("two hops");
        };
        assert_eq!(first.kind, "redirect");
        assert_eq!(first.total, ms(66));
        assert_eq!(values(first), ["3 ms", "41 ms", "22 ms", "not read"]);
        assert_eq!(first.rows[3].bar, None);
        assert_eq!(second.kind, "final response · reused connection");
        assert_eq!(second.total, ms(118));
        assert_eq!(values(second), ["—", "—", "110 ms", "8 ms"]);
        assert!(!second.rows[0].measured && !second.rows[1].measured);
        let Some((start, _)) = second.rows[2].bar else {
            panic!("first byte bar");
        };
        assert_eq!(start, ms(66), "after the first hop");
    }

    #[test]
    fn an_ip_address_has_no_lookup_and_says_why() {
        let timings = Timings {
            hops: vec![HopTimings {
                dns: None,
                connect: Some(ms(1)),
                tls: None,
                first_byte: ms(9),
                download: ms(2),
                ..Default::default()
            }],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        assert_eq!(timeline.detail, "new connection");
        assert_eq!(
            values(&timeline.hops[0]),
            ["— IP address", "1 ms", "9 ms", "2 ms"]
        );
        assert_eq!(
            timeline.hops[0].rows[1].bar.map(|bar| bar.0),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn one_hop_on_a_reused_connection_says_so_and_zero_draws_nothing_wide() {
        let timings = Timings {
            hops: vec![HopTimings::default()],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        assert_eq!(timeline.detail, "reused connection");
        assert_eq!(
            timeline.hops[0].rows[2].bar,
            Some((Duration::ZERO, Duration::ZERO))
        );
    }

    fn wire(headers: &[(&str, &str)]) -> WireRequest {
        WireRequest {
            method: "POST".to_owned(),
            url: "https://api.test/items".to_owned(),
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn a_hop_shows_its_request_line_and_headers_in_wire_order() {
        let timings = Timings {
            hops: vec![HopTimings {
                wire: wire(&[
                    ("user-agent", "reqflux"),
                    ("accept", "*/*"),
                    ("host", "api.test"),
                ]),
                ..Default::default()
            }],
        };
        let Some(block) = Timeline::new(&timings).and_then(|t| t.hops[0].request.clone()) else {
            panic!("a request block");
        };
        assert_eq!(block.line(), "POST https://api.test/items");
        assert_eq!(
            block.headers,
            [
                ("user-agent".to_owned(), "reqflux".to_owned()),
                ("accept".to_owned(), "*/*".to_owned()),
                ("host".to_owned(), "api.test".to_owned()),
            ]
        );
    }

    #[test]
    fn every_hop_keeps_its_own_request() {
        let mut first = wire(&[("cookie", "a=b")]);
        first.url = "https://api.test/old".to_owned();
        let timings = Timings {
            hops: vec![
                HopTimings {
                    wire: first,
                    ..Default::default()
                },
                HopTimings {
                    wire: wire(&[("host", "api.test")]),
                    ..Default::default()
                },
            ],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        let urls: Vec<_> = timeline
            .hops
            .iter()
            .filter_map(|hop| hop.request.as_ref().map(|block| block.url.as_str()))
            .collect();
        assert_eq!(urls, ["https://api.test/old", "https://api.test/items"]);
    }

    #[test]
    fn an_empty_wire_has_no_request_block() {
        let timings = Timings {
            hops: vec![HopTimings::default()],
        };
        let Some(timeline) = Timeline::new(&timings) else {
            panic!("a timeline");
        };
        assert_eq!(timeline.hops[0].request, None);
    }

    #[test]
    fn three_hops_count_two_redirects() {
        let hop = HopTimings::default();
        let timings = Timings {
            hops: vec![hop.clone(), hop.clone(), hop],
        };
        let detail = Timeline::new(&timings).map(|timeline| timeline.detail);
        assert_eq!(detail.as_deref(), Some("2 redirects followed"));
    }
}
