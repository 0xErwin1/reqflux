//! The error banner of a failed send, with the Retry button that sends again, and the test
//! and console rows of the pre-request script that ran before the failure.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::tokens::{border, cut, family, icon, space};
use bolt_legacy::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div,
    prelude::*,
};
use reqflux_engine::ConsoleLevel;

use crate::shell::request::{Failure, accent_keycap};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{Shell, region};
use crate::tokens;

/// The banner of `failure`, element id `response-error`, with `response-retry`, which sends
/// the request `id` again.
pub(super) fn render(id: &str, failure: &Failure, cx: &mut Context<Shell>) -> Div {
    let colors = bolt_legacy::colors(cx);
    let banner = region("response-error")
        .role(Role::Alert)
        .aria_label(format!("Request failed. {}", failure.error))
        .h(tokens::RESPONSE_ERROR_BANNER)
        .flex()
        .items_center()
        .gap(space::MD)
        .pl(space::MD)
        .pr(space::SM)
        .border_l(tokens::RESPONSE_ERROR_EDGE)
        .border_color(colors.danger)
        .bg(tokens::wash(colors.danger, tokens::RESPONSE_ERROR_WASH))
        .text_size(tokens::RESPONSE_ERROR_TEXT)
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(icon::MD)
                .color(colors.danger),
        )
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child("Request failed."),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(family::MONO)
                .text_size(tokens::GRID_TEXT)
                .text_color(colors.foreground)
                .child(failure.error.clone()),
        )
        .child(div().flex_1())
        .child(retry(id, cx));
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .pl(space::MD)
        .pr(space::MD)
        .pb(space::MD)
        .child(banner)
        .children(results(failure, cx))
}

/// The test and console rows of the failure's pre-request script, under the banner, in the
/// look of the response panes; `None` when the failure has nothing to show.
fn results(failure: &Failure, cx: &App) -> Option<AnyElement> {
    let scripts = &failure.scripts;
    if scripts.tests.is_empty() && scripts.console.is_empty() {
        return None;
    }
    let colors = bolt_legacy::colors(cx);
    let tint = bolt_legacy::brand(cx).accent_tint;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (index, test) in scripts.tests.iter().enumerate() {
        let n = index + 1;
        let row = labeled_row(
            format!("response-error-test-{n}"),
            match (&test.message, test.passed) {
                (Some(message), false) => format!("{}: {message}", test.name),
                _ => test.name.clone(),
            },
        )
        .flex()
        .items_center()
        .gap(space::SM)
        .h(tokens::GRID_ROW)
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
        rows.push(match test.passed {
            true => row.into_any_element(),
            false => row
                .child(
                    part(format!("response-error-test-{n}-message"))
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.danger)
                        .child(test.message.clone().unwrap_or_else(|| "failed".to_owned())),
                )
                .into_any_element(),
        });
    }
    for (index, entry) in scripts.console.iter().enumerate() {
        let color = match entry.level {
            ConsoleLevel::Log | ConsoleLevel::Info => colors.foreground,
            ConsoleLevel::Warn => colors.warning,
            ConsoleLevel::Error => colors.danger,
        };
        rows.push(
            labeled_row(
                format!("response-error-console-{}", index + 1),
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
            .into_any_element(),
        );
    }
    Some(
        region("response-error-results")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .font_family(family::MONO)
            .text_size(tokens::GRID_TEXT)
            .children(rows)
            .into_any_element(),
    )
}

/// The primary Retry button, element id `response-retry`.
fn retry(id: &str, cx: &mut Context<Shell>) -> Stateful<Div> {
    let brand = bolt_legacy::brand(cx);
    let id = id.to_owned();
    region("response-retry")
        .role(Role::Button)
        .aria_label("Retry")
        .relative()
        .flex_none()
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(brand.on_accent)
        .child(Chamfer::new(cut::CONTROL).fill(brand.accent))
        .child(
            Icon::new(IconName::RotateCcw)
                .size(icon::MD)
                .color(brand.on_accent),
        )
        .child("Retry")
        .child(accent_keycap())
        .on_click(cx.listener(move |this, _event, window, cx| this.send(&id, window, cx)))
}
