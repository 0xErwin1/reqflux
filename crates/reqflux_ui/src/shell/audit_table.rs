//! The table of the audit view (app-pending S2): the header row and a row per entry the
//! filters keep, newest first, `audit-row-<n>` from 1 at the top, with its time `-time`,
//! class badge `-class`, tool `-tool`, target `-target`, workspace `-workspace` and outcome
//! `-outcome`. A click expands the row into its detail `audit-detail-<n>`: the time as
//! written, the decision, the reason `-reason`, the workspace id, the target ids `-targets`
//! and the request line `-request`. Every value is cut and stripped of control characters.
//!
//! Rows are `bolt`'s `ListRow` at `DocumentMetrics`' table sizes.

use bolt::composites::ListRow;
use bolt::primitives::{Icon, TextVariant, status_diamond};
use bolt::tokens::{Borders, DocumentMetrics, Feedback};
use gpui::{
    AnyElement, Context, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Role, Styled,
    Window, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::audit::{AuditRecord, Decision};

use super::approvals_model::shown;
use super::approvals_view::class_badge;
use super::audit_view::NO_MATCH;
use super::request::ui;
use super::rows::part;
use super::tree_model::Node;
use super::{Shell, region, text_region};
use crate::workspace::OpenedWorkspace;

use crate::tokens::{
    AUDIT_CHEVRON_COLUMN as CHEVRON_WIDTH, AUDIT_CLASS_COLUMN as CLASS_WIDTH,
    AUDIT_OUTCOME_COLUMN as OUTCOME_WIDTH, AUDIT_TIME_COLUMN as TIME_WIDTH,
    AUDIT_TOOL_COLUMN as TOOL_WIDTH, AUDIT_WORKSPACE_COLUMN as WORKSPACE_WIDTH,
};

/// The columns after the chevron, as the header names them.
const COLUMNS: [(&str, Option<Pixels>); 6] = [
    ("Time", Some(TIME_WIDTH)),
    ("Class", Some(CLASS_WIDTH)),
    ("Tool", Some(TOOL_WIDTH)),
    ("Target", None),
    ("Workspace", Some(WORKSPACE_WIDTH)),
    ("Outcome", Some(OUTCOME_WIDTH)),
];

/// The shield note under an expanded row.
const NOTE: &str =
    "The audit log holds identifiers and masked URLs, never an argument value or a secret.";

/// The header row and the rows of `entries`, each with its index in the log.
pub(super) fn render(
    shell: &Shell,
    entries: &[(usize, &AuditRecord)],
    _window: &mut Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let mut rows = region("audit-rows")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col();
    if entries.is_empty() {
        rows = rows.child(
            ui::role(text_region("audit-no-match"), TextVariant::Caption, cx)
                .aria_label(NO_MATCH)
                .pl(DocumentMetrics::PADDING_X)
                .pr(DocumentMetrics::PADDING_X)
                .py(DocumentMetrics::TOOLBAR_PADDING_Y)
                .child(NO_MATCH),
        );
    }
    for (position, &(index, entry)) in entries.iter().enumerate() {
        let n = position + 1;
        let expanded = shell.audit.expanded == Some(index);
        rows = rows.child(row(n, index, entry, expanded, shell, cx));
        if expanded {
            rows = rows.child(detail(n, entry, cx));
        }
    }
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(header(cx))
        .child(rows)
        .into_any_element()
}

/// The header row naming the columns.
fn header(cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let mut header = cells()
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .text_size(DocumentMetrics::TABLE_HEADER_FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(div().flex_none().w(CHEVRON_WIDTH));
    for (name, width) in COLUMNS {
        header = header.child(cell(div(), width).child(name));
    }
    header
}

/// The row `audit-row-<n>` of `entry`, the log's entry `index`.
fn row(
    n: usize,
    index: usize,
    entry: &AuditRecord,
    expanded: bool,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let id = format!("audit-row-{n}");
    let ours = entry.workspace_id.as_deref() == shell.workspace_id.as_deref();
    let target = shown(&super::audit_model::target_text(entry, |target| {
        ours.then(|| node_name(&shell.roots, target))
            .flatten()
            .unwrap_or_else(|| target.to_owned())
    }));
    let workspace = workspace_text(entry, ours, &shell.workspace);
    let time = local_time(entry);
    let tool = shown(&entry.tool);
    let (outcome, outcome_color) = outcome(entry.decision, &colors);
    let label = format!("{time} {tool} {target} {outcome}");
    let mono = bolt::fonts::editor_family(cx);
    let text = |suffix: &str, value: &str| {
        part(format!("{id}-{suffix}"))
            .role(Role::Label)
            .aria_label(value.to_owned())
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(value.to_owned())
    };
    let chevron = Icon::new(ui::icon(if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    }))
    .size(DocumentMetrics::TABLE_CHEVRON)
    .color(colors.muted);
    let element = ListRow::new(id.clone())
        .selected(expanded)
        .build(cx)
        .role(Role::Label)
        .aria_label(label)
        .aria_expanded(expanded);
    ui::role(element, TextVariant::Body, cx)
        .flex_none()
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
        .text_size(DocumentMetrics::TABLE_CELL_FONT)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .when(expanded, |row| {
            row.bg(tint.opacity(DocumentMetrics::EXPANDED_ROW_ALPHA))
        })
        .child(div().flex_none().w(CHEVRON_WIDTH).child(chevron))
        .child(cell(
            text("time", &time)
                .font_family(mono.clone())
                .text_size(DocumentMetrics::TABLE_META_FONT)
                .text_color(colors.muted),
            Some(TIME_WIDTH),
        ))
        .child(
            cell(div(), Some(CLASS_WIDTH)).child(class_badge(format!("{id}-class"), &entry.class)),
        )
        .child(cell(
            text("tool", &tool)
                .font_family(mono.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.strong),
            Some(TOOL_WIDTH),
        ))
        .child(cell(text("target", &target).font_family(mono), None))
        .child(cell(text("workspace", &workspace), Some(WORKSPACE_WIDTH)))
        .child(
            cell(div(), Some(OUTCOME_WIDTH))
                .flex()
                .items_center()
                .gap(Feedback::STATUS_GAP)
                .text_color(outcome_color)
                .child(status_diamond(outcome_color, Feedback::STATUS_DIAMOND))
                .child(text("outcome", outcome)),
        )
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.toggle_audit_entry(index, cx);
        }))
        .into_any_element()
}

/// The detail `audit-detail-<n>` of an expanded row.
fn detail(n: usize, entry: &AuditRecord, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("audit-detail-{n}");
    let request = match (&entry.method, &entry.url) {
        (Some(method), Some(url)) => format!("{method} {url}"),
        _ => "—".to_owned(),
    };
    let fields = [
        ("time", "Time (UTC)", entry.time.clone()),
        ("decision", "Decision", entry.decision.name().to_owned()),
        (
            "reason",
            "Reason",
            entry.reason.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "workspace",
            "Workspace id",
            entry.workspace_id.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        ("targets", "Target ids", entry.targets.join(", ")),
        ("request", "Request", request),
    ];
    let field = |(suffix, name, value): (&str, &str, String)| {
        let value = shown(&value);
        div()
            .flex()
            .flex_col()
            .gap(DocumentMetrics::DETAIL_LABEL_GAP)
            .min_w(DocumentMetrics::DETAIL_FIELD_MIN_WIDTH)
            .child(
                div()
                    .text_size(DocumentMetrics::DETAIL_LABEL_FONT)
                    .text_color(colors.muted)
                    .child(name.to_owned()),
            )
            .child(
                part(format!("{id}-{suffix}"))
                    .role(Role::Label)
                    .aria_label(value.clone())
                    .text_color(colors.foreground)
                    .child(value),
            )
    };
    part(id.clone())
        .flex_none()
        .flex()
        .flex_col()
        .pt(DocumentMetrics::DETAIL_PADDING_TOP)
        .pb(DocumentMetrics::DETAIL_PADDING_BOTTOM)
        .pl(DocumentMetrics::DETAIL_PADDING_LEFT)
        .pr(DocumentMetrics::PADDING_X)
        .bg(colors.background)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .text_size(DocumentMetrics::TABLE_CELL_FONT)
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(DocumentMetrics::DETAIL_FIELD_GAP)
                .children(fields.map(field)),
        )
        .child(
            div()
                .mt(DocumentMetrics::DETAIL_ACTIONS_MARGIN_TOP)
                .flex()
                .items_center()
                .gap(DocumentMetrics::GAP)
                .text_size(DocumentMetrics::TABLE_META_FONT)
                .text_color(colors.muted)
                .child(
                    Icon::new(ui::icon(IconName::Shield))
                        .size(DocumentMetrics::TABLE_ICON)
                        .color(colors.muted),
                )
                .child(NOTE),
        )
        .into_any_element()
}

/// A row of cells.
fn cells() -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(DocumentMetrics::GAP)
        .pl(DocumentMetrics::PADDING_X)
        .pr(DocumentMetrics::PADDING_X)
}

/// `element` as a cell `width` wide, or taking the rest of the row.
fn cell<E: Styled>(element: E, width: Option<Pixels>) -> E {
    match width {
        Some(width) => element.flex_none().w(width).min_w_0(),
        None => element.flex_1().min_w_0(),
    }
}

/// The outcome label and color of `decision`.
fn outcome(decision: Decision, colors: &bolt::Colors) -> (&'static str, Hsla) {
    match decision {
        Decision::Allowed => ("Allowed", colors.success),
        Decision::Refused => ("Refused", colors.danger),
    }
}

/// The local time of `entry`: `HH:MM:SS.mmm` today, else with the date; the written time
/// when it did not parse.
fn local_time(entry: &AuditRecord) -> String {
    use chrono::{DateTime, Local};
    let Some(time) = entry.millis.and_then(DateTime::from_timestamp_millis) else {
        return shown(&entry.time);
    };
    let time = time.with_timezone(&Local);
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M:%S%.3f").to_string()
    } else {
        time.format("%b %d %H:%M").to_string()
    }
}

/// The workspace of `entry`: the open one's name when it is `ours`, else its id.
fn workspace_text(entry: &AuditRecord, ours: bool, workspace: &OpenedWorkspace) -> String {
    match (&entry.workspace_id, workspace) {
        (Some(_), OpenedWorkspace::Loaded(engine)) if ours => {
            shown(&engine.workspace().manifest.name)
        }
        (Some(id), _) => shown(id),
        (None, _) => "—".to_owned(),
    }
}

/// The name of the node `id` in `roots`, searched depth first.
fn node_name(roots: &[Node], id: &str) -> Option<String> {
    roots.iter().find_map(|node| {
        if node.id == id {
            Some(node.name.clone())
        } else {
            node_name(&node.children, id)
        }
    })
}
