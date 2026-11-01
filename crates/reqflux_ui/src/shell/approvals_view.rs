//! The MCP approvals view (mcp-approvals S3), `approvals-view`, as the artboard
//! `design/bolt-v2/12-agent-approval.html` draws it: opened from the rail's
//! `rail-approvals`, the title bar bell, the status bar chip, the palette and Ctrl+Shift+M,
//! it takes the document island while the sidebar keeps the collection tree. Under the tab
//! row (the open requests and the active `MCP approvals` tab) a header with the title, the
//! subtitle and Refresh sits over the pending calls on the left, `approvals-list`, longest
//! wait first, and the selected call's detail on the right ([`super::approvals_detail`]).
//!
//! The list carries the [`APPROVALS_CONTEXT`] keys: `j`/`k` and the arrows move, `a` allows
//! once, `s` allows for this session and `d` denies. The reason field sits outside it, so
//! typing never answers. Every value shown is cut and stripped of control characters.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::Keycap;
use bolt_legacy::tokens::{border, cut, family, icon, space, text};
use bolt_legacy::{Icon, IconName, Island};
use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    Role, Styled, div, prelude::*,
};
use reqflux_engine::approvals::ApprovalDecision;

use super::approvals::{
    APPROVALS_CONTEXT, AllowSelectedApproval, AllowSelectedApprovalForSession,
    DenySelectedApproval, Pending, SelectNextApproval, SelectPreviousApproval,
};
use super::approvals_model::{class_label, client_name, waiting_label};
use super::history::now_millis;
use super::rows::{labeled_row, part};
use super::{Shell, approvals_detail, region, tabs, text as text_role, text_region};
use crate::tokens::{self, method_color, wash};

/// The view's title and subtitle.
pub(crate) const TITLE: &str = "MCP approvals";
const SUBTITLE: &str = "agent calls that a policy parked for a person";

/// The footer of the list, after its `j` and `k` keycaps.
const FOOTER: &str = "move · decisions go to the audit log";

/// The color of the class `class`: read is info, write and execute warning, destructive
/// danger.
pub(super) fn class_color(class: &str, cx: &App) -> Hsla {
    let colors = bolt_legacy::colors(cx);
    match class {
        "read" => colors.info,
        "destructive" => colors.danger,
        _ => colors.warning,
    }
}

/// A badge `id` printing `label` in `color` on its wash at `alpha`, cut at the keycap size.
pub(super) fn badge(id: String, label: String, color: Hsla, alpha: f32) -> AnyElement {
    labeled_row(id, label.clone())
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .h(tokens::APPROVAL_BADGE)
        .pl(space::HALF)
        .pr(space::HALF)
        .font_family(family::INTERFACE)
        .text_size(tokens::APPROVAL_BADGE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .whitespace_nowrap()
        .child(Chamfer::new(cut::KEYCAP).fill(wash(color, alpha)))
        .child(div().relative().child(label))
        .into_any_element()
}

/// The badge of `class`, element id `id`.
pub(super) fn class_badge(id: String, class: &str, cx: &App) -> AnyElement {
    badge(
        id,
        class_label(class),
        class_color(class, cx),
        tokens::APPROVAL_BADGE_WASH,
    )
}

/// The view in the document island.
pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let line = bolt_legacy::colors(cx).line;
    region("approvals-view").flex_1().min_w_0().h_full().child(
        Island::new()
            .size_full()
            .child(tabs::render(shell, cx))
            .child(header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .border_t(border::THIN)
                    .border_color(line)
                    .child(list(shell, cx))
                    .child(approvals_detail::render(shell, cx)),
            ),
    )
}

/// The header: the Bot icon, the title, the subtitle and Refresh.
fn header(cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    let tint = bolt_legacy::brand(cx).accent_tint;
    let hover = colors.hover;
    div()
        .flex_none()
        .h(tokens::APPROVALS_HEADER)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .border_t(border::THIN)
        .border_color(colors.line)
        .child(Icon::new(IconName::Bot).size(icon::LG).color(tint))
        .child(
            text_role::role(text_region("approvals-title"), text::BODY, cx)
                .aria_label(TITLE)
                .text_size(tokens::APPROVALS_TITLE_TEXT)
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(TITLE),
        )
        .child(
            text_role::role(text_region("approvals-subtitle"), text::BODY, cx)
                .aria_label(SUBTITLE)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(SUBTITLE),
        )
        .child(div().flex_1())
        .child(
            text_role::role(text_region("approvals-refresh"), text::BODY, cx)
                .role(Role::Button)
                .aria_label("Refresh")
                .cursor_pointer()
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .gap(space::SM)
                .h(bolt_legacy::tokens::chrome::CONTROL)
                .pl(space::MD)
                .pr(space::MD)
                .text_size(bolt_legacy::tokens::field::FONT)
                .text_color(colors.foreground)
                .child(Chamfer::new(cut::CONTROL).fill(colors.raised))
                .hover(move |style| style.bg(hover))
                .child(
                    Icon::new(IconName::RefreshCw)
                        .size(icon::ITEM)
                        .color(colors.foreground),
                )
                .child(div().relative().child("Refresh"))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.refresh_approvals(window, cx);
                })),
        )
}

/// The list: the `PENDING · N` header, a row per call and the keys footer.
fn list(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = *bolt_legacy::colors(cx);
    let count = shell.approvals.pending();
    let pending_label = format!("Pending · {count}");
    let now = now_millis();
    let selected = shell.approvals.selected_index();
    let rows: Vec<AnyElement> = shell
        .approvals
        .queue()
        .enumerate()
        .map(|(index, pending)| row(index, pending, selected == Some(index), now, cx))
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
        .on_action(
            cx.listener(|this, _: &AllowSelectedApprovalForSession, window, cx| {
                this.answer_approval(ApprovalDecision::AllowSession, window, cx);
            }),
        )
        .on_action(cx.listener(|this, _: &DenySelectedApproval, window, cx| {
            this.answer_approval(ApprovalDecision::Deny, window, cx);
        }))
        .flex_none()
        .w(tokens::APPROVALS_LIST_WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .bg(colors.background)
        .border_r(border::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_none()
                .pl(space::LG)
                .pr(space::LG)
                .pt(space::MD)
                .pb(space::MD)
                .child(
                    text_role::role(text_region("approvals-pending-label"), text::LABEL, cx)
                        .aria_label(pending_label.clone())
                        .text_size(tokens::APPROVAL_CAPTION_TEXT)
                        .child(pending_label.to_uppercase()),
                ),
        )
        .child(
            div()
                .id("approvals-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(rows),
        )
        .child(
            text_role::role(text_region("approvals-footer"), text::CAPTION, cx)
                .aria_label(format!("j k {FOOTER}"))
                .flex_none()
                .flex()
                .items_center()
                .gap(space::XS)
                .pl(space::LG)
                .pr(space::LG)
                .pt(space::MD)
                .pb(space::MD)
                .border_t(border::THIN)
                .border_color(colors.line)
                .whitespace_nowrap()
                .child(Keycap::new("approvals-footer-j", "j"))
                .child(Keycap::new("approvals-footer-k", "k"))
                .child(FOOTER),
        )
}

/// The row `approvals-row-<index + 1>` of `pending`: the tool and its class badge, the
/// method and path (or the target) with the environment, and the client with its wait.
fn row(
    index: usize,
    pending: &Pending,
    selected: bool,
    now: i64,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = *bolt_legacy::colors(cx);
    let tint = bolt_legacy::brand(cx).accent_tint;
    let hover = colors.hover;
    let n = index + 1;
    let request = &pending.request;
    let detail = &pending.detail;
    let tool = super::approvals_model::shown(&request.tool);
    let meta = format!(
        "{} · {}",
        client_name(request),
        waiting_label(pending.arrived_at, now)
    );
    let mut line: Vec<AnyElement> = Vec::new();
    let mut line_label = match detail.request_line(request) {
        Some((method, path)) => {
            line.push(
                div()
                    .text_color(method_color(&method, bolt_legacy::palette(cx)))
                    .child(method.clone())
                    .into_any_element(),
            );
            line.push(div().child(format!(" {path}")).into_any_element());
            format!("{method} {path}")
        }
        None => {
            line.push(div().child(detail.target.clone()).into_any_element());
            detail.target.clone()
        }
    };
    if let Some(environment) = &detail.environment {
        line.push(div().child(" · ").into_any_element());
        line.push(
            div()
                .text_color(tint)
                .child(environment.clone())
                .into_any_element(),
        );
        line_label = format!("{line_label} · {environment}");
    }
    let label = format!("{tool} {} {line_label} {meta}", class_label(&request.class));
    labeled_row(format!("approvals-row-{n}"), label)
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .flex_col()
        .gap(space::HALF)
        .pl(space::LG)
        .pr(space::LG)
        .pt(space::MD)
        .pb(space::MD)
        .border_b(border::THIN)
        .border_color(colors.row_divider)
        .cursor_pointer()
        .when(selected, |row| {
            row.bg(wash(tint, tokens::APPROVAL_SELECTED_WASH)).child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(border::THICK)
                    .bg(tint),
            )
        })
        .when(!selected, |row| row.hover(move |style| style.bg(hover)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::SM)
                .child(Icon::new(IconName::Bot).size(icon::MD).color(tint))
                .child(
                    text_role::role(div(), text::CODE, cx)
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.strong)
                        .child(tool),
                )
                .child(class_badge(
                    format!("approvals-row-{n}-class"),
                    &request.class,
                    cx,
                )),
        )
        .child(
            part(format!("approvals-row-{n}-request"))
                .role(Role::Label)
                .aria_label(line_label)
                .flex()
                .font_family(family::MONO)
                .text_size(tokens::APPROVALS_ROW_REQUEST_TEXT)
                .text_color(colors.foreground)
                .whitespace_nowrap()
                .overflow_hidden()
                .children(line),
        )
        .child(
            part(format!("approvals-row-{n}-meta"))
                .role(Role::Label)
                .aria_label(meta.clone())
                .font_family(family::INTERFACE)
                .text_size(tokens::APPROVALS_ROW_META_TEXT)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(meta),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.select_approval(index, window, cx);
        }))
        .into_any_element()
}
