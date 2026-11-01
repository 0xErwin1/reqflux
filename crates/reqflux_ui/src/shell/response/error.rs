//! The error banner of a failed send, `bolt`'s danger banner with the Retry button that sends
//! again, and the test and console rows of the pre-request script that ran before the
//! failure.

use bolt::controls::Button;
use bolt::primitives::{BannerBlock, BannerVariant};
use bolt::tokens::Spacing;
use bolt_legacy::IconName;
use gpui::{AnyElement, App, Context, Div, ParentElement, Role, Styled, div, prelude::*};

use super::results::{console_row, list, test_row};
use crate::shell::request::{Failure, ui};
use crate::shell::{Shell, region};

/// The banner of `failure`, element id `response-error`, with `response-retry`, which sends
/// the request `id` again.
pub(super) fn render(id: &str, failure: &Failure, cx: &mut Context<Shell>) -> Div {
    let banner = region("response-error")
        .role(Role::Alert)
        .aria_label(format!("Request failed. {}", failure.error))
        .child(
            BannerBlock::new(BannerVariant::Danger, "Request failed.")
                .default_icon(ui::icon(IconName::TriangleAlert))
                .with_body(failure.error.clone())
                .with_actions(retry(id, cx)),
        );
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(Spacing::SM)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pb(Spacing::MD)
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
    let tests = scripts
        .tests
        .iter()
        .enumerate()
        .map(|(index, test)| test_row("response-error-test", index + 1, test, cx));
    let console = scripts
        .console
        .iter()
        .enumerate()
        .map(|(index, entry)| console_row("response-error-console", index + 1, entry, cx));
    let rows = tests.chain(console);
    Some(
        list(region("response-error-results"), cx)
            .children(rows)
            .into_any_element(),
    )
}

/// The primary Retry button, element id `response-retry`.
fn retry(id: &str, cx: &mut Context<Shell>) -> Div {
    let id = id.to_owned();
    ui::tagged("response-retry").child(
        Button::new("response-retry", "Retry")
            .primary()
            .icon(ui::icon(IconName::RotateCcw))
            .kbd("Ctrl ↵")
            .on_click(cx.listener(move |this, _event, window, cx| this.send(&id, window, cx))),
    )
}
