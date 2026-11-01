//! The response tab row: the `Response` tab with the state of the send (`Recorded response`
//! in the history tab), the `Tests` and
//! `Console` tabs of a response with script results, the time and size at the right, and the
//! indeterminate progress bar under it while sending. The tabs are `bolt`'s document tabs and
//! the status is a `bolt` badge.

use bolt::Colors;
use bolt::composites::{document_tab, document_tab_bar, document_tab_title};
use bolt::primitives::{Badge, BadgeTone, Icon};
use bolt::tokens::{Fields, ResultMetrics, Spacing, TabMetrics};
use gpui::{
    Animation, AnimationExt, App, Context, Div, Hsla, InteractiveElement, ParentElement, Role,
    Stateful, Styled, div, prelude::*, relative,
};
use gpui_kit_assets::IconName;

use super::model::ResponseTab;
use super::sub_tabs::effective_tab;
use crate::shell::request::ui;
use crate::shell::request::{Completed, Received, SendState, StatusClass, elapsed_text, size_text};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, history, region, text_region};
use crate::tokens;

/// The row of the send `send` of the request `id`, `None` before the first send.
pub(super) fn render(id: &str, send: Option<&RequestSend>, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let state = send.map(|send| &send.state);
    let received = match state {
        Some(SendState::Done(Completed::Response(received))) => Some(received),
        _ => None,
    };
    let tab = send
        .zip(received)
        .map(|(send, received)| effective_tab(send.tab, received));
    let response_selected = !matches!(tab, Some(ResponseTab::Tests | ResponseTab::Console));
    let (icon_name, icon_color, status, meta) = match state {
        None | Some(SendState::Idle) => (IconName::Send, tint, None, None),
        Some(SendState::Sending { started, .. }) => (
            IconName::Loader,
            tint,
            Some(("Sending".to_owned(), BadgeTone::Accent)),
            Some(("response-elapsed", elapsed_text(started.elapsed()))),
        ),
        Some(SendState::Done(Completed::Response(received))) => (
            IconName::Send,
            tint,
            Some((
                received.status_text(),
                class_tone(StatusClass::of(received.status)),
            )),
            Some((
                "response-meta",
                format!(
                    "{} · {}",
                    elapsed_text(received.elapsed),
                    size_text(received.size())
                ),
            )),
        ),
        Some(SendState::Done(Completed::Failure(failure))) => (
            IconName::TriangleAlert,
            colors.danger,
            Some(("Failed".to_owned(), BadgeTone::Danger)),
            Some((
                "response-meta",
                format!("{} · {}", elapsed_text(failure.elapsed), size_text(0)),
            )),
        ),
    };
    let request = id.to_owned();
    let icon_color = if response_selected {
        icon_color
    } else {
        colors.muted
    };
    let title = response_title(id);
    let response = tab_chip("response-tab-response", title, response_selected, cx)
        .child(
            Icon::new(ui::icon(icon_name))
                .size(TabMetrics::ICON)
                .color(icon_color),
        )
        .child(document_tab_title(title, response_selected, cx))
        .children(status.map(|(label, tone)| status_chip(label, tone)))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.select_response_tab(&request, ResponseTab::Body, cx);
        }));
    let scripts = received
        .map(|received| script_tabs(received, &colors))
        .unwrap_or_default();
    let script_tabs: Vec<_> = scripts
        .into_iter()
        .map(|(script_tab, label, icon_name, counts)| {
            let request = id.to_owned();
            let selected = tab == Some(script_tab);
            let icon_color = if selected { tint } else { colors.muted };
            tab_chip(
                format!("response-tab-{}", label.to_lowercase()),
                label,
                selected,
                cx,
            )
            .child(
                Icon::new(ui::icon(icon_name))
                    .size(TabMetrics::ICON)
                    .color(icon_color),
            )
            .child(document_tab_title(label, selected, cx))
            .children(counts.into_iter().map(|(count, color)| {
                div()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_size(TabMetrics::RESULT_META_SIZE)
                    .text_color(color)
                    .child(count)
            }))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.select_response_tab(&request, script_tab, cx);
            }))
        })
        .collect();
    document_tab_bar(cx)
        .child(response)
        .children(script_tabs)
        .child(div().flex_1())
        .children(meta.map(|(id, label)| meta_label(id, label, &colors, cx)))
}

/// The title of the response tab: `Recorded response` for the send of a history tab,
/// `Response` otherwise.
fn response_title(id: &str) -> &'static str {
    if history::is_recorded(id) {
        "Recorded response"
    } else {
        "Response"
    }
}

/// One script tab: what it selects, its label, icon and counts.
type ScriptTab = (ResponseTab, &'static str, IconName, Vec<(String, Hsla)>);

/// The `Tests` and `Console` tabs of a response with something to show (S12): `Tests` with
/// its passed and failed counts, `Console` with its entry count.
fn script_tabs(received: &Received, colors: &Colors) -> Vec<ScriptTab> {
    let mut tabs = Vec::new();
    let scripts = &received.scripts;
    if !scripts.tests.is_empty() {
        let passed = scripts.tests.iter().filter(|test| test.passed).count();
        let failed = scripts.tests.len() - passed;
        let mut counts = vec![(format!("{passed} passed"), colors.muted)];
        if failed > 0 {
            counts.push((format!("{failed} failed"), colors.danger));
        }
        tabs.push((
            ResponseTab::Tests,
            "Tests",
            IconName::CircleCheckBig,
            counts,
        ));
    }
    if !scripts.console.is_empty() {
        let counts = vec![(scripts.console.len().to_string(), colors.muted)];
        tabs.push((ResponseTab::Console, "Console", IconName::Code, counts));
    }
    tabs
}

/// A `bolt` document tab `id` named `label`, the selected one raised.
fn tab_chip(id: impl Into<String>, label: &'static str, selected: bool, cx: &App) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    document_tab(gpui::ElementId::Name(id.into()), selected, cx)
        .debug_selector(move || selector)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
}

/// The status badge, element id `response-status`, in the `tone` of its class.
fn status_chip(label: String, tone: BadgeTone) -> Stateful<Div> {
    text_region("response-status")
        .aria_label(label.clone())
        .flex_none()
        .child(Badge::new(label, tone))
}

/// A clock and `label` at the right of the row, element id `id`.
fn meta_label(id: &'static str, label: String, colors: &Colors, cx: &App) -> Stateful<Div> {
    text_region(id)
        .aria_label(label.clone())
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::XXS)
        .whitespace_nowrap()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(ResultMetrics::STATEMENT_FONT)
        .text_color(colors.muted)
        .child(
            Icon::new(ui::icon(IconName::Clock))
                .size(Fields::SEGMENT_ICON)
                .color(colors.muted),
        )
        .child(label)
}

/// The badge tone of a status class.
fn class_tone(class: StatusClass) -> BadgeTone {
    match class {
        StatusClass::Success => BadgeTone::Success,
        StatusClass::Info => BadgeTone::Info,
        StatusClass::Warning => BadgeTone::Warning,
        StatusClass::Danger => BadgeTone::Danger,
    }
}

/// The indeterminate progress bar, element id `response-progress`: an accent segment that
/// crosses the background track again and again.
pub(super) fn progress(cx: &App) -> Stateful<Div> {
    let segment = tokens::RESPONSE_PROGRESS_SEGMENT;
    region("response-progress")
        .flex_none()
        .relative()
        .h(tokens::RESPONSE_PROGRESS)
        .mx(Spacing::MD)
        .mb(Spacing::SM)
        .overflow_hidden()
        .bg(bolt::colors(cx).background)
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(segment))
                .bg(bolt::brand::brand(cx).accent)
                .with_animation(
                    "response-progress-segment",
                    Animation::new(tokens::RESPONSE_PROGRESS_PERIOD).repeat(),
                    move |bar, delta| bar.left(relative(delta * (1.0 + segment) - segment)),
                ),
        )
}
