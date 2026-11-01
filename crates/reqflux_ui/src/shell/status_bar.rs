//! The status bar: chips on the desk, drawn as DBFlux's status bar draws them. The last send
//! of the window on the left, when there is one; on the right the Problems chip, when the
//! config file did not load or there are config or workspace load warnings, which opens the
//! Problems panel above it. Pending approvals count on the title bar bell and the rail badge,
//! not here.

use std::time::Instant;

use bolt::Colors;
use bolt::composites::StatusChip;
use bolt::primitives::Icon;
use bolt::tokens::{Fields, ShellMetrics};
use gpui::{
    Context, FontWeight, Hsla, IntoElement, ParentElement, Role, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::last_request::{LastKind, summary};
use super::problems::{Problems, count};
use super::request::{StatusClass, ui};
use super::rows::labeled_row;
use super::{Shell, problems_panel, region};

pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let last = shell.last_send.as_ref().and_then(|last| {
        let state = &shell.sends.get(&last.id)?.state;
        summary(&last.method, &last.path, state, Instant::now())
    });
    let problems = shell.problems();

    region("status-bar")
        .flex_none()
        .h(ShellMetrics::STATUS_BAR_HEIGHT)
        .w_full()
        .flex()
        .items_center()
        .gap(ShellMetrics::STATUS_BAR_GAP)
        .pl(ShellMetrics::STATUS_BAR_PADDING_X)
        .pr(ShellMetrics::STATUS_BAR_PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ShellMetrics::STATUS_FONT)
        .text_color(colors.muted)
        .children(last.map(|(text, kind)| {
            let (icon, color) = last_icon(kind, &colors, tint);
            StatusChip::new("status-last-request")
                .build(cx)
                .role(Role::Label)
                .aria_label(text.clone())
                .child(
                    Icon::new(ui::icon(icon))
                        .size(ShellMetrics::STATUS_ICON)
                        .color(color),
                )
                .child(
                    div()
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_color(colors.foreground)
                        .child(text),
                )
        }))
        .child(div().flex_1())
        .when(!problems.is_empty(), |bar| {
            bar.child(problems_chip(shell, &problems, window, cx))
        })
}

/// The Problems chip: the alert icon, then `1 error` in danger and `3 warnings` in the body
/// color, leaving out a zero part, and a chevron up. Any error gives it a danger wash and a
/// danger icon; warnings alone keep it on the island fill with a warning icon. The panel
/// anchors to it.
fn problems_chip(
    shell: &Shell,
    problems: &Problems,
    window: &Window,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let errors = problems.has_error().then(|| count(1, "error"));
    let warnings =
        (!problems.warnings.is_empty()).then(|| count(problems.warnings.len(), "warning"));
    // Any error washes the chip in danger; its text keeps the body color and weight.
    let (chip, icon_color) = if problems.has_error() {
        (
            StatusChip::new("status-problems")
                .tone(colors.danger)
                .wash(true),
            colors.danger,
        )
    } else {
        (StatusChip::new("status-problems"), colors.warning)
    };
    let separator = errors.is_some() && warnings.is_some();
    let chip = chip
        .clickable(true)
        .build(cx)
        .role(Role::Button)
        .aria_label(problems.chip_label())
        .text_color(colors.foreground)
        .font_weight(FontWeight::NORMAL)
        .cursor_pointer()
        .child(
            Icon::new(ui::icon(IconName::TriangleAlert))
                .size(ShellMetrics::STATUS_ICON)
                .color(icon_color),
        )
        .children(errors.map(|text| {
            labeled_row("status-problems-errors".to_owned(), text.clone())
                .text_color(colors.danger)
                .child(text)
        }))
        .when(separator, |chip| {
            chip.child(div().text_color(colors.line_2).child("·"))
        })
        .children(warnings.map(|text| {
            labeled_row("status-problems-warnings".to_owned(), text.clone()).child(text)
        }))
        .child(
            Icon::new(ui::icon(IconName::ChevronUp))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        )
        .on_click(cx.listener(|this, _event, window, cx| this.toggle_problems(window, cx)));
    div()
        .relative()
        .flex_none()
        .child(chip)
        .when(shell.problems_open, |anchor| {
            anchor.child(problems_panel::render(shell, problems, window, cx))
        })
}

/// The icon of the last send: a loader in tint while sending, a check for an answer under
/// 400, an alert in warning for a 4xx and in danger for a 5xx or a failure.
fn last_icon(kind: LastKind, colors: &Colors, tint: Hsla) -> (IconName, Hsla) {
    match kind {
        LastKind::Sending => (IconName::Loader, tint),
        LastKind::Answered(StatusClass::Success | StatusClass::Info) => {
            (IconName::Check, colors.success)
        }
        LastKind::Answered(StatusClass::Warning) => (IconName::TriangleAlert, colors.warning),
        LastKind::Answered(StatusClass::Danger) | LastKind::Failed => {
            (IconName::TriangleAlert, colors.danger)
        }
    }
}
