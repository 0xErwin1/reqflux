//! The status bar: chips on the desk. The last send of the window on the left, when there
//! is one; on the right the Problems chip, when the config file did not load or there are
//! config or workspace load warnings, which opens the Problems panel above it. Pending
//! approvals count on the title bar bell and the rail badge, not here.

use std::time::Instant;

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::StatusChip;
use bolt_legacy::tokens::{Colors, chrome, cut, family, icon, status_chip};
use bolt_legacy::{Icon, IconName};
use gpui::{Context, Hsla, IntoElement, ParentElement, Role, Styled, Window, div, prelude::*};

use super::last_request::{LastKind, summary};
use super::problems::{Problems, count};
use super::request::StatusClass;
use super::rows::labeled_row;
use super::{Shell, problems_panel, region};
use crate::tokens;

pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    let brand = bolt_legacy::brand(cx);
    let last = shell.last_send.as_ref().and_then(|last| {
        let state = &shell.sends.get(&last.id)?.state;
        summary(&last.method, &last.path, state, Instant::now())
    });
    let problems = shell.problems();

    region("status-bar")
        .flex_none()
        .h(chrome::STATUS_BAR)
        .w_full()
        .flex()
        .items_center()
        .gap(chrome::ISLAND_GAP)
        .pl(chrome::ISLAND_GAP)
        .pr(chrome::ISLAND_GAP)
        .children(last.map(|(text, kind)| {
            let (icon, color) = last_icon(kind, colors, brand.accent_tint);
            StatusChip::new("status-last-request", text)
                .icon(icon)
                .icon_color(color)
        }))
        .child(div().flex_1())
        .when(!problems.is_empty(), |bar| {
            bar.child(problems_chip(shell, &problems, window, cx))
        })
}

/// The Problems chip: the alert icon, then `1 error` in danger and `3 warnings` in the body
/// color, leaving out a zero part, and a chevron up. Any error gives it a danger wash and a
/// danger icon; warnings alone keep it neutral with a warning icon. The panel anchors to it.
fn problems_chip(
    shell: &Shell,
    problems: &Problems,
    window: &Window,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    let errors = problems.has_error().then(|| count(1, "error"));
    let warnings =
        (!problems.warnings.is_empty()).then(|| count(problems.warnings.len(), "warning"));
    let (fill, icon_color) = if problems.has_error() {
        (
            tokens::wash(colors.danger, tokens::PROBLEMS_CHIP_WASH),
            colors.danger,
        )
    } else {
        (colors.panel, colors.warning)
    };
    let separator = errors.is_some() && warnings.is_some();
    let chip = labeled_row("status-problems".to_owned(), problems.chip_label())
        .role(Role::Button)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(status_chip::GAP)
        .h(chrome::STATUS_CHIP)
        .pl(status_chip::PADDING_X)
        .pr(status_chip::PADDING_X)
        .font_family(family::INTERFACE)
        .text_size(status_chip::FONT)
        .text_color(colors.foreground)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(Chamfer::new(cut::CONTROL).fill(fill))
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(icon::CHIP)
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
            Icon::new(IconName::ChevronUp)
                .size(icon::SM)
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
