//! The approval dialog (mcp-approvals S3): the first pending request as a modal over the
//! shell, `approval-dialog`, with `Allow once`, `Allow for this session` and `Deny`. A
//! backdrop the size of the window takes clicks outside without answering.

use bolt::chamfer::Chamfer;
use bolt::tokens::{border, cut, space, text};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Point, Role, Styled,
    Window, anchored, deferred, div, point, prelude::*,
};
use reqflux_engine::approvals::{ApprovalDecision, ApprovalRequest};

use super::{Shell, region, text as text_role, text_region};
use crate::tokens;

/// The longest value a dialog row shows; longer ones are cut.
const MAX_VALUE: usize = 200;

/// `<n> approval(s) pending` for the status bar, or `None` when none is.
pub(crate) fn pending_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 approval pending".to_owned()),
        n => Some(format!("{n} approvals pending")),
    }
}

/// `value` without control characters, cut to [`MAX_VALUE`] characters.
fn shown(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_VALUE)
        .collect()
}

/// The rows the dialog shows for `request`, as (element id, label, value): the tool, its
/// class, the workspace, the targets and, for `request_execute`, the method and the masked
/// URL. The request carries names only, never an argument value.
pub(crate) fn rows(request: &ApprovalRequest) -> Vec<(&'static str, &'static str, String)> {
    let workspace = request
        .workspace
        .as_ref()
        .map_or_else(|| "None".to_owned(), |workspace| shown(&workspace.name));
    let targets = if request.targets.is_empty() {
        "None".to_owned()
    } else {
        shown(&request.targets.join(", "))
    };
    let mut rows = vec![
        ("approval-tool", "Tool", shown(&request.tool)),
        ("approval-class", "Class", shown(&request.class)),
        ("approval-workspace", "Workspace", workspace),
        ("approval-targets", "Targets", targets),
    ];
    if let Some(method) = &request.method {
        rows.push(("approval-method", "Method", shown(method)));
    }
    if let Some(url) = &request.url {
        rows.push(("approval-url", "URL", shown(url)));
    }
    rows
}

/// A dialog button `id` labeled `label` that answers `decision`.
fn button(
    id: &'static str,
    label: &'static str,
    decision: ApprovalDecision,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let (hover, line) = (colors.hover, colors.line_2);
    let color = match decision {
        ApprovalDecision::Deny => colors.danger,
        ApprovalDecision::AllowOnce | ApprovalDecision::AllowSession => colors.strong,
    };
    text_role::role(text_region(id), text::BODY, cx)
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .flex_none()
        .h(tokens::APPROVAL_BUTTON)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .border(border::THIN)
        .border_color(line)
        .text_color(color)
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.answer_approval(decision, window, cx);
        }))
        .into_any_element()
}

/// The dialog over the shell while a request is pending.
pub(super) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let request = shell.approvals.shown()?;
    let pending = shell.approvals.pending();
    let colors = bolt::colors(cx);
    let (strong, muted) = (colors.strong, colors.muted);
    let mut body: Vec<AnyElement> = Vec::new();
    for (id, label, value) in rows(request) {
        body.push(
            div()
                .flex()
                .gap(space::MD)
                .child(
                    text_role::role(div(), text::CAPTION, cx)
                        .flex_none()
                        .w(tokens::APPROVAL_LABEL)
                        .text_color(muted)
                        .child(label),
                )
                .child(
                    text_role::role(text_region(id), text::BODY, cx)
                        .flex_1()
                        .min_w_0()
                        .text_color(strong)
                        .aria_label(value.clone())
                        .child(value),
                )
                .into_any_element(),
        );
    }
    let title = "An agent asks to run a tool";
    let queue = (pending > 1).then(|| {
        text_role::role(text_region("approval-queue"), text::CAPTION, cx)
            .child(format!("1 of {pending}"))
    });
    let panel = region("approval-dialog")
        .role(Role::Dialog)
        .aria_label(title)
        .occlude()
        .relative()
        .w(tokens::APPROVAL_WIDTH)
        .pl(space::LG)
        .pr(space::LG)
        .pt(space::LG)
        .pb(space::LG)
        .flex()
        .flex_col()
        .gap(space::MD)
        .child(
            Chamfer::new(cut::OVERLAY)
                .fill(colors.raised)
                .edge(colors.line_2, border::THIN),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    text_role::role(div(), text::BODY, cx)
                        .text_color(strong)
                        .child(title),
                )
                .children(queue),
        )
        .children(body)
        .child(
            div()
                .flex()
                .justify_end()
                .gap(space::SM)
                .child(button("approval-deny", "Deny", ApprovalDecision::Deny, cx))
                .child(button(
                    "approval-allow-session",
                    "Allow for this session",
                    ApprovalDecision::AllowSession,
                    cx,
                ))
                .child(button(
                    "approval-allow",
                    "Allow once",
                    ApprovalDecision::AllowOnce,
                    cx,
                )),
        );
    let viewport = window.viewport_size();
    let left = ((viewport.width - tokens::APPROVAL_WIDTH) / 2.).max(gpui::Pixels::ZERO);
    let top = (viewport.height / 4.).max(gpui::Pixels::ZERO);
    let backdrop = region("approval-backdrop")
        .occlude()
        .w(viewport.width)
        .h(viewport.height);
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(5))
            .child(deferred(anchored().position(point(left, top)).child(panel)).with_priority(6))
            .into_any_element(),
    )
}
