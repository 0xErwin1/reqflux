//! The result of the selected step, `runner-result`, under the grid: its request, status,
//! time and size with Open request and Close, then its tests, `runner-tests`, beside its
//! recorded response, `runner-response`, drawn by the response pane from the send history
//! stored for it. When the run sent the request more than once, the response is the last
//! send's and the pane says so.

use bolt::composites::header::section_header;
use bolt::controls::Button;
use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Borders, DocumentMetrics, HeaderMetrics, Spacing};
use gpui::{AnyElement, Context, Div, FontWeight, ParentElement, Role, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_engine::{StepResult, TestResult};

use super::RunTab;
use super::model::tests_text;
use super::view::tone_color;
use crate::shell::request::{elapsed_text, size_text, ui};
use crate::shell::rows::part;
use crate::shell::{Shell, region, response, text_region};
use crate::tokens::{self, RUNNER_TESTS_WIDTH as TESTS_WIDTH, method_hue};

/// The result of step `step`.
pub(super) fn render(
    shell: &Shell,
    run: &RunTab,
    step: usize,
    result: &StepResult,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let plan = run.model.plan.iter().find(|row| row.id == result.item.id);
    let method = result
        .method
        .clone()
        .or_else(|| plan.map(|plan| plan.method.clone()))
        .unwrap_or_default();
    let (status, status_tone) = match (result.status, &result.error) {
        (Some(code), _) => (
            match result.reason.as_deref() {
                Some(reason) if !reason.is_empty() => format!("{code} {reason}"),
                Some(_) | None => code.to_string(),
            },
            match code {
                200..=299 => super::model::Tone::Success,
                400..=499 => super::model::Tone::Warning,
                _ => super::model::Tone::Danger,
            },
        ),
        (None, _) => ("No response".to_owned(), super::model::Tone::Danger),
    };
    let meta = format!(
        "{} \u{b7} {} \u{b7} request {} of {}",
        elapsed_text(std::time::Duration::from_millis(result.elapsed_ms)),
        size_text(result.size),
        step + 1,
        run.model.plan.len(),
    );
    let open_id = result.item.id.clone();
    let title = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .h(DocumentMetrics::TOOLBAR_HEIGHT)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(&method, cx))
                .child(method),
        )
        .child(
            ui::role(text_region("runner-result-title"), TextVariant::Heading, cx)
                .aria_label(result.item.name.clone())
                .child(result.item.name.clone()),
        )
        .child(
            part("runner-result-status".to_owned())
                .role(Role::Label)
                .aria_label(status.clone())
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(DocumentMetrics::TABLE_META_FONT)
                .text_color(tone_color(status_tone, cx))
                .child(status),
        )
        .child(
            part("runner-result-meta".to_owned())
                .role(Role::Label)
                .aria_label(meta.clone())
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(DocumentMetrics::TABLE_META_FONT)
                .text_color(colors.muted)
                .child(meta),
        )
        .child(div().flex_1())
        .child(Button::new("runner-open-request", "Open request").on_click(
            cx.listener(move |this, _event, _window, cx| this.open_document(&open_id, cx)),
        ))
        .child(
            Button::new("runner-result-close", "Close result")
                .ghost()
                .icon(ui::icon(IconName::X))
                .icon_only()
                .on_click(cx.listener(|this, _event, _window, cx| this.close_run_result(cx))),
        );
    region("runner-result")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(title)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(tests(result, cx))
                .child(response_pane(shell, run, step, result, cx)),
        )
        .into_any_element()
}

/// The tests of the step, `runner-test-<n>` from 1, a failing one with its message; or its
/// error when it got no response.
fn tests(result: &StepResult, cx: &mut Context<Shell>) -> gpui::Stateful<Div> {
    let colors = bolt::colors(cx);
    let (counts, tone) = tests_text(result);
    let mut column = div()
        .id("runner-tests")
        .role(Role::List)
        .aria_label("Tests")
        .flex_none()
        .w(TESTS_WIDTH)
        .overflow_y_scroll()
        .pl(Spacing::LG)
        .pr(Spacing::LG)
        .border_r(Borders::THIN)
        .border_color(colors.line)
        .child(
            section_header("TESTS", Some(ui::icon(IconName::ListChecks)), cx).child(
                div()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_size(DocumentMetrics::TABLE_META_FONT)
                    .text_color(tone_color(tone, cx))
                    .child(counts),
            ),
        );
    if let Some(error) = &result.error {
        column = column.child(
            ui::role(text_region("runner-result-error"), TextVariant::Code, cx)
                .aria_label(error.clone())
                .py(Spacing::SM)
                .text_color(colors.danger)
                .child(error.clone()),
        );
    }
    if result.tests.is_empty() {
        let text = "No tests in this request's scripts.";
        column = column.child(
            ui::role(text_region("runner-no-tests"), TextVariant::Caption, cx)
                .aria_label(text)
                .py(Spacing::SM)
                .child(text),
        );
    }
    let rows: Vec<AnyElement> = result
        .tests
        .iter()
        .enumerate()
        .map(|(index, test)| test_row(index + 1, test, cx))
        .collect();
    column.children(rows)
}

fn test_row(n: usize, test: &TestResult, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let (icon, color) = if test.passed {
        (IconName::CircleCheck, colors.success)
    } else {
        (IconName::CircleX, colors.danger)
    };
    let label = match (&test.message, test.passed) {
        (_, true) => format!("{} passed", test.name),
        (Some(message), false) => format!("{} failed: {message}", test.name),
        (None, false) => format!("{} failed", test.name),
    };
    part(format!("runner-test-{n}"))
        .role(Role::ListItem)
        .aria_label(label)
        .flex()
        .gap(Spacing::SM)
        .py(Spacing::XS)
        .child(
            Icon::new(ui::icon(icon))
                .size(DocumentMetrics::TABLE_ICON)
                .color(color),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_color(colors.foreground).child(test.name.clone()))
                .children(
                    test.message
                        .clone()
                        .filter(|_| !test.passed)
                        .map(|message| {
                            div()
                                .font_family(bolt::fonts::editor_family(cx))
                                .text_size(DocumentMetrics::TABLE_META_FONT)
                                .text_color(colors.danger)
                                .child(message)
                        }),
                ),
        )
        .into_any_element()
}

/// The recorded response of the step, or why there is none.
fn response_pane(
    shell: &Shell,
    run: &RunTab,
    step: usize,
    result: &StepResult,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let pane = region("runner-response")
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col();
    let Some(key) = run.responses.get(&step) else {
        let text = if result.status.is_some() {
            "No recorded response: the run could not record this send in history."
        } else {
            "No response to show."
        };
        return pane
            .child(
                ui::role(
                    text_region("runner-response-note"),
                    TextVariant::Caption,
                    cx,
                )
                .aria_label(text)
                .p(Spacing::LG)
                .child(text),
            )
            .into_any_element();
    };
    let sends = run.model.sends_of(&result.item.id);
    let note = (sends > 1).then(|| {
        let text = format!(
            "The run sent this request {sends} times; this is the response of the last send."
        );
        ui::role(
            text_region("runner-response-note"),
            TextVariant::Caption,
            cx,
        )
        .aria_label(text.clone())
        .pl(Spacing::LG)
        .pr(Spacing::LG)
        .py(Spacing::SM)
        .child(text)
    });
    pane.children(note)
        .child(response::render(key, shell.sends.get(key), cx))
        .into_any_element()
}
