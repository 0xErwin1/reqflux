//! The selected call's detail in the MCP approvals view (mcp-approvals S3), as the artboard
//! `design/bolt-v2/12-agent-approval.html` draws it: the title bar (the tool, `Pending` and
//! the call id), the field grid (Requested by, Environment, Policy class, Tool, Policy,
//! Request), "What it will send" and "Variables it uses" for an execution, the reason field,
//! `Deny`, `Allow for this session` and `Allow once` with their keys, and the footnote.
//!
//! Every value comes from the request (names, ids, masked values) or from the window's own
//! workspace through the engine, masked there ([`super::approvals_model::Detail`]); this
//! module never sees a secret value.

use bolt::chamfer::Chamfer;
use bolt::controls::Keycap;
use bolt::tokens::{border, chrome, cut, family, field, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Role,
    Styled, div, prelude::*,
};
use gpui_component::input::Input;
use reqflux_engine::approvals::{ApprovalRequest, CallPreview, UsedVariable};
use reqflux_engine::{MASK, SentBody};

use super::approvals::{Pending, REASON_PLACEHOLDER};
use super::approvals_model::{EXECUTE_TOOL, Sends, client_name, shown};
use super::approvals_view::{badge, class_badge};
use super::diamond::Diamond;
use super::request::{secret_badge, spans};
use super::rows::{labeled_row, part};
use super::{Shell, region, text as text_role, text_region};
use crate::tokens::{self, method_color, wash};

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
                text_role::role(text_region("approvals-empty"), text::BODY, cx)
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
        .gap(tokens::APPROVAL_SECTION_GAP)
        .p(tokens::APPROVAL_PADDING)
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
                .gap(space::MD)
                .pl(tokens::APPROVAL_PADDING)
                .pr(tokens::APPROVAL_PADDING)
                .py(tokens::APPROVAL_ACTIONS_PADDING_Y)
                .border_t(border::THIN)
                .border_color(colors.line)
                .children(reason_field(shell, cx))
                .child(button(Action::Deny, cx))
                .child(button(Action::AllowSession, cx))
                .child(button(Action::AllowOnce, cx)),
        )
        .child(
            text_role::role(text_region("approval-footnote"), text::CAPTION, cx)
                .aria_label(FOOTNOTE)
                .flex_none()
                .flex()
                .items_center()
                .gap(space::SM)
                .pl(tokens::APPROVAL_PADDING)
                .pr(tokens::APPROVAL_PADDING)
                .pb(tokens::APPROVAL_ACTIONS_PADDING_Y)
                .child(Icon::new(IconName::Shield).size(icon::CHIP))
                .child(div().flex_1().min_w_0().child(FOOTNOTE)),
        )
        .into_any_element()
}

/// The title bar: the Bot icon, the tool, `Pending` and the call id.
fn title_bar(pending: &Pending, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let tool = shown(&pending.request.tool);
    let call = format!("call {}", pending.request.id);
    div()
        .flex_none()
        .h(tokens::APPROVAL_TITLE_BAR)
        .flex()
        .items_center()
        .gap(tokens::APPROVAL_CAPTION_GAP)
        .pl(tokens::APPROVAL_PADDING)
        .pr(tokens::APPROVAL_PADDING)
        .border_b(border::THIN)
        .border_color(colors.line)
        .child(Icon::new(IconName::Bot).size(icon::LG).color(tint))
        .child(
            text_region("approval-title")
                .aria_label(tool.clone())
                .font_family(family::MONO)
                .text_size(tokens::APPROVAL_TITLE_TEXT)
                .font_weight(FontWeight::BOLD)
                .text_color(colors.strong)
                .child(tool),
        )
        .child(badge(
            "approval-status".to_owned(),
            "Pending".to_owned(),
            tint,
            tokens::APPROVAL_PENDING_WASH,
        ))
        .child(div().flex_1())
        .child(
            text_region("approval-call-id")
                .aria_label(call.clone())
                .font_family(family::MONO)
                .text_size(tokens::APPROVAL_CALL_ID_TEXT)
                .text_color(colors.muted)
                .child(call),
        )
}

/// The field grid, three to a row: Requested by, Environment, Policy class; Tool, Policy and
/// Request (Target for a call that sends nothing).
fn fields(pending: &Pending, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let request = &pending.request;
    let detail = &pending.detail;
    let client = client_name(request);
    let requested_by = value("approval-client", client.clone(), cx)
        .child(Icon::new(IconName::Bot).size(icon::CHIP).color(tint))
        .child(client)
        .into_any_element();
    let environment = match &detail.environment {
        Some(name) => value("approval-environment", name.clone(), cx)
            .child(Diamond::new(tint))
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
            class_badge("approval-class".to_owned(), &request.class, cx),
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
                .font_family(family::INTERFACE)
                .child(detail.target.clone())
                .into_any_element(),
        ),
    ];
    let grid_row = |cells: [(&'static str, AnyElement); 3]| {
        div()
            .flex()
            .gap(tokens::APPROVAL_FIELD_GAP)
            .children(cells.into_iter().map(|(name, value)| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(tokens::APPROVAL_FIELD_NAME_GAP)
                    .child(
                        div()
                            .font_family(family::INTERFACE)
                            .text_size(tokens::APPROVAL_FIELD_NAME_TEXT)
                            .text_color(colors.muted)
                            .child(name),
                    )
                    .child(value)
            }))
    };
    div()
        .flex()
        .flex_col()
        .gap(tokens::APPROVAL_FIELD_GAP)
        .child(grid_row(first))
        .child(grid_row(second))
}

/// A field value `id` labeled `label`, in mono strong text.
fn value(id: &'static str, label: String, cx: &App) -> gpui::Stateful<gpui::Div> {
    text_region(id)
        .aria_label(label)
        .flex()
        .items_center()
        .gap(space::HALF)
        .font_family(family::MONO)
        .text_size(text::CODE.size)
        .text_color(bolt::colors(cx).strong)
        .whitespace_nowrap()
        .overflow_hidden()
}

/// A section: its uppercase caption over `body`.
fn section(caption: &'static str, body: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(tokens::APPROVAL_CAPTION_GAP)
        .child(
            text_role::role(div(), text::LABEL, cx)
                .text_size(tokens::APPROVAL_CAPTION_TEXT)
                .child(caption.to_uppercase()),
        )
        .child(body)
        .into_any_element()
}

/// The box a section's body sits in, element id `id`.
fn boxed(id: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    let colors = bolt::colors(cx);
    region(id).relative().flex().flex_col().child(
        Chamfer::new(cut::INPUT)
            .fill(colors.background)
            .edge(colors.line, border::THIN),
    )
}

/// "What it will send": the method and URL template, the resolved URL and where it was
/// resolved, the headers and the body, masked by the engine.
fn sends(preview: &CallPreview, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let palette = bolt::palette(cx);
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
            .min_h(tokens::APPROVAL_SEND_LINE)
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
            .min_h(tokens::APPROVAL_SEND_LINE)
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
    boxed("approval-sends", cx)
        .pl(space::LG)
        .pr(space::LG)
        .pt(space::MD)
        .pb(space::MD)
        .font_family(family::MONO)
        .text_size(text::CODE.size)
        .text_color(colors.strong)
        .child(
            line()
                .child(
                    text_region("approval-method")
                        .aria_label(method.clone())
                        .font_weight(FontWeight::BOLD)
                        .text_color(method_color(&method, palette))
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
                .gap(space::SM)
                .text_color(colors.muted)
                .child(Icon::new(IconName::ArrowRight).size(icon::CHIP))
                .child(
                    text_region("approval-url")
                        .aria_label(url.clone())
                        .text_color(colors.foreground)
                        .child(url),
                )
                .child(
                    text_region("approval-resolved-in")
                        .aria_label(resolved.clone())
                        .font_family(family::INTERFACE)
                        .text_size(text::CAPTION.size)
                        .child(resolved),
                ),
        )
        .when(!headers.is_empty(), |sends| {
            sends
                .child(div().h(tokens::APPROVAL_SEND_GAP))
                .children(headers)
        })
        .when_some(body, |sends, body| {
            sends.child(div().h(tokens::APPROVAL_SEND_GAP)).child(
                text_region("approval-body")
                    .relative()
                    .aria_label(body.clone())
                    .line_height(tokens::APPROVAL_SEND_LINE)
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
        .gap(space::SM)
        .pl(space::LG)
        .pr(space::LG)
        .pt(space::MD)
        .pb(space::MD)
        .child(
            text_role::role(text_region("approval-unavailable"), text::CAPTION, cx)
                .relative()
                .aria_label(reason.clone())
                .child(reason),
        )
        .when_some(method, |boxed, method| {
            boxed.child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(space::SM)
                    .font_family(family::MONO)
                    .text_size(text::CODE.size)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(
                        text_region("approval-method")
                            .aria_label(method.clone())
                            .font_weight(FontWeight::BOLD)
                            .text_color(method_color(&method, bolt::palette(cx)))
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
            div()
                .relative()
                .flex()
                .gap(space::SM)
                .text_size(text::CAPTION.size)
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
    let tint = bolt::brand(cx).accent_tint;
    let row = || {
        div()
            .relative()
            .flex()
            .items_center()
            .h(tokens::APPROVAL_TABLE_ROW)
            .pl(space::MD)
            .pr(space::MD)
            .border_b(border::THIN)
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
                .gap(space::SM)
                .overflow_hidden()
                .whitespace_nowrap();
            let value_cell = if variable.secret {
                value_cell
                    .child(div().text_color(colors.muted).child(value))
                    .child(secret_badge(colors))
            } else {
                value_cell.text_color(colors.foreground).child(value)
            };
            labeled_row(format!("approval-variable-{}", index + 1), label)
                .relative()
                .flex()
                .items_center()
                .h(tokens::APPROVAL_TABLE_ROW)
                .pl(space::MD)
                .pr(space::MD)
                .border_b(border::THIN)
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
        .font_family(family::MONO)
        .text_size(tokens::APPROVAL_TABLE_TEXT)
        .child(
            row()
                .border_color(colors.line_2)
                .text_size(tokens::APPROVAL_TABLE_HEADER_TEXT)
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

/// The reason field, `approval-reason`, with its pencil and its placeholder while empty; a
/// click anywhere on it gives it the keyboard.
fn reason_field(shell: &Shell, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let input = shell.approvals.reason.clone()?;
    let colors = bolt::colors(cx);
    let empty = input.read(cx).value().is_empty();
    let target = input.clone();
    Some(
        region("approval-reason")
            .role(Role::TextInput)
            .relative()
            .flex_1()
            .min_w_0()
            .h(chrome::CONTROL)
            .flex()
            .items_center()
            .gap(field::GAP)
            .pl(field::PADDING_X)
            .pr(field::PADDING_X)
            .text_size(field::FONT)
            .child(
                Chamfer::new(cut::CONTROL)
                    .fill(colors.background)
                    .edge(colors.line, border::THIN),
            )
            .child(Icon::new(IconName::Pencil).size(icon::MD))
            .on_click(cx.listener(move |_, _event, window, cx| {
                target.update(cx, |state, cx| state.focus(window, cx));
            }))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .when(empty, |field| {
                        field.child(
                            text_region("approval-reason-placeholder")
                                .aria_label(REASON_PLACEHOLDER)
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .flex()
                                .items_center()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_color(colors.muted)
                                .child(REASON_PLACEHOLDER),
                        )
                    })
                    .child(
                        Input::new(&input)
                            .appearance(false)
                            .bordered(false)
                            .text_color(colors.strong),
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

/// The button of `action`: Deny in danger on its wash, Allow for this session a ghost with a
/// `line_2` edge, Allow once filled with the accent; each with its key on a keycap.
fn button(action: Action, cx: &mut Context<Shell>) -> AnyElement {
    use reqflux_engine::approvals::ApprovalDecision;
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let (id, key_id, label, key, decision) = match action {
        Action::Deny => (
            "approval-deny",
            "approval-deny-key",
            "Deny",
            "d",
            ApprovalDecision::Deny,
        ),
        Action::AllowSession => (
            "approval-allow-session",
            "approval-allow-session-key",
            "Allow for this session",
            "s",
            ApprovalDecision::AllowSession,
        ),
        Action::AllowOnce => (
            "approval-allow",
            "approval-allow-key",
            "Allow once",
            "a",
            ApprovalDecision::AllowOnce,
        ),
    };
    let (chamfer, color, weight, icon_name) = match action {
        Action::Deny => (
            Chamfer::new(cut::CONTROL).fill(wash(colors.danger, tokens::APPROVAL_DENY_WASH)),
            colors.danger,
            FontWeight::SEMIBOLD,
            IconName::CircleX,
        ),
        Action::AllowSession => (
            Chamfer::new(cut::CONTROL).edge(colors.line_2, border::THIN),
            colors.foreground,
            FontWeight::SEMIBOLD,
            IconName::Check,
        ),
        Action::AllowOnce => (
            Chamfer::new(cut::CONTROL).fill(brand.accent),
            brand.on_accent,
            FontWeight::BOLD,
            IconName::Check,
        ),
    };
    let keycap = match action {
        Action::AllowOnce => part(key_id.to_owned())
            .role(Role::Label)
            .aria_label(key)
            .relative()
            .pl(tokens::KEYCAP_PADDING_X)
            .pr(tokens::KEYCAP_PADDING_X)
            .font_family(family::MONO)
            .text_size(tokens::SMALL_MONO_TEXT)
            .font_weight(FontWeight::MEDIUM)
            .bg(tokens::approval_keycap_on_accent())
            .child(key)
            .into_any_element(),
        Action::Deny | Action::AllowSession => Keycap::new(key_id, key).into_any_element(),
    };
    let hover = colors.hover;
    text_region(id)
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(chrome::CONTROL)
        .pl(space::MD)
        .pr(space::MD)
        .font_family(family::INTERFACE)
        .text_size(field::FONT)
        .font_weight(weight)
        .text_color(color)
        .child(chamfer)
        .hover(move |style| style.bg(hover))
        .child(Icon::new(icon_name).size(icon::MD).color(color))
        .child(div().relative().child(label))
        .child(keycap)
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
