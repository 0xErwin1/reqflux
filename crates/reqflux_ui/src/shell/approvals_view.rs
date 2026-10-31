//! The MCP approvals view (mcp-approvals S3), `approvals-view`, opened from the rail's
//! `rail-approvals`, the title bar bell, the status bar chip, the palette and Ctrl+Shift+M.
//! It takes the place of the sidebar and document islands: the pending calls on the left,
//! `approvals-list`, longest wait first, and the selected call's detail on the right with a
//! reason field and `Deny`, `Allow for this session` and `Allow once`.
//!
//! The list carries the [`APPROVALS_CONTEXT`] keys: `j`/`k` and the arrows move, `a` allows
//! once, `r` denies. The reason field sits outside it, so typing never answers. A request
//! carries names only, never an argument value; every value shown is cut and stripped of
//! control characters.

use bolt::Island;
use bolt::chamfer::Chamfer;
use bolt::tokens::{border, chrome, cut, family, space, text};
use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    Role, Styled, div, prelude::*,
};
use gpui_component::input::Input;
use reqflux_engine::approvals::{ApprovalDecision, ApprovalRequest};

use super::approvals::{
    APPROVALS_CONTEXT, AllowSelectedApproval, DenySelectedApproval, Pending, SelectNextApproval,
    SelectPreviousApproval,
};
use super::history::now_millis;
use super::rows::labeled_row;
use super::{Shell, region, text as text_role, text_region};
use crate::tokens;

/// The longest value a detail row shows; longer ones are cut.
const MAX_VALUE: usize = 200;

/// The text of the view without pending calls.
pub(crate) const EMPTY: &str = "No agent call is waiting";

/// The footer of the list.
pub(crate) const FOOTER: &str = "Every decision is written to the audit log.";

/// `<n> approval(s) pending` for the status bar, or `None` when none is.
pub(crate) fn pending_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 approval pending".to_owned()),
        n => Some(format!("{n} approvals pending")),
    }
}

/// How long a call has waited: `less than a minute`, `<n> min` or `<n> h`.
pub(crate) fn waiting_label(arrived_at: i64, now: i64) -> String {
    let minutes = now.saturating_sub(arrived_at).max(0) / 60_000;
    match minutes {
        0 => "less than a minute".to_owned(),
        1..60 => format!("{minutes} min"),
        _ => format!("{} h", minutes / 60),
    }
}

/// `at`, milliseconds since the Unix epoch, as `HH:MM:SS UTC`.
pub(crate) fn clock_label(at: i64) -> String {
    let seconds = at.max(0) / 1000 % 86_400;
    format!(
        "{:02}:{:02}:{:02} UTC",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// `value` without control characters, cut to [`MAX_VALUE`] characters.
fn shown(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_VALUE)
        .collect()
}

/// The workspace name of `request`, or `None` when the call has none.
fn workspace_name(request: &ApprovalRequest) -> String {
    request
        .workspace
        .as_ref()
        .map_or_else(|| "None".to_owned(), |workspace| shown(&workspace.name))
}

/// The rows of the detail of `pending`, as (element id, label, value): the tool, the
/// workspace, the targets, for `request_execute` the method and the masked URL, when it was
/// requested and how long it has waited. The class shows as a badge, apart.
pub(crate) fn rows(pending: &Pending, now: i64) -> Vec<(&'static str, &'static str, String)> {
    let request = &pending.request;
    let targets = if request.targets.is_empty() {
        "None".to_owned()
    } else {
        shown(&request.targets.join(", "))
    };
    let mut rows = vec![
        ("approval-tool", "Tool", shown(&request.tool)),
        ("approval-workspace", "Workspace", workspace_name(request)),
        ("approval-targets", "Targets", targets),
    ];
    if let Some(method) = &request.method {
        rows.push(("approval-method", "Method", shown(method)));
    }
    if let Some(url) = &request.url {
        rows.push(("approval-url", "URL", shown(url)));
    }
    rows.push((
        "approval-requested-at",
        "Requested at",
        clock_label(pending.arrived_at),
    ));
    rows.push((
        "approval-waiting",
        "Waiting",
        waiting_label(pending.arrived_at, now),
    ));
    rows
}

/// The color of a class badge: read is info, write and execute warning, destructive danger.
fn class_color(class: &str, cx: &App) -> Hsla {
    let colors = bolt::colors(cx);
    match class {
        "read" => colors.info,
        "destructive" => colors.danger,
        _ => colors.warning,
    }
}

/// The badge of `class`, element id `id`, its label the class.
fn class_badge(id: String, class: &str, cx: &App) -> AnyElement {
    let class = shown(class);
    labeled_row(id, class.clone())
        .flex_none()
        .relative()
        .pl(space::HALF)
        .pr(space::HALF)
        .font_family(family::INTERFACE)
        .text_size(tokens::BADGE_TEXT)
        .font_weight(FontWeight::BOLD)
        .text_color(bolt::colors(cx).on_semantic)
        .child(Chamfer::new(cut::KEYCAP).fill(class_color(&class, cx)))
        .child(div().relative().child(class.to_uppercase()))
        .into_any_element()
}

/// The view: the list of pending calls beside the selected call's detail.
pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    region("approvals-view")
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .gap(chrome::ISLAND_GAP)
        .child(
            div()
                .flex_none()
                .w(chrome::SIDEBAR_WIDTH)
                .h_full()
                .child(Island::new().size_full().child(list(shell, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(Island::new().size_full().child(detail(shell, cx))),
        )
}

/// The list: the `Pending · N` header, a row per call and the audit footer.
fn list(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let (hover, selected_wash, line) = (colors.hover, colors.hover, colors.line);
    let tint = bolt::brand(cx).accent_tint;
    let now = now_millis();
    let selected = shell.approvals.selected_index();
    let count = shell.approvals.pending();
    let pending_label = format!("Pending · {count}");
    let rows: Vec<AnyElement> = shell
        .approvals
        .queue()
        .enumerate()
        .map(|(index, pending)| {
            let request = &pending.request;
            let tool = shown(&request.tool);
            let meta = format!(
                "{} · waiting {}",
                workspace_name(request),
                waiting_label(pending.arrived_at, now)
            );
            let label = format!("{tool} {} {meta}", shown(&request.class));
            let is_selected = selected == Some(index);
            labeled_row(format!("approvals-row-{}", index + 1), label)
                .aria_selected(is_selected)
                .relative()
                .flex_none()
                .flex()
                .flex_col()
                .gap(space::XS)
                .pl(space::LG)
                .pr(space::LG)
                .pt(space::SM)
                .pb(space::SM)
                .border_b(border::THIN)
                .border_color(line)
                .cursor_pointer()
                .when(is_selected, |row| {
                    row.bg(selected_wash).child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(border::THICK)
                            .bg(tint),
                    )
                })
                .when(!is_selected, |row| row.hover(move |style| style.bg(hover)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(space::SM)
                        .child(
                            text_role::role(div(), text::CODE, cx)
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(tool),
                        )
                        .child(class_badge(
                            format!("approvals-row-{}-class", index + 1),
                            &request.class,
                            cx,
                        )),
                )
                .child(
                    text_role::role(div(), text::CAPTION, cx)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(meta),
                )
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.select_approval(index, window, cx);
                }))
                .into_any_element()
        })
        .collect();
    region("approvals-list")
        .track_focus(&shell.approvals.focus)
        .key_context(APPROVALS_CONTEXT)
        .on_action(cx.listener(|this, _: &SelectNextApproval, _, cx| {
            this.step_approval(true, cx);
        }))
        .on_action(cx.listener(|this, _: &SelectPreviousApproval, _, cx| {
            this.step_approval(false, cx);
        }))
        .on_action(cx.listener(|this, _: &AllowSelectedApproval, window, cx| {
            this.answer_approval(ApprovalDecision::AllowOnce, window, cx);
        }))
        .on_action(cx.listener(|this, _: &DenySelectedApproval, window, cx| {
            this.answer_approval(ApprovalDecision::Deny, window, cx);
        }))
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .h(tokens::SIDEBAR_HEADER)
                .flex()
                .items_center()
                .pl(space::LG)
                .pr(space::MD)
                .child(
                    text_role::role(text_region("approvals-pending-label"), text::LABEL, cx)
                        .aria_label(pending_label.clone())
                        .child(pending_label),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .border_t(border::THIN)
                .border_color(line)
                .children(rows),
        )
        .child(
            text_role::role(text_region("approvals-footer"), text::CAPTION, cx)
                .aria_label(FOOTER)
                .flex_none()
                .h(tokens::SIDEBAR_FOOTER)
                .flex()
                .items_center()
                .pl(space::LG)
                .pr(space::LG)
                .child(FOOTER),
        )
}

/// The detail of the selected call, or the empty state when none is pending.
fn detail(shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    let Some(pending) = shell.approvals.selected_pending() else {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                text_role::role(text_region("approvals-empty"), text::BODY, cx)
                    .aria_label(EMPTY)
                    .child(EMPTY),
            )
            .into_any_element();
    };
    let colors = bolt::colors(cx);
    let (strong, muted) = (colors.strong, colors.muted);
    let mut body: Vec<AnyElement> = vec![field_row(
        "Class",
        class_badge("approval-class".to_owned(), &pending.request.class, cx),
        cx,
    )];
    for (id, label, value) in rows(pending, now_millis()) {
        let value = text_role::role(text_region(id), text::BODY, cx)
            .flex_1()
            .min_w_0()
            .text_color(strong)
            .aria_label(value.clone())
            .child(value)
            .into_any_element();
        body.push(field_row(label, value, cx));
    }
    let title = "An agent asks to run a tool";
    div()
        .size_full()
        .flex()
        .flex_col()
        .gap(space::MD)
        .p(space::LG)
        .child(
            text_role::role(div(), text::HEADING, cx)
                .text_color(strong)
                .child(title),
        )
        .children(body)
        .child(
            text_role::role(div(), text::CAPTION, cx)
                .text_color(muted)
                .child("Reason"),
        )
        .children(reason_field(shell, cx))
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
        )
        .into_any_element()
}

/// One detail row: `label` in the label column, then `value`.
fn field_row(label: &'static str, value: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(space::MD)
        .child(
            text_role::role(div(), text::CAPTION, cx)
                .flex_none()
                .w(tokens::APPROVAL_LABEL)
                .text_color(bolt::colors(cx).muted)
                .child(label),
        )
        .child(value)
        .into_any_element()
}

/// The reason field, `approval-reason`; a click anywhere on it gives it the keyboard.
fn reason_field(shell: &Shell, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let input = shell.approvals.reason.clone()?;
    let colors = bolt::colors(cx);
    let target = input.clone();
    Some(
        region("approval-reason")
            .role(Role::TextInput)
            .relative()
            .h(tokens::APPROVAL_REASON)
            .flex()
            .items_center()
            .pl(space::SM)
            .pr(space::SM)
            .child(
                Chamfer::new(cut::CONTROL)
                    .fill(colors.background)
                    .edge(colors.line, border::THIN),
            )
            .on_click(cx.listener(move |_, _event, window, cx| {
                target.update(cx, |state, cx| state.focus(window, cx));
            }))
            .child(
                Input::new(&input)
                    .appearance(false)
                    .bordered(false)
                    .flex_1()
                    .min_w_0()
                    .text_color(colors.strong),
            )
            .into_any_element(),
    )
}

/// A detail button `id` labeled `label` that answers `decision`: Deny in danger, Allow
/// once filled with the accent, Allow for this session plain.
fn button(
    id: &'static str,
    label: &'static str,
    decision: ApprovalDecision,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let (fill, edge, color) = match decision {
        ApprovalDecision::Deny => (colors.raised, colors.danger, colors.danger),
        ApprovalDecision::AllowSession => (colors.raised, colors.line_2, colors.strong),
        ApprovalDecision::AllowOnce => (brand.accent, brand.accent, brand.on_accent),
    };
    let hover = colors.hover;
    text_role::role(text_region(id), text::BODY, cx)
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .relative()
        .flex_none()
        .h(tokens::APPROVAL_BUTTON)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .text_color(color)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(fill)
                .edge(edge, border::THIN),
        )
        .hover(move |style| style.bg(hover))
        .child(div().relative().child(label))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.answer_approval(decision, window, cx);
        }))
        .into_any_element()
}
