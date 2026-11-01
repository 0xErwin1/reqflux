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
//!
//! It draws with DBFlux's `bolt`: the document `Island`, `DocumentMetrics` for the header,
//! `ApprovalsMetrics` (DBFlux's own approvals view) with `ListRow` for the pending rows,
//! `Badge`, `Kbd` and `Button`.

use bolt::composites::{Island, ListRow};
use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, Icon, Kbd, TextVariant};
use bolt::tokens::{ApprovalsMetrics, Borders, DocumentMetrics};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, Styled,
    div, prelude::*,
};
use reqflux_engine::approvals::ApprovalDecision;

use super::approvals::{
    APPROVALS_CONTEXT, AllowSelectedApproval, AllowSelectedApprovalForSession,
    DenySelectedApproval, Pending, SelectNextApproval, SelectPreviousApproval,
};
use super::approvals_model::{class_label, client_name, waiting_label};
use super::history::now_millis;
use super::request::ui;
use super::rows::{labeled_row, part};
use super::{Shell, approvals_detail, region, tabs, text_region};
use crate::tokens::method_hue;

/// The view's title and subtitle.
pub(crate) const TITLE: &str = "MCP approvals";
const SUBTITLE: &str = "agent calls that a policy parked for a person";

/// The footer of the list, after its `j` and `k` keycaps.
const FOOTER: &str = "move · decisions go to the audit log";

/// The tone of the class `class`: read is info, write and execute warning, destructive
/// danger.
pub(super) fn class_tone(class: &str) -> BadgeTone {
    match class {
        "read" => BadgeTone::Info,
        "destructive" => BadgeTone::Danger,
        _ => BadgeTone::Warning,
    }
}

/// `bolt`'s badge printing `label` in `tone`, found by tests as `id`.
// bolt gap: an id on Badge names no accessible node, so a labeled wrapper carries the id
// and the label the automation tree reads.
pub(super) fn badge(id: String, label: String, tone: BadgeTone) -> AnyElement {
    labeled_row(id, label.clone())
        .flex_none()
        .child(Badge::new(label, tone))
        .into_any_element()
}

/// The badge of `class`, element id `id`.
pub(super) fn class_badge(id: String, class: &str) -> AnyElement {
    badge(id, class_label(class), class_tone(class))
}

/// `bolt`'s keycap `key`, found by tests as `id`.
pub(super) fn keycap(id: impl Into<String>, key: &'static str) -> AnyElement {
    Kbd::new(key)
        .id(gpui::ElementId::Name(id.into().into()))
        .into_any_element()
}

/// The view in the document island.
pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let line = bolt::colors(cx).line;
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
                    .border_t(Borders::THIN)
                    .border_color(line)
                    .child(list(shell, cx))
                    .child(approvals_detail::render(shell, cx)),
            ),
    )
}

/// The header: the Bot icon, the title, the subtitle and Refresh.
fn header(cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    div()
        .flex_none()
        .h(DocumentMetrics::HEADER_HEIGHT)
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(
            Icon::new(ui::icon(IconName::Bot))
                .size(DocumentMetrics::TITLE_ICON)
                .color(tint),
        )
        .child(
            ui::role(text_region("approvals-title"), TextVariant::Body, cx)
                .aria_label(TITLE)
                .text_size(DocumentMetrics::TITLE_FONT)
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(TITLE),
        )
        .child(
            ui::role(text_region("approvals-subtitle"), TextVariant::Body, cx)
                .aria_label(SUBTITLE)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(SUBTITLE),
        )
        .child(div().flex_1())
        .child(
            Button::new("approvals-refresh", "Refresh")
                .secondary()
                .icon(ui::icon(IconName::RefreshCw))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.refresh_approvals(window, cx);
                })),
        )
}

/// The list: the `PENDING · N` header, a row per call and the keys footer.
fn list(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
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
        .w(ApprovalsMetrics::LIST_WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .bg(colors.background)
        .border_r(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_none()
                .pl(ApprovalsMetrics::LIST_PADDING_X)
                .pr(ApprovalsMetrics::LIST_PADDING_X)
                .py(ApprovalsMetrics::LIST_PADDING_Y)
                .child(
                    ui::role(
                        text_region("approvals-pending-label"),
                        TextVariant::Label,
                        cx,
                    )
                    .aria_label(pending_label.clone())
                    .text_size(ApprovalsMetrics::SECTION_LABEL_FONT)
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
            ui::role(text_region("approvals-footer"), TextVariant::Caption, cx)
                .aria_label(format!("j k {FOOTER}"))
                .flex_none()
                .flex()
                .items_center()
                .gap(ApprovalsMetrics::HINT_GAP)
                .pl(ApprovalsMetrics::LIST_PADDING_X)
                .pr(ApprovalsMetrics::LIST_PADDING_X)
                .py(ApprovalsMetrics::LIST_PADDING_Y)
                .text_size(DocumentMetrics::TABLE_META_FONT)
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .whitespace_nowrap()
                .child(keycap("approvals-footer-j", "j"))
                .child(keycap("approvals-footer-k", "k"))
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
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
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
                    .text_color(method_hue(&method, cx))
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
    ListRow::new(format!("approvals-row-{n}"))
        .selected(selected)
        .selection_bar(true)
        .build(cx)
        .role(Role::Label)
        .aria_label(label)
        .aria_selected(selected)
        .flex_none()
        .flex()
        .flex_col()
        .gap(ApprovalsMetrics::ROW_GAP)
        .pl(ApprovalsMetrics::LIST_PADDING_X)
        .pr(ApprovalsMetrics::LIST_PADDING_X)
        .py(ApprovalsMetrics::LIST_PADDING_Y)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex()
                .items_center()
                .gap(DocumentMetrics::GAP)
                .child(
                    Icon::new(ui::icon(IconName::Bot))
                        .size(ApprovalsMetrics::ROW_ICON)
                        .color(tint),
                )
                .child(
                    ui::role(div(), TextVariant::Code, cx)
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
                )),
        )
        .child(
            ui::role(
                part(format!("approvals-row-{n}-request")),
                TextVariant::Code,
                cx,
            )
            .role(Role::Label)
            .aria_label(line_label)
            .flex()
            .text_size(DocumentMetrics::TABLE_META_FONT)
            .whitespace_nowrap()
            .overflow_hidden()
            .children(line),
        )
        .child(
            ui::role(
                part(format!("approvals-row-{n}-meta")),
                TextVariant::Caption,
                cx,
            )
            .role(Role::Label)
            .aria_label(meta.clone())
            .text_size(ApprovalsMetrics::META_FONT)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(meta),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.select_approval(index, window, cx);
        }))
        .into_any_element()
}
