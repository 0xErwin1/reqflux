//! The response tab row: the `Response` tab with the state of the send, the `Tests` and
//! `Console` tabs of a response with script results, the time and size at the right, and the
//! indeterminate progress bar under it while sending.

use bolt::chamfer::Chamfer;
use bolt::tokens::{Colors, chrome, cut, family, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    Animation, AnimationExt, App, Context, Div, FontWeight, Hsla, ParentElement, Role, Stateful,
    Styled, div, prelude::*, relative,
};

use super::model::ResponseTab;
use super::sub_tabs::effective_tab;
use crate::shell::request::{Completed, Received, SendState, StatusClass, elapsed_text, size_text};
use crate::shell::rows::part;
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text_region};
use crate::tokens;

/// The row of the send `send` of the request `id`, `None` before the first send.
pub(super) fn render(id: &str, send: Option<&RequestSend>, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
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
            Some(("Sending".to_owned(), tint, None)),
            Some(("response-elapsed", elapsed_text(started.elapsed()))),
        ),
        Some(SendState::Done(Completed::Response(received))) => {
            let color = class_color(StatusClass::of(received.status), colors);
            (
                IconName::Send,
                tint,
                Some((
                    received.status_text(),
                    color,
                    Some(tokens::wash(color, tokens::RESPONSE_STATUS_WASH)),
                )),
                Some((
                    "response-meta",
                    format!(
                        "{} · {}",
                        elapsed_text(received.elapsed),
                        size_text(received.size())
                    ),
                )),
            )
        }
        Some(SendState::Done(Completed::Failure(failure))) => (
            IconName::TriangleAlert,
            colors.danger,
            Some(("Failed".to_owned(), colors.danger, None)),
            Some((
                "response-meta",
                format!("{} · {}", elapsed_text(failure.elapsed), size_text(0)),
            )),
        ),
    };
    let request = id.to_owned();
    let (label_color, icon_color) = match response_selected {
        true => (colors.strong, icon_color),
        false => (colors.muted, colors.muted),
    };
    let response =
        crate::shell::text::role(part("response-tab-response".to_owned()), text::BODY, cx)
            .role(Role::Tab)
            .aria_label("Response")
            .aria_selected(response_selected)
            .relative()
            .flex()
            .items_center()
            .gap(space::SM)
            .h(chrome::DOC_TAB)
            .pl(space::MD)
            .pr(space::MD)
            .whitespace_nowrap()
            .cursor_pointer()
            .text_color(label_color)
            .when(response_selected, |tab| {
                tab.font_weight(FontWeight::SEMIBOLD)
                    .child(Chamfer::new(cut::CONTROL).fill(colors.raised))
            })
            .child(Icon::new(icon_name).size(icon::MD).color(icon_color))
            .child("Response")
            .children(status.map(|(label, color, wash)| status_chip(label, color, wash)))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.select_response_tab(&request, ResponseTab::Body, cx);
            }));
    let scripts = received
        .map(|received| script_tabs(received, colors))
        .unwrap_or_default();
    let script_tabs: Vec<_> = scripts
        .into_iter()
        .map(|(script_tab, label, icon_name, counts)| {
            let request = id.to_owned();
            tab_chip(
                format!("response-tab-{}", label.to_lowercase()),
                label,
                icon_name,
                counts,
                tab == Some(script_tab),
                cx,
            )
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.select_response_tab(&request, script_tab, cx);
            }))
        })
        .collect();
    div()
        .flex_none()
        .h(tokens::RESPONSE_TAB_ROW)
        .flex()
        .items_center()
        .gap(space::XS)
        .pl(space::MD)
        .pr(space::MD)
        .child(response)
        .children(script_tabs)
        .child(div().flex_1())
        .children(meta.map(|(id, label)| meta_label(id, label, colors)))
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

/// A selectable tab `label` with its icon and optional counts, element id `id`.
fn tab_chip(
    id: String,
    label: &'static str,
    icon_name: IconName,
    counts: Vec<(String, Hsla)>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let (label_color, icon_color) = if selected {
        (colors.strong, bolt::brand(cx).accent_tint)
    } else {
        (colors.muted, colors.muted)
    };
    crate::shell::text::role(part(id), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(chrome::DOC_TAB)
        .pl(space::MD)
        .pr(space::MD)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(selected, |chip| {
            chip.child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .child(Icon::new(icon_name).size(icon::SM).color(icon_color))
        .child(
            div()
                .text_color(label_color)
                .when(selected, |label| label.font_weight(FontWeight::SEMIBOLD))
                .child(label),
        )
        .children(counts.into_iter().map(|(count, color)| {
            div()
                .font_family(family::MONO)
                .text_size(tokens::SMALL_MONO_TEXT)
                .text_color(color)
                .child(count)
        }))
}

/// The status chip, element id `response-status`, in `color` on `wash` when there is one.
fn status_chip(label: String, color: Hsla, wash: Option<Hsla>) -> Stateful<Div> {
    text_region("response-status")
        .aria_label(label.clone())
        .relative()
        .pl(space::HALF)
        .pr(space::HALF)
        .font_family(family::MONO)
        .text_size(tokens::RESPONSE_STATUS_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .children(wash.map(|wash| Chamfer::new(cut::KEYCAP).fill(wash)))
        .child(label)
}

/// A clock and `label` at the right of the row, element id `id`.
fn meta_label(id: &'static str, label: String, colors: &Colors) -> Stateful<Div> {
    text_region(id)
        .aria_label(label.clone())
        .flex_none()
        .flex()
        .items_center()
        .gap(space::HALF)
        .whitespace_nowrap()
        .font_family(family::MONO)
        .text_size(tokens::RESPONSE_META_TEXT)
        .text_color(colors.muted)
        .child(
            Icon::new(IconName::Clock)
                .size(icon::SM)
                .color(colors.muted),
        )
        .child(label)
}

/// The chip color of a status class.
fn class_color(class: StatusClass, colors: &Colors) -> Hsla {
    match class {
        StatusClass::Success => colors.success,
        StatusClass::Info => colors.info,
        StatusClass::Warning => colors.warning,
        StatusClass::Danger => colors.danger,
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
        .mx(space::MD)
        .mb(space::SM)
        .overflow_hidden()
        .bg(bolt::colors(cx).background)
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(segment))
                .bg(bolt::brand(cx).accent)
                .with_animation(
                    "response-progress-segment",
                    Animation::new(tokens::RESPONSE_PROGRESS_PERIOD).repeat(),
                    move |bar, delta| bar.left(relative(delta * (1.0 + segment) - segment)),
                ),
        )
}
