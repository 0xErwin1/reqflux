//! The status bar: chips on the desk. It shows the last send of the window, when there is
//! one, the load warnings count, when there are warnings, the pending approvals count, when
//! an agent waits for one (a click opens the MCP approvals view), and why the config file was not used, when it did not parse.

use std::time::Instant;

use bolt::IconName;
use bolt::controls::StatusChip;
use bolt::tokens::{Colors, chrome};
use gpui::{
    Context, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, prelude::*,
};

use super::approvals_view::pending_label;
use super::last_request::{LastKind, summary};
use super::request::StatusClass;
use super::{Shell, region};

/// `<n> warnings` for the load warnings, or `None` when there are none.
pub(crate) fn warnings_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 warning".to_owned()),
        n => Some(format!("{n} warnings")),
    }
}

pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let last = shell.last_send.as_ref().and_then(|last| {
        let state = &shell.sends.get(&last.id)?.state;
        summary(&last.method, &last.path, state, Instant::now())
    });

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
        .children(
            warnings_label(shell.workspace.warning_count()).map(|warnings| {
                StatusChip::new("status-warnings", warnings)
                    .icon(IconName::TriangleAlert)
                    .icon_color(colors.warning)
            }),
        )
        .children(pending_label(shell.approvals.pending()).map(|pending| {
            div()
                .id("status-approvals-open")
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.open_approvals(window, cx);
                }))
                .child(
                    StatusChip::new("status-approvals", pending)
                        .icon(IconName::Bot)
                        .icon_color(colors.warning),
                )
        }))
        .children(shell.config_warning.as_ref().map(|warning| {
            StatusChip::new("status-config-warning", "Config not loaded")
                .label(warning.clone())
                .icon(IconName::TriangleAlert)
                .icon_color(colors.warning)
        }))
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
