//! The Tests and Console panes of a response (S12): one row per recorded test and one per
//! console entry, with the failure and severity colors.

use bolt_legacy::tokens::{border, family, icon, space};
use bolt_legacy::{Icon, IconName};
use gpui::{AnyElement, App, ParentElement, Styled, div, prelude::*};

use reqflux_engine::ConsoleLevel;

use crate::shell::region;
use crate::shell::request::Received;
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// The pane of the test results of `received`: a check and the name of a passing test, a
/// failed one with its message in danger.
pub(super) fn tests(received: &Received, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    let tint = bolt_legacy::brand(cx).accent_tint;
    region("response-tests")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(border::THIN)
        .border_color(colors.line)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .children(
            received
                .scripts
                .tests
                .iter()
                .enumerate()
                .map(|(index, test)| {
                    let n = index + 1;
                    let row = labeled_row(
                        format!("response-test-{n}"),
                        match (&test.message, test.passed) {
                            (Some(message), false) => format!("{}: {message}", test.name),
                            _ => test.name.clone(),
                        },
                    )
                    .flex()
                    .items_center()
                    .gap(space::SM)
                    .h(tokens::GRID_ROW)
                    .pl(space::MD)
                    .pr(space::MD)
                    .border_b(border::THIN)
                    .border_color(colors.row_divider)
                    .when(test.passed, |row| {
                        row.child(Icon::new(IconName::CircleCheck).size(icon::SM).color(tint))
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
                        true => row,
                        false => row.child(
                            part(format!("response-test-{n}-message"))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(colors.danger)
                                .child(test.message.clone().unwrap_or_else(|| "failed".to_owned())),
                        ),
                    }
                }),
        )
        .into_any_element()
}

/// The pane of the console entries of `received`: the level label then the message, a
/// warning in the warning color and an error in danger.
pub(super) fn console(received: &Received, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    region("response-console")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(border::THIN)
        .border_color(colors.line)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .children(
            received
                .scripts
                .console
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let color = match entry.level {
                        ConsoleLevel::Log | ConsoleLevel::Info => colors.foreground,
                        ConsoleLevel::Warn => colors.warning,
                        ConsoleLevel::Error => colors.danger,
                    };
                    labeled_row(
                        format!("response-console-{}", index + 1),
                        format!("{}: {}", entry.level.as_str(), entry.message),
                    )
                    .flex()
                    .items_center()
                    .h(tokens::GRID_ROW)
                    .border_b(border::THIN)
                    .border_color(colors.row_divider)
                    .child(
                        div()
                            .w(tokens::GRID_NUMBER_COLUMN)
                            .flex_none()
                            .pl(space::MD)
                            .pr(space::MD)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(
                                if matches!(entry.level, ConsoleLevel::Log | ConsoleLevel::Info) {
                                    colors.muted
                                } else {
                                    color
                                },
                            )
                            .child(entry.level.as_str()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr(space::MD)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(color)
                            .child(entry.message.clone()),
                    )
                }),
        )
        .into_any_element()
}
