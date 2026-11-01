//! The Tests and Console panes of a response (S12): a filter bar, then one row per recorded
//! test and one per console entry the filter keeps, with the failure and severity colors.
//! The error banner of a failed send lists the pre-request script's rows the same way.

use bolt::primitives::{Badge, BadgeTone, Icon, SegmentedItem, TextVariant};
use bolt::tokens::{Borders, Fields, GridMetrics, Heights, ResultMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, ParentElement, SharedString, Stateful, Styled, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::{ConsoleEntry, ConsoleLevel, TestResult};

use super::filters::{
    ConsoleFilter, TestsFilter, console_count, console_empty_text, shown_console, shown_tests,
    tests_empty_text,
};
use crate::shell::request::segmented::control;
use crate::shell::request::{Received, ui};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{Shell, region, text_region};
use crate::tokens;

/// The note at the end of the Console bar.
pub(crate) const REDACTED_NOTE: &str = "secrets are redacted before they are printed";

/// `element` as a scrolling list of result rows, in the data face.
pub(super) fn list(element: Stateful<Div>, cx: &App) -> Stateful<Div> {
    element
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
}

/// The row `<prefix>-<n>` of `test`: a check and the name of a passing test, a failed one
/// with its message, `<prefix>-<n>-message`, in danger.
pub(super) fn test_row(prefix: &str, n: usize, test: &TestResult, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let row = labeled_row(
        format!("{prefix}-{n}"),
        match (&test.message, test.passed) {
            (Some(message), false) => format!("{}: {message}", test.name),
            _ => test.name.clone(),
        },
    )
    .flex()
    .items_center()
    .gap(Spacing::SM)
    .h(Heights::ROW)
    .pl(Spacing::MD)
    .pr(Spacing::MD)
    .border_b(Borders::THIN)
    .border_color(colors.row_divider)
    .when(test.passed, |row| {
        row.child(
            Icon::new(ui::icon(IconName::CircleCheck))
                .size(Fields::CHEVRON)
                .color(tint),
        )
    })
    .child(
        div()
            .flex_none()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(if test.passed {
                colors.foreground
            } else {
                colors.strong
            })
            .child(test.name.clone()),
    );
    match test.passed {
        true => row.into_any_element(),
        false => row
            .child(
                part(format!("{prefix}-{n}-message"))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(colors.danger)
                    .child(test.message.clone().unwrap_or_else(|| "failed".to_owned())),
            )
            .into_any_element(),
    }
}

/// The row `<prefix>-<n>` of `entry`: the level label then the message, a warning in the
/// warning color and an error in danger.
pub(super) fn console_row(prefix: &str, n: usize, entry: &ConsoleEntry, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let color = match entry.level {
        ConsoleLevel::Log | ConsoleLevel::Info => colors.foreground,
        ConsoleLevel::Warn => colors.warning,
        ConsoleLevel::Error => colors.danger,
    };
    let level_color = match entry.level {
        ConsoleLevel::Log | ConsoleLevel::Info => colors.muted,
        ConsoleLevel::Warn | ConsoleLevel::Error => color,
    };
    labeled_row(
        format!("{prefix}-{n}"),
        format!("{}: {}", entry.level.as_str(), entry.message),
    )
    .flex()
    .items_center()
    .h(Heights::ROW)
    .border_b(Borders::THIN)
    .border_color(colors.row_divider)
    .child(
        div()
            .w(GridMetrics::ROW_NUMBER_WIDTH)
            .flex_none()
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(level_color)
            .child(entry.level.as_str()),
    )
    .child(
        div()
            .flex_1()
            .min_w_0()
            .pr(Spacing::MD)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(color)
            .child(entry.message.clone()),
    )
    .into_any_element()
}

/// The filter bar `<id>` over a results pane (response-views/02): 36 px, muted 12 px text.
fn bar(id: &'static str, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    region(id)
        .flex_none()
        .h(tokens::RESULTS_BAR_HEIGHT)
        .flex()
        .items_center()
        .gap(tokens::RESULTS_BAR_GAP)
        .pl(tokens::RESULTS_BAR_PADDING_X)
        .pr(tokens::RESULTS_BAR_PADDING_X)
        .border_t(Borders::THIN)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ResultMetrics::FOOTER_FONT)
        .text_color(colors.muted)
}

/// The caption `<id>` that takes the rows' place when the filter keeps none.
fn none(id: &'static str, text: &'static str, cx: &App) -> Stateful<Div> {
    ui::role(text_region(id), TextVariant::Caption, cx)
        .aria_label(text)
        .p(Spacing::MD)
        .child(text)
}

/// The Tests pane of `received` in the response of the request `id`: the bar
/// `response-tests-bar` with the `<n> passed` and `<n> failed` badges
/// (`response-tests-passed`, `response-tests-failed`) and the All / Failed filter
/// `response-tests-filter`, then the rows `response-test-<n>` it keeps, numbered as
/// received, or `response-tests-none`.
pub(super) fn tests(
    id: &str,
    received: &Received,
    filter: TestsFilter,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let all = &received.scripts.tests;
    let failed = all.iter().filter(|test| !test.passed).count();
    let passed = all.len() - failed;
    let filters = [
        (TestsFilter::All, "all", "All"),
        (TestsFilter::Failed, "failed", "Failed"),
    ];
    let active = filters
        .iter()
        .find(|(kind, ..)| *kind == filter)
        .map_or("all", |(_, key, _)| *key);
    let items = filters
        .iter()
        .map(|(_, key, label)| {
            SegmentedItem::new(*key, *label).element_id(format!("response-tests-filter-{key}"))
        })
        .collect();
    let request = id.to_owned();
    let on_select = cx.listener(move |this, picked: &SharedString, _window, cx| {
        if let Some((kind, ..)) = filters.iter().find(|(_, key, _)| *key == picked.as_ref()) {
            this.filter_response_tests(&request, *kind, cx);
        }
    });
    let badge = |id: &'static str, count: usize, word: &str, tone: BadgeTone| {
        (count > 0).then(|| Badge::new(format!("{count} {word}"), tone).id(id))
    };
    let shown = shown_tests(all, filter);
    let rows: Vec<AnyElement> = shown
        .iter()
        .map(|(n, test)| test_row("response-test", *n, test, cx))
        .collect();
    let empty = rows
        .is_empty()
        .then(|| tests_empty_text(filter))
        .flatten()
        .map(|text| none("response-tests-none", text, cx));
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            bar("response-tests-bar", cx)
                .children(badge(
                    "response-tests-passed",
                    passed,
                    "passed",
                    BadgeTone::Success,
                ))
                .children(badge(
                    "response-tests-failed",
                    failed,
                    "failed",
                    BadgeTone::Danger,
                ))
                .child(div().flex_1())
                .child(control(
                    "response-tests-filter",
                    None,
                    false,
                    items,
                    active,
                    on_select,
                )),
        )
        .child(
            list(region("response-tests"), cx)
                .children(empty)
                .children(rows),
        )
        .into_any_element()
}

/// The Console pane of `received` in the response of the request `id`: the bar
/// `response-console-bar` with the All / Warnings / Errors filter `response-console-filter`,
/// each with its count, and the redaction note `response-console-note`, then the rows
/// `response-console-<n>` it keeps, numbered as received, or `response-console-none`.
pub(super) fn console(
    id: &str,
    received: &Received,
    filter: ConsoleFilter,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let all = &received.scripts.console;
    let filters = [
        (ConsoleFilter::All, "all", "All"),
        (ConsoleFilter::Warnings, "warnings", "Warnings"),
        (ConsoleFilter::Errors, "errors", "Errors"),
    ];
    let active = filters
        .iter()
        .find(|(kind, ..)| *kind == filter)
        .map_or("all", |(_, key, _)| *key);
    let items = filters
        .iter()
        .map(|(kind, key, label)| {
            SegmentedItem::new(*key, *label)
                .element_id(format!("response-console-filter-{key}"))
                .count(console_count(all, *kind).to_string())
        })
        .collect();
    let request = id.to_owned();
    let on_select = cx.listener(move |this, picked: &SharedString, _window, cx| {
        if let Some((kind, ..)) = filters.iter().find(|(_, key, _)| *key == picked.as_ref()) {
            this.filter_response_console(&request, *kind, cx);
        }
    });
    let shown = shown_console(all, filter);
    let rows: Vec<AnyElement> = shown
        .iter()
        .map(|(n, entry)| console_row("response-console", *n, entry, cx))
        .collect();
    let empty = rows
        .is_empty()
        .then(|| console_empty_text(filter))
        .flatten()
        .map(|text| none("response-console-none", text, cx));
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            bar("response-console-bar", cx)
                .child(control(
                    "response-console-filter",
                    None,
                    false,
                    items,
                    active,
                    on_select,
                ))
                .child(div().flex_1())
                .child(
                    text_region("response-console-note")
                        .aria_label(REDACTED_NOTE)
                        .flex()
                        .items_center()
                        .gap(ResultMetrics::FOOTER_ITEM_GAP)
                        .whitespace_nowrap()
                        .child(
                            Icon::new(ui::icon(IconName::Shield))
                                .size(ResultMetrics::FOOTER_ICON)
                                .color(colors.muted),
                        )
                        .child(REDACTED_NOTE),
                ),
        )
        .child(
            list(region("response-console"), cx)
                .children(empty)
                .children(rows),
        )
        .into_any_element()
}
