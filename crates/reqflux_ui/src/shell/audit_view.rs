//! The audit view (app-pending S2), `audit-view`, as the artboard
//! `design/bolt-v2/13-audit.html` draws it: opened from the rail's `rail-audit`, it takes the
//! document island while the sidebar keeps the collection tree. A header with the title, the
//! subtitle and Refresh; a toolbar with the search field, the time range, the Class and
//! Outcome selects and Clear; the timeline of the calls in the range; the table of entries,
//! newest first ([`super::audit_table`]); and a footer with the log's path and the count.
//!
//! Read-only: nothing here writes the log. A line that does not parse is reported in
//! `audit-skipped` by its number and never shown; a log that cannot be read shows
//! `audit-error` instead of the table. The log holds identifiers and masked URLs, never an
//! argument value or a secret, so nothing shown is one.
//!
//! It draws with DBFlux's `bolt`: the document `Island`, `DocumentMetrics` for the header,
//! toolbar, timeline and table, `Input`, `SegmentedControl`, `Dropdown`, `Button`,
//! `BannerBlock` and `EmptyState`.

use bolt::composites::{EmptyState, Island};
use bolt::controls::{Button, Input};
use bolt::primitives::{BannerBlock, BannerVariant, Icon, SegmentedItem, TextVariant};
use bolt::tokens::{Borders, DocumentMetrics, Feedback};
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Pixels, Styled, Window, div,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::audit::Controls;
use super::audit_model::{self, Bucket, RANGES, Range, visible};
use super::history::now_millis;
use super::request::{segmented, ui};
use super::{Shell, audit_table, region, text_region};

/// The view's title and subtitle.
pub(crate) const TITLE: &str = "Audit log";
const SUBTITLE: &str = "every write, execute or destructive call through reqflux mcp";
/// The title of the empty state.
pub(crate) const EMPTY: &str = "No agent call recorded yet";
const EMPTY_TEXT: &str = "Every write, execute or destructive call an agent makes through \
     reqflux mcp is recorded here, allowed or refused.";
/// The title shown when the log cannot be read.
pub(crate) const UNREADABLE: &str = "The audit log could not be read";
/// Shown when the filters keep no entry.
pub(crate) const NO_MATCH: &str = "No entries match the filters";
/// After the log's path in the footer.
const WHERE: &str = "· per-user state, never in the workspace";

/// The view in the document island.
pub(super) fn render(shell: &Shell, window: &mut Window, cx: &mut Context<Shell>) -> AnyElement {
    let now = now_millis();
    let audit = &shell.audit;
    let mut island = Island::new()
        .size_full()
        .child(super::tabs::render(shell, cx))
        .child(header(cx));
    if let Some(controls) = &audit.controls {
        island = island.child(toolbar(controls, audit.filters.range, shell, cx));
    }
    let body: AnyElement = match &audit.read {
        Some(Err(error)) => state(
            EmptyState::new(ui::icon(IconName::TriangleAlert), error.clone())
                .title(UNREADABLE)
                .danger(),
            "audit-error",
            UNREADABLE,
        ),
        None => state(
            EmptyState::new(ui::icon(IconName::FingerprintPattern), EMPTY_TEXT).title(EMPTY),
            "audit-empty",
            EMPTY,
        ),
        Some(Ok(log)) if log.entries.is_empty() && log.skipped.is_empty() => state(
            EmptyState::new(ui::icon(IconName::FingerprintPattern), EMPTY_TEXT).title(EMPTY),
            "audit-empty",
            EMPTY,
        ),
        Some(Ok(log)) => {
            let shown = visible(&log.entries, &audit.filters, now);
            let buckets = audit_model::timeline(&shown, audit.filters.range, now);
            let mut body = div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(timeline(&buckets, cx));
            if !log.skipped.is_empty() {
                body = body.child(skipped(&log.skipped));
            }
            body.child(audit_table::render(shell, &shown, window, cx))
                .into_any_element()
        }
    };
    let count = audit.log().map(|log| {
        let shown = visible(&log.entries, &audit.filters, now).len();
        count_text(shown, audit.filters.range)
    });
    region("audit-view")
        .flex_1()
        .min_w_0()
        .h_full()
        .child(
            island
                .child(body)
                .child(footer(audit.path.as_deref(), count, cx)),
        )
        .into_any_element()
}

/// `<n> events in <range>`, or `1 event`.
fn count_text(count: usize, range: Range) -> String {
    let noun = if count == 1 { "event" } else { "events" };
    format!("{count} {noun} in {}", range.label())
}

/// The header: the fingerprint icon, the title, the subtitle and Refresh.
fn header(cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    div()
        .flex_none()
        .h(DocumentMetrics::HEADER_HEIGHT)
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .border_t(Borders::THIN)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            Icon::new(ui::icon(IconName::FingerprintPattern))
                .size(DocumentMetrics::TITLE_ICON)
                .color(tint),
        )
        .child(
            ui::role(text_region("audit-title"), TextVariant::Body, cx)
                .aria_label(TITLE)
                .text_size(DocumentMetrics::TITLE_FONT)
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(TITLE),
        )
        .child(
            ui::role(text_region("audit-subtitle"), TextVariant::Body, cx)
                .aria_label(SUBTITLE)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(SUBTITLE),
        )
        .child(div().flex_1())
        .child(
            Button::new("audit-refresh", "Refresh")
                .primary()
                .icon(ui::icon(IconName::RefreshCw))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.refresh_audit(cx);
                })),
        )
}

/// The toolbar: search, time range, a rule, the Class and Outcome selects and Clear.
fn toolbar(
    controls: &Controls,
    range: Range,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let entity = cx.entity().downgrade();
    let items = RANGES
        .iter()
        .map(|range| {
            SegmentedItem::new(range.label(), range.label())
                .element_id(format!("audit-range-{}", range.label()))
        })
        .collect();
    let ranges = segmented::control(
        "audit-range",
        Some("Time range".into()),
        false,
        items,
        range.label(),
        move |id, _window, cx| {
            let Some(range) = Range::of(id) else {
                return;
            };
            if let Err(error) = entity.update(cx, |this, cx| this.select_audit_range(range, cx)) {
                log::warn!("the audit toolbar outlived the shell: {error}");
            }
        },
    );
    div()
        .flex_none()
        .h(DocumentMetrics::TOOLBAR_HEIGHT)
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div().flex_none().w(DocumentMetrics::SEARCH_WIDTH).child(
                Input::new(&controls.search)
                    .id("audit-search")
                    .aria_label(super::audit::SEARCH_PLACEHOLDER)
                    .prefix(
                        Icon::new(ui::icon(IconName::Search))
                            .size(DocumentMetrics::SEARCH_ICON)
                            .color(colors.muted),
                    ),
            ),
        )
        .child(div().flex_none().child(ranges))
        .child(
            div()
                .flex_none()
                .w(Borders::THIN)
                .h(DocumentMetrics::TOOLBAR_RULE_HEIGHT)
                .mx(DocumentMetrics::TOOLBAR_RULE_MARGIN_X)
                .bg(colors.line),
        )
        .child(div().flex_none().child(controls.class.clone()))
        .child(div().flex_none().child(controls.outcome.clone()))
        .child(div().flex_1())
        .when(shell.audit.filters.is_filtered(), |bar| {
            bar.child(
                Button::new("audit-clear", "Clear filters")
                    .ghost()
                    .icon(ui::icon(IconName::Close))
                    .icon_only()
                    .tooltip("Clear filters")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.clear_audit_filters(window, cx);
                    })),
            )
        })
}

/// The timeline strip `audit-timeline`: a bar per bucket, the refused calls stacked on the
/// others, and the legend.
// bolt gap: no timeline (histogram) strip; DBFlux's audit document draws its own from these
// `DocumentMetrics` tokens.
fn timeline(buckets: &[Bucket], cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let calls = bolt::brand::brand(cx).accent;
    let peak = buckets
        .iter()
        .map(|bucket| bucket.total)
        .max()
        .unwrap_or(0)
        .max(1);
    let height = |count: usize| {
        if count == 0 {
            Pixels::ZERO
        } else {
            #[allow(clippy::cast_precision_loss)]
            let share = count as f32 / peak as f32;
            // A bar with any call is at least a hairline high.
            (DocumentMetrics::TIMELINE_BAR_HEIGHT * share).max(Borders::THIN)
        }
    };
    let bars = buckets.iter().map(|bucket| {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .justify_end()
            .h(DocumentMetrics::TIMELINE_BAR_HEIGHT)
            .child(div().h(height(bucket.refused)).bg(colors.danger))
            .child(div().h(height(bucket.total - bucket.refused)).bg(calls))
    });
    let swatch = |color| {
        div()
            .flex_none()
            .size(DocumentMetrics::TIMELINE_SWATCH)
            .bg(color)
    };
    let legend = |swatch: gpui::Div, label: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(Feedback::STATUS_GAP)
            .child(swatch)
            .child(label)
    };
    region("audit-timeline")
        .flex_none()
        .flex()
        .items_end()
        .gap(DocumentMetrics::TIMELINE_LEGEND_GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .py(DocumentMetrics::TIMELINE_PADDING_Y)
        .bg(colors.background)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_1()
                .flex()
                .items_end()
                .gap(DocumentMetrics::TIMELINE_BAR_GAP)
                .children(bars),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(DocumentMetrics::TIMELINE_LEGEND_ROW_GAP)
                .text_size(DocumentMetrics::TIMELINE_LEGEND_FONT)
                .text_color(colors.muted)
                .child(legend(swatch(calls), "calls"))
                .child(legend(swatch(colors.danger), "refused")),
        )
}

/// The warning `audit-skipped` naming the lines that did not parse; their text never shows.
fn skipped(lines: &[usize]) -> impl IntoElement {
    let numbers = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let text = if lines.len() == 1 {
        format!("1 line of the audit log could not be read and is not shown: line {numbers}")
    } else {
        format!(
            "{} lines of the audit log could not be read and are not shown: lines {numbers}",
            lines.len()
        )
    };
    text_region("audit-skipped")
        .aria_label(text.clone())
        .flex_none()
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .py(DocumentMetrics::TOOLBAR_PADDING_Y)
        .child(
            BannerBlock::new(BannerVariant::Warning, text)
                .default_icon(ui::icon(IconName::TriangleAlert)),
        )
}

/// `empty`, centered in the body, found by tests as `id` and named by `title`.
fn state(empty: EmptyState, id: &'static str, title: &'static str) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .child(text_region(id).aria_label(title).flex_none().child(empty))
        .into_any_element()
}

/// The footer: the log's path and where it lives, and the count of the entries shown.
fn footer(
    path: Option<&std::path::Path>,
    count: Option<String>,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let path = path.map(|path| path.display().to_string());
    div()
        .flex_none()
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .text_size(DocumentMetrics::TABLE_META_FONT)
        .text_color(colors.muted)
        .whitespace_nowrap()
        .child(
            Icon::new(ui::icon(IconName::File))
                .size(DocumentMetrics::TABLE_ICON)
                .color(colors.muted),
        )
        .when_some(path, |footer, path| {
            footer.child(
                ui::role(text_region("audit-path"), TextVariant::Code, cx)
                    .aria_label(path.clone())
                    .text_size(DocumentMetrics::TABLE_META_FONT)
                    .text_color(colors.foreground)
                    .overflow_hidden()
                    .child(path),
            )
        })
        .child(WHERE)
        .child(div().flex_1())
        .when_some(count, |footer, count| {
            footer.child(
                text_region("audit-count")
                    .aria_label(count.clone())
                    .child(count),
            )
        })
}
