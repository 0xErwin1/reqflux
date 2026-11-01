//! The selected call's detail in the MCP approvals view (mcp-approvals S3), as the artboard
//! `design/bolt-v2/12-agent-approval.html` draws it: the title bar (the tool, `Pending` and
//! the call id), the field grid (Requested by, Environment, Policy class, Tool, Policy,
//! Request), "What it will send" and "Variables it uses" for an execution, the reason field,
//! `Deny`, `Allow for this session` and `Allow once` with their keys, and the footnote.
//!
//! Every value comes from the request (names, ids, masked values) or from the window's own
//! workspace through the engine, masked there ([`super::approvals_model::Detail`]); this
//! module never sees a secret value.
//!
//! It draws with DBFlux's `bolt` as DBFlux's own approvals view does: `ApprovalsMetrics` for
//! the title bar, the field grid, the payload box and the action row, `Badge`, `Input`,
//! `Button` with its keycap, `status_diamond` and the SECRET badge of the request grids.

use bolt::controls::{Button, Input};
use bolt::primitives::{BadgeTone, Chamfer, Icon, TextVariant, status_diamond};
use bolt::tokens::{
    ApprovalsMetrics, Borders, ChamferCut, DocumentMetrics, Feedback, ModalMetrics, Spacing,
};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
    div, prelude::*, relative,
};
use reqflux_engine::approvals::{ApprovalRequest, CallPreview, UsedVariable};
use reqflux_engine::{MASK, SentBody};

use super::approvals::{Pending, REASON_PLACEHOLDER};
use super::approvals_model::{EXECUTE_TOOL, Sends, client_name, shown};
use super::approvals_view::{badge, class_badge};
use super::request::{secret, spans, ui};
use super::rows::labeled_row;
use super::{Shell, region, text_region};
use crate::tokens::{self, method_hue};

/// The text of the view without pending calls.
pub(crate) const EMPTY: &str = "No agent call is waiting";

/// The footnote under the actions.
const FOOTNOTE: &str = "Allow once sends this call only; for this session allows this class in \
                        this workspace until the agent's reqflux mcp exits. Audit logs the \
                        decision, agent and request; never the token.";

/// The most body lines and characters "What it will send" shows.
const BODY_LINES: usize = 8;
const BODY_CHARACTERS: usize = 600;

/// The detail of the selected call, or the empty state when none is pending.
pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    let Some(pending) = shell.approvals.selected_pending() else {
        return div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                ui::role(text_region("approvals-empty"), TextVariant::Body, cx)
                    .aria_label(EMPTY)
                    .child(EMPTY),
            )
            .into_any_element();
    };
    let colors = bolt::colors(cx);
    let mut content = div()
        .id("approval-content")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(ApprovalsMetrics::SECTION_GAP)
        .pl(ApprovalsMetrics::DETAIL_PADDING_X)
        .pr(ApprovalsMetrics::DETAIL_PADDING_X)
        .py(ApprovalsMetrics::SECTION_GAP)
        .child(fields(pending, cx));
    match &pending.detail.sends {
        Sends::Nothing => {}
        Sends::Preview(preview) => {
            content = content
                .child(section("What it will send", sends(preview, cx), cx))
                .child(section("Variables it uses", variables(preview, cx), cx));
        }
        Sends::Unavailable(reason) => {
            content = content.child(section(
                "What it will send",
                unavailable(&pending.request, reason, cx),
                cx,
            ));
        }
    }
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(title_bar(pending, cx))
        .child(content)
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(ApprovalsMetrics::TITLE_GAP)
                .pl(ApprovalsMetrics::DETAIL_PADDING_X)
                .pr(ApprovalsMetrics::DETAIL_PADDING_X)
                .py(ApprovalsMetrics::FOOTER_PADDING_Y)
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .children(reason_field(shell, cx))
                .child(button(Action::Deny, cx))
                .child(button(Action::AllowSession, cx))
                .child(button(Action::AllowOnce, cx)),
        )
        .child(
            ui::role(text_region("approval-footnote"), TextVariant::Caption, cx)
                .aria_label(FOOTNOTE)
                .flex_none()
                .flex()
                .items_center()
                .gap(ApprovalsMetrics::VALUE_ICON_GAP)
                .pl(ApprovalsMetrics::DETAIL_PADDING_X)
                .pr(ApprovalsMetrics::DETAIL_PADDING_X)
                .pb(ApprovalsMetrics::FOOTER_PADDING_Y)
                .child(Icon::new(ui::icon(IconName::Shield)).size(ApprovalsMetrics::VALUE_ICON))
                .child(div().flex_1().min_w_0().child(FOOTNOTE)),
        )
        .into_any_element()
}

/// The title bar: the Bot icon, the tool, `Pending` and the call id.
fn title_bar(pending: &Pending, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let tool = shown(&pending.request.tool);
    let call = format!("call {}", pending.request.id);
    div()
        .flex_none()
        .h(ApprovalsMetrics::TITLE_HEIGHT)
        .flex()
        .items_center()
        .gap(ApprovalsMetrics::TITLE_GAP)
        .pl(ApprovalsMetrics::DETAIL_PADDING_X)
        .pr(ApprovalsMetrics::DETAIL_PADDING_X)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            Icon::new(ui::icon(IconName::Bot))
                .size(ApprovalsMetrics::TITLE_ICON)
                .color(tint),
        )
        .child(
            ui::role(text_region("approval-title"), TextVariant::Code, cx)
                .aria_label(tool.clone())
                .text_size(ApprovalsMetrics::TITLE_FONT)
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(tool),
        )
        .child(badge(
            "approval-status".to_owned(),
            "Pending".to_owned(),
            BadgeTone::Accent,
        ))
        .child(div().flex_1())
        .child(
            ui::role(text_region("approval-call-id"), TextVariant::Code, cx)
                .aria_label(call.clone())
                .text_size(ApprovalsMetrics::META_FONT)
                .text_color(colors.muted)
                .child(call),
        )
}

/// The field grid, three to a row: Requested by, Environment, Policy class; Tool, Policy and
/// Request (Target for a call that sends nothing).
fn fields(pending: &Pending, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let request = &pending.request;
    let detail = &pending.detail;
    let client = client_name(request);
    let requested_by = value("approval-client", client.clone(), cx)
        .child(
            Icon::new(ui::icon(IconName::Bot))
                .size(ApprovalsMetrics::VALUE_ICON)
                .color(tint),
        )
        .child(client)
        .into_any_element();
    let environment = match &detail.environment {
        Some(name) => value("approval-environment", name.clone(), cx)
            .child(status_diamond(tint, Feedback::STATUS_DIAMOND))
            .child(name.clone()),
        None => value("approval-environment", "None".to_owned(), cx)
            .text_color(colors.muted)
            .child("None"),
    }
    .into_any_element();
    let tool = shown(&request.tool);
    let policy = request
        .policy
        .as_deref()
        .map_or_else(|| "Not given".to_owned(), shown);
    let target_name = if request.tool == EXECUTE_TOOL {
        "Request"
    } else {
        "Target"
    };
    let first = [
        ("Requested by", requested_by),
        ("Environment", environment),
        (
            "Policy class",
            class_badge("approval-class".to_owned(), &request.class),
        ),
    ];
    let second = [
        (
            "Tool",
            value("approval-tool", tool.clone(), cx)
                .child(tool)
                .into_any_element(),
        ),
        (
            "Policy",
            value("approval-policy", policy.clone(), cx)
                .child(policy)
                .into_any_element(),
        ),
        (
            target_name,
            value("approval-request", detail.target.clone(), cx)
                .font_family(bolt::fonts::ui_family(cx))
                .child(detail.target.clone())
                .into_any_element(),
        ),
    ];
    let grid_row = |cells: [(&'static str, AnyElement); 3]| {
        div()
            .flex()
            .gap(ApprovalsMetrics::GRID_GAP)
            .children(cells.into_iter().map(|(name, value)| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DocumentMetrics::DETAIL_LABEL_GAP)
                    .child(
                        ui::role(div(), TextVariant::Caption, cx)
                            .text_size(DocumentMetrics::DETAIL_LABEL_FONT)
                            .child(name),
                    )
                    .child(value)
            }))
    };
    div()
        .flex()
        .flex_col()
        .gap(ApprovalsMetrics::GRID_GAP)
        .child(grid_row(first))
        .child(grid_row(second))
}

/// A field value `id` labeled `label`, in mono strong text.
fn value(id: &'static str, label: String, cx: &App) -> gpui::Stateful<gpui::Div> {
    ui::role(text_region(id), TextVariant::Code, cx)
        .aria_label(label)
        .flex()
        .items_center()
        .gap(ApprovalsMetrics::VALUE_ICON_GAP)
        .text_color(bolt::colors(cx).strong)
        .whitespace_nowrap()
        .overflow_hidden()
}

/// A section: its uppercase caption over `body`.
fn section(caption: &'static str, body: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(ApprovalsMetrics::SECTION_TITLE_GAP)
        .child(
            ui::role(div(), TextVariant::Label, cx)
                .text_size(ApprovalsMetrics::SECTION_LABEL_FONT)
                .child(caption.to_uppercase()),
        )
        .child(body)
        .into_any_element()
}

/// The box a section's body sits in, element id `id`.
fn boxed(id: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    let colors = bolt::colors(cx);
    region(id).relative().flex().flex_col().child(
        Chamfer::new(ChamferCut::INPUT)
            .fill(colors.background)
            .border(colors.line),
    )
}

/// "What it will send": the method and URL template, the resolved URL and where it was
/// resolved, the headers and the body, masked by the engine.
fn sends(preview: &CallPreview, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let method = shown(&preview.method);
    let template = shown(&preview.inspection.url);
    let url = shown(&preview.url);
    let resolved = preview.environment.as_deref().map_or_else(
        || "resolved with no environment".to_owned(),
        |name| format!("resolved in {}", shown(name)),
    );
    let line = || {
        div()
            .relative()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
    };
    let headers: Vec<AnyElement> = preview
        .headers
        .iter()
        .enumerate()
        .map(|(index, (name, value))| {
            let (name, value) = (shown(name), shown(value));
            labeled_row(
                format!("approval-header-{}", index + 1),
                format!("{name}: {value}"),
            )
            .relative()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .child(div().text_color(colors.info).child(name))
            .child(div().child(": "))
            .children(masked(&value, cx))
            .into_any_element()
        })
        .collect();
    let body = match &preview.body {
        SentBody::None => None,
        SentBody::Text(text) => Some(body_preview(text)),
        SentBody::Omitted(description) => Some(shown(description)),
    };
    ui::role(boxed("approval-sends", cx), TextVariant::Code, cx)
        .pl(ApprovalsMetrics::CODE_PADDING_X)
        .pr(ApprovalsMetrics::CODE_PADDING_X)
        .py(ApprovalsMetrics::CODE_PADDING_Y)
        .text_size(ApprovalsMetrics::CODE_FONT)
        .line_height(relative(ApprovalsMetrics::CODE_LINE_HEIGHT))
        .text_color(colors.strong)
        .child(
            line()
                .child(
                    text_region("approval-method")
                        .aria_label(method.clone())
                        .font_weight(FontWeight::BOLD)
                        .text_color(method_hue(&method, cx))
                        .child(method),
                )
                .child(div().child(" "))
                .child(
                    text_region("approval-url-template")
                        .aria_label(template.clone())
                        .flex()
                        .children(spans::render(
                            &preview.inspection,
                            &preview.inspection.url,
                            cx,
                        )),
                ),
        )
        .child(
            line()
                .gap(Spacing::SM)
                .text_color(colors.muted)
                .child(Icon::new(ui::icon(IconName::ArrowRight)).size(ApprovalsMetrics::VALUE_ICON))
                .child(
                    text_region("approval-url")
                        .aria_label(url.clone())
                        .text_color(colors.foreground)
                        .child(url),
                )
                .child(
                    ui::role(
                        text_region("approval-resolved-in"),
                        TextVariant::Caption,
                        cx,
                    )
                    .aria_label(resolved.clone())
                    .child(resolved),
                ),
        )
        .when(!headers.is_empty(), |sends| {
            sends.child(div().h(Spacing::SM)).children(headers)
        })
        .when_some(body, |sends, body| {
            sends.child(div().h(Spacing::SM)).child(
                text_region("approval-body")
                    .relative()
                    .aria_label(body.clone())
                    .whitespace_normal()
                    .child(body),
            )
        })
        .into_any_element()
}

/// `value` as runs, each [`MASK`] in the muted color.
fn masked(value: &str, cx: &App) -> Vec<AnyElement> {
    let muted = bolt::colors(cx).muted;
    let mut runs = Vec::new();
    for (index, run) in value.split(MASK).enumerate() {
        if index > 0 {
            runs.push(div().text_color(muted).child(MASK).into_any_element());
        }
        if !run.is_empty() {
            runs.push(div().child(run.to_owned()).into_any_element());
        }
    }
    runs
}

/// The first [`BODY_LINES`] lines of `text`, at most [`BODY_CHARACTERS`] characters, with
/// `…` when it was cut.
fn body_preview(text: &str) -> String {
    let lines: Vec<&str> = text.lines().take(BODY_LINES + 1).collect();
    let mut kept: String = lines
        .iter()
        .take(BODY_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .filter(|character| *character == '\n' || !character.is_control())
        .take(BODY_CHARACTERS)
        .collect();
    if lines.len() > BODY_LINES || text.chars().count() > kept.chars().count() {
        kept.push('…');
    }
    kept
}

/// What the request carries, when the window could not read the call from its workspace:
/// why, then the method, the masked URL and the targets.
fn unavailable(request: &ApprovalRequest, reason: &str, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let reason = shown(reason);
    let targets = if request.targets.is_empty() {
        "None".to_owned()
    } else {
        shown(&request.targets.join(", "))
    };
    let method = request.method.as_deref().map(shown);
    let url = request.url.as_deref().map(shown).unwrap_or_default();
    boxed("approval-sends", cx)
        .gap(Spacing::SM)
        .pl(ApprovalsMetrics::CODE_PADDING_X)
        .pr(ApprovalsMetrics::CODE_PADDING_X)
        .py(ApprovalsMetrics::CODE_PADDING_Y)
        .child(
            ui::role(
                text_region("approval-unavailable"),
                TextVariant::Caption,
                cx,
            )
            .relative()
            .aria_label(reason.clone())
            .child(reason),
        )
        .when_some(method, |boxed, method| {
            boxed.child(
                ui::role(div(), TextVariant::Code, cx)
                    .relative()
                    .flex()
                    .items_center()
                    .gap(Spacing::SM)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(
                        text_region("approval-method")
                            .aria_label(method.clone())
                            .font_weight(FontWeight::BOLD)
                            .text_color(method_hue(&method, cx))
                            .child(method),
                    )
                    .child(
                        text_region("approval-url")
                            .aria_label(url.clone())
                            .text_color(colors.foreground)
                            .child(url),
                    ),
            )
        })
        .child(
            ui::role(div(), TextVariant::Caption, cx)
                .relative()
                .flex()
                .gap(Spacing::SM)
                .child(div().text_color(colors.muted).child("Targets"))
                .child(
                    text_region("approval-targets")
                        .aria_label(targets.clone())
                        .text_color(colors.strong)
                        .child(targets),
                ),
        )
        .into_any_element()
}

/// "Variables it uses": variable, value and source; a secret's value masked with the
/// SECRET badge.
fn variables(preview: &CallPreview, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let row = || {
        div()
            .relative()
            .flex()
            .items_center()
            .h(ModalMetrics::TABLE_ROW_HEIGHT)
            .pl(ModalMetrics::LIST_ROW_PADDING_X)
            .pr(ModalMetrics::LIST_ROW_PADDING_X)
            .border_b(Borders::THIN)
    };
    let cells = |variable: &UsedVariable| {
        let name = format!("{{{{{}}}}}", shown(&variable.name));
        let value = shown(&variable.value);
        let source = shown(&variable.source.label());
        let label = [
            name.clone(),
            value.clone(),
            if variable.secret {
                "SECRET".to_owned()
            } else {
                String::new()
            },
            source.clone(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        (name, value, source, label)
    };
    let rows: Vec<AnyElement> = preview
        .variables
        .iter()
        .enumerate()
        .map(|(index, variable)| {
            let (name, value, source, label) = cells(variable);
            let value_cell = div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .overflow_hidden()
                .whitespace_nowrap();
            let value_cell = if variable.secret {
                value_cell
                    .child(div().text_color(colors.muted).child(value))
                    .child(secret())
            } else {
                value_cell.text_color(colors.foreground).child(value)
            };
            labeled_row(format!("approval-variable-{}", index + 1), label)
                .relative()
                .flex()
                .items_center()
                .h(ModalMetrics::TABLE_ROW_HEIGHT)
                .pl(ModalMetrics::LIST_ROW_PADDING_X)
                .pr(ModalMetrics::LIST_ROW_PADDING_X)
                .border_b(Borders::THIN)
                .border_color(colors.row_divider)
                .child(
                    div()
                        .w(tokens::APPROVAL_VARIABLE_COLUMN)
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(tint)
                        .child(name),
                )
                .child(value_cell)
                .child(
                    div()
                        .w(tokens::APPROVAL_SOURCE_COLUMN)
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.muted)
                        .child(source),
                )
                .into_any_element()
        })
        .collect();
    boxed("approval-variables", cx)
        .overflow_hidden()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(DocumentMetrics::TABLE_CELL_FONT)
        .child(
            row()
                .border_color(colors.line_2)
                .text_size(ModalMetrics::TABLE_HEADER_FONT)
                .text_color(colors.muted)
                .child(
                    div()
                        .w(tokens::APPROVAL_VARIABLE_COLUMN)
                        .flex_none()
                        .child("variable"),
                )
                .child(div().flex_1().child("value"))
                .child(
                    div()
                        .w(tokens::APPROVAL_SOURCE_COLUMN)
                        .flex_none()
                        .child("from"),
                ),
        )
        .children(rows)
        .into_any_element()
}

/// The reason field, `approval-reason`: `bolt`'s input with the pencil before the text,
/// named by the reason placeholder its state shows.
fn reason_field(shell: &Shell, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let input = shell.approvals.reason.clone()?;
    let muted = bolt::colors(cx).muted;
    Some(
        div()
            .flex_1()
            .min_w_0()
            .child(
                Input::new(&input)
                    .id("approval-reason")
                    .w_full()
                    .aria_label(REASON_PLACEHOLDER)
                    .prefix(
                        Icon::new(ui::icon(IconName::Pencil))
                            .size(ApprovalsMetrics::VALUE_ICON)
                            .color(muted),
                    ),
            )
            .into_any_element(),
    )
}

/// A detail action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Deny,
    AllowSession,
    AllowOnce,
}

/// The button of `action`: Deny in danger, Allow for this session secondary, Allow once
/// primary; each with its key on a keycap, `<button id>-key`.
fn button(action: Action, cx: &mut Context<Shell>) -> AnyElement {
    use reqflux_engine::approvals::ApprovalDecision;
    let (id, label, key, decision) = match action {
        Action::Deny => ("approval-deny", "Deny", "d", ApprovalDecision::Deny),
        Action::AllowSession => (
            "approval-allow-session",
            "Allow for this session",
            "s",
            ApprovalDecision::AllowSession,
        ),
        Action::AllowOnce => (
            "approval-allow",
            "Allow once",
            "a",
            ApprovalDecision::AllowOnce,
        ),
    };
    let button = Button::new(id, label).aria_label(label).kbd(key);
    let button = match action {
        Action::Deny => button.danger().icon(ui::icon(IconName::CircleX)),
        Action::AllowSession => button.secondary().icon(ui::icon(IconName::Check)),
        Action::AllowOnce => button.primary().icon(ui::icon(IconName::Check)),
    };
    button
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.answer_approval(decision, window, cx);
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_preview_keeps_the_first_lines_and_marks_a_cut() {
        assert_eq!(body_preview("{\"a\": 1}"), "{\"a\": 1}");
        let long = (1..=10)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(body_preview(&long), "1\n2\n3\n4\n5\n6\n7\n8…");
    }
}
