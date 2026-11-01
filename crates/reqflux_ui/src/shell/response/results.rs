//! The Tests and Console panes of a response (S12): one row per recorded test and one per
//! console entry, with the failure and severity colors. The error banner of a failed send
//! lists the pre-request script's rows the same way.

use bolt::primitives::Icon;
use bolt::tokens::{Borders, Fields, GridMetrics, Heights, Spacing};
use bolt_legacy::IconName;
use gpui::{AnyElement, App, Div, ParentElement, Stateful, Styled, div, prelude::*};
use reqflux_engine::{ConsoleEntry, ConsoleLevel, TestResult};

use crate::shell::region;
use crate::shell::request::{Received, ui};
use crate::shell::rows::{labeled_row, part};

/// `element` as a scrolling list of result rows, in the data face.
pub(super) fn list(element: Stateful<Div>, cx: &App) -> Stateful<Div> {
    element
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
}

/// A pane `id` of result rows under a line.
fn pane(id: &'static str, cx: &App) -> Stateful<Div> {
    list(region(id), cx)
        .border_t(Borders::THIN)
        .border_color(bolt::colors(cx).line)
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

/// The pane of the test results of `received`.
pub(super) fn tests(received: &Received, cx: &App) -> AnyElement {
    pane("response-tests", cx)
        .children(
            received
                .scripts
                .tests
                .iter()
                .enumerate()
                .map(|(index, test)| test_row("response-test", index + 1, test, cx)),
        )
        .into_any_element()
}

/// The pane of the console entries of `received`.
pub(super) fn console(received: &Received, cx: &App) -> AnyElement {
    pane("response-console", cx)
        .children(
            received
                .scripts
                .console
                .iter()
                .enumerate()
                .map(|(index, entry)| console_row("response-console", index + 1, entry, cx)),
        )
        .into_any_element()
}
