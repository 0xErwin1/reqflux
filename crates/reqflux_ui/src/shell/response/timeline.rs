//! The Timeline view of a response (response-views/02), element id `response-timeline`: the
//! summary bar (`response-timeline-summary`, `<n> hop(s) · <detail>`, and the note
//! `response-timeline-note`), one group header `response-timeline-hop-<h>` per hop when the
//! send followed redirects, the phase rows `response-timeline-<h>-<phase>` with their bars
//! `response-timeline-<h>-<phase>-bar` on one time axis, and the total
//! `response-timeline-total`. A send without timings says so in `response-timeline-none`.

use bolt::semantic::ChartColors;
use bolt::tokens::{Borders, GridMetrics, Heights, ResultMetrics, Spacing};
use gpui::{
    AnyElement, App, Div, FontWeight, Hsla, ParentElement, Stateful, Styled, div, prelude::*,
    relative,
};

use super::timeline_model::{Hop, NO_TIMINGS, Phase, Row, TLS_NOTE, Timeline};
use crate::shell::request::{Received, elapsed_text, size_text};
use crate::shell::rows::{labeled_row, part};
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
    let hops = timeline.hops.iter().enumerate().flat_map(|(index, hop)| {
        let h = index + 1;
        let header = grouped.then(|| hop_header(h, hop, cx));
        header
            .into_iter()
            .chain(hop.rows.iter().map(move |row| phase_row(h, row, cx)))
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

/// The row of `row` in hop `h`: the phase name, its bar on the axis and the value.
fn phase_row(h: usize, row: &Row, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("response-timeline-{h}-{}", row.phase.key());
    let label = format!("{}: {}", row.phase.name(), row.value);
    let bar = row
        .bar
        .map(|bar| strip_bar(format!("{id}-bar"), bar, phase_color(row.phase, cx)));
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
        .child(strip(bar, cx))
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

/// The color of `phase`'s bar: DNS cyan, connect info, first byte the accent tint,
/// download success, as the artboard draws them.
fn phase_color(phase: Phase, cx: &App) -> Hsla {
    let colors = bolt::colors(cx);
    match phase {
        Phase::Dns => ChartColors::for_current(cx).stats_accent,
        Phase::Connect => colors.info,
        Phase::FirstByte => bolt::brand::brand(cx).accent_tint,
        Phase::Download => colors.success,
    }
}

// bolt gap: a timeline strip, phase bars over one time axis (B1). The two helpers below
// and their two sizes in `tokens` are the whole stand-in: the track and one bar on it.

/// The track of a phase row, filling the space between the name and the value, holding
/// `bar` when the phase was measured.
fn strip(bar: Option<Stateful<Div>>, cx: &App) -> Div {
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(tokens::TIMELINE_STRIP_HEIGHT)
        .bg(bolt::colors(cx).background)
        .children(bar)
}

/// The bar `id` from `start` over `width` of the track, both from 0 to 1, in `color`.
fn strip_bar(id: String, (start, width): (f32, f32), color: Hsla) -> Stateful<Div> {
    part(id)
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(start))
        .w(relative(width))
        .min_w(tokens::TIMELINE_STRIP_MIN_BAR)
        .bg(color)
}
