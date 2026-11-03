//! The Timeline view of a response (response-views/02), element id `response-timeline`: the
//! summary bar (`response-timeline-summary`, `<n> hop(s) · <detail>`, and the note
//! `response-timeline-note`), one group header `response-timeline-hop-<h>` per hop when the
//! send followed redirects, the phase rows `response-timeline-<h>-<phase>` with their bars
//! `response-timeline-<h>-<phase>-bar` on one time axis (`bolt`'s `TimelineStrip`,
//! `response-timeline-<h>-<phase>-strip`), and the total
//! `response-timeline-total`. A send without timings says so in `response-timeline-none`.

use std::time::Duration;

use bolt::primitives::{BadgeTone, TimelineBar, TimelineStrip};
use bolt::tokens::{Borders, GridMetrics, Heights, ResultMetrics, Spacing};
use gpui::{AnyElement, App, Div, FontWeight, ParentElement, Stateful, Styled, div, prelude::*};

use super::timeline_model::{Hop, NO_TIMINGS, Phase, Row, TLS_NOTE, Timeline};
use crate::shell::request::{Received, elapsed_text, size_text};
use crate::shell::rows::labeled_row;
use crate::shell::{region, text_region};
use crate::tokens;

/// The Timeline of `received`.
pub(super) fn render(received: &Received, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let pane = region("response-timeline")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ResultMetrics::FOOTER_FONT);
    let Some(timeline) = Timeline::new(&received.timings) else {
        return pane
            .child(
                text_region("response-timeline-none")
                    .aria_label(NO_TIMINGS)
                    .p(Spacing::MD)
                    .text_color(colors.muted)
                    .child(NO_TIMINGS),
            )
            .into_any_element();
    };
    let grouped = timeline.hops.len() > 1;
    let hops = timeline
        .hops
        .iter()
        .enumerate()
        .flat_map(move |(index, hop)| {
            let h = index + 1;
            let header = grouped.then(|| hop_header(h, hop, cx));
            header
                .into_iter()
                .chain(
                    hop.rows
                        .iter()
                        .map(move |row| phase_row(h, row, timeline.total, cx)),
                )
                .collect::<Vec<_>>()
        });
    pane.child(summary(&timeline, cx))
        .children(hops)
        .child(total(&timeline, received, cx))
        .into_any_element()
}

/// The bar over the rows: the hop count and detail, then the TLS note at the end.
fn summary(timeline: &Timeline, cx: &App) -> Div {
    let colors = bolt::colors(cx);
    let text = format!("{} · {}", timeline.hops_label, timeline.detail);
    div()
        .flex()
        .items_center()
        .gap(tokens::RESULTS_BAR_GAP)
        .h(tokens::RESULTS_BAR_HEIGHT)
        .pl(tokens::RESULTS_BAR_PADDING_X)
        .pr(tokens::RESULTS_BAR_PADDING_X)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .text_color(colors.muted)
        .child(
            text_region("response-timeline-summary")
                .aria_label(text.clone())
                .whitespace_nowrap()
                .child(text),
        )
        .child(div().flex_1())
        .child(
            text_region("response-timeline-note")
                .aria_label(TLS_NOTE)
                .whitespace_nowrap()
                .child(TLS_NOTE),
        )
}

/// The header of hop `h`: `Hop <h>`, what it was and its time.
fn hop_header(h: usize, hop: &Hop, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let time = elapsed_text(hop.total);
    labeled_row(
        format!("response-timeline-hop-{h}"),
        format!("Hop {h}: {}, {time}", hop.kind),
    )
    .flex()
    .items_center()
    .gap(Spacing::SM)
    .h(Heights::ROW)
    .pl(tokens::RESULTS_BAR_PADDING_X)
    .pr(tokens::RESULTS_BAR_PADDING_X)
    .border_b(Borders::THIN)
    .border_color(colors.row_divider)
    .bg(colors.background)
    .text_color(colors.muted)
    .child(
        div()
            .font_weight(FontWeight::BOLD)
            .text_color(colors.strong)
            .child(format!("Hop {h}")),
    )
    .child(div().whitespace_nowrap().child(hop.kind.clone()))
    .child(div().flex_1())
    .child(
        div()
            .font_family(bolt::fonts::editor_family(cx))
            .child(time),
    )
    .into_any_element()
}

/// The row of `row` in hop `h`: the phase name, its bar on the axis of `span` and the value.
fn phase_row(h: usize, row: &Row, span: Duration, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("response-timeline-{h}-{}", row.phase.key());
    let label = format!("{}: {}", row.phase.name(), row.value);
    let strip =
        TimelineStrip::new(format!("{id}-strip"), span).bars(row.bar.map(|(start, duration)| {
            TimelineBar::new(format!("{id}-bar"), label.clone(), start, duration)
                .tone(phase_tone(row.phase))
        }));
    labeled_row(id, label)
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .h(Heights::ROW)
        .pl(tokens::RESULTS_BAR_PADDING_X)
        .pr(tokens::RESULTS_BAR_PADDING_X)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .w(tokens::GRID_KEY_COLUMN)
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::XS)
                .whitespace_nowrap()
                .text_color(colors.foreground)
                .child(row.phase.name())
                .children(
                    row.phase
                        .qualifier()
                        .map(|words| div().text_color(colors.muted).child(words)),
                ),
        )
        .child(strip)
        .child(
            div()
                .w(GridMetrics::ROW_NUMBER_WIDTH)
                .flex_none()
                .flex()
                .justify_end()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_color(if row.measured {
                    colors.strong
                } else {
                    colors.muted
                })
                .child(row.value.clone()),
        )
        .into_any_element()
}

/// The last row: `Total`, the response size and the sum of every phase.
fn total(timeline: &Timeline, received: &Received, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let time = elapsed_text(timeline.total);
    let size = format!("response {}", size_text(received.size()));
    text_region("response-timeline-total")
        .aria_label(format!("Total: {time}"))
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .h(Heights::ROW)
        .pl(tokens::RESULTS_BAR_PADDING_X)
        .pr(tokens::RESULTS_BAR_PADDING_X)
        .text_color(colors.muted)
        .child(
            div()
                .w(tokens::GRID_KEY_COLUMN)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.strong)
                .child("Total"),
        )
        .child(div().flex_1())
        .child(div().whitespace_nowrap().child(size))
        .child(
            div()
                .w(GridMetrics::ROW_NUMBER_WIDTH)
                .flex_none()
                .flex()
                .justify_end()
                .font_family(bolt::fonts::editor_family(cx))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(time),
        )
}

/// The tone of `phase`'s bar: DNS violet, connect info, first byte the accent, download
/// success.
fn phase_tone(phase: Phase) -> BadgeTone {
    match phase {
        Phase::Dns => BadgeTone::Violet,
        Phase::Connect => BadgeTone::Info,
        Phase::FirstByte => BadgeTone::Accent,
        Phase::Download => BadgeTone::Success,
    }
}
