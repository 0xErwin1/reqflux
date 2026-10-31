//! The Problems panel, anchored above the Problems chip at the right of the status bar: a
//! header with the title, the count and an Esc keycap; a `CONFIG` section with the config
//! error, when the config file did not load; then a `WARNINGS · N` section with every config
//! and workspace load warning, its file over what happened. Escape or a click outside closes
//! it. Escape is bound in the panel's own [`PROBLEMS_CONTEXT`], which is deeper than the
//! shell's, so it closes the panel before it reaches `Cancel send`.

use bolt::chamfer::Chamfer;
use bolt::controls::Keycap;
use bolt::tokens::{border, cut, family, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    Anchor, AnyElement, App, Context, FontWeight, InteractiveElement, IntoElement, KeyBinding,
    MouseButton, ParentElement, Pixels, Point, Styled, Window, actions, anchored, deferred, div,
    point, prelude::*,
};

use super::problems::{CONFIG_ERROR_TITLE, Problem, Problems};
use super::rows::labeled_row;
use super::text as text_role;
use super::{Shell, region};
use crate::tokens;

/// The key context of the open panel.
const PROBLEMS_CONTEXT: &str = "Problems";

actions!(
    shell,
    [
        /// Closes the Problems panel.
        CloseProblems,
    ]
);

/// Binds Escape in the panel's context.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseProblems,
        Some(PROBLEMS_CONTEXT),
    )]);
}

/// The panel and its backdrop. The backdrop covers the window, the chip included, so a click
/// on the chip closes the panel instead of reopening it.
pub(super) fn render(
    shell: &Shell,
    problems: &Problems,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let panel = region("problems-panel")
        .key_context(PROBLEMS_CONTEXT)
        .track_focus(&shell.problems_focus)
        .occlude()
        .relative()
        .w(tokens::PROBLEMS_PANEL_WIDTH)
        .flex()
        .flex_col()
        .font_family(family::INTERFACE)
        .text_size(tokens::PROBLEMS_TEXT)
        .text_color(colors.foreground)
        .whitespace_normal()
        .child(
            Chamfer::new(cut::OVERLAY)
                .fill(colors.panel)
                .edge(colors.line_2, border::THIN),
        )
        .child(header(problems, cx))
        .children(problems.error.as_ref().map(|error| config_error(error, cx)))
        .when(!problems.warnings.is_empty(), |panel| {
            panel
                .child(caption(
                    "problems-warnings-label",
                    format!("WARNINGS · {}", problems.warnings.len()),
                    cx,
                ))
                .children(
                    problems
                        .warnings
                        .iter()
                        .enumerate()
                        .map(|(index, warning)| warning_row(index, warning, cx)),
                )
        })
        .on_action(cx.listener(|this, _: &CloseProblems, window, cx| {
            this.close_problems(window, cx);
        }));
    let backdrop = region("problems-backdrop")
        .occlude()
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| this.close_problems(window, cx)),
        );
    let above = point(Pixels::ZERO, -space::MD);
    div()
        .absolute()
        .top_0()
        .right_0()
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(
            deferred(
                anchored()
                    .anchor(Anchor::BottomRight)
                    .offset(above)
                    .child(panel),
            )
            .with_priority(2),
        )
        .into_any_element()
}

/// `Problems`, the count and the Esc keycap.
fn header(problems: &Problems, cx: &Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let summary = problems.summary();
    region("problems-header")
        .flex()
        .flex_none()
        .items_center()
        .gap(space::SM)
        .h(tokens::PROBLEMS_HEADER)
        .pl(space::LG)
        .pr(space::LG)
        .border_b(border::THIN)
        .border_color(colors.line)
        .child(
            text_role::role(
                labeled_row("problems-title".to_owned(), "Problems".to_owned()),
                text::BODY,
                cx,
            )
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.strong)
            .child("Problems"),
        )
        .child(
            labeled_row("problems-count".to_owned(), summary.clone())
                .font_family(family::MONO)
                .text_size(tokens::PROBLEMS_COUNT_TEXT)
                .text_color(colors.muted)
                .child(summary),
        )
        .child(div().flex_1())
        .child(Keycap::new("problems-keycap", "Esc"))
}

/// An uppercase section caption.
fn caption(id: &str, label: String, cx: &Context<Shell>) -> impl IntoElement {
    text_role::role(labeled_row(id.to_owned(), label.clone()), text::LABEL, cx)
        .pl(space::LG)
        .pr(space::LG)
        .pt(space::MD)
        .pb(space::HALF)
        .child(label)
}

/// The `CONFIG` caption and the error row: a danger wash with a danger edge, the alert icon,
/// the title and the error, which names the file, line and column and never quotes it.
fn config_error(error: &str, cx: &Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    div()
        .flex()
        .flex_col()
        .child(caption("problems-config-label", "CONFIG".to_owned(), cx))
        .child(
            labeled_row(
                "problems-config-error".to_owned(),
                format!("{CONFIG_ERROR_TITLE}: {error}"),
            )
            .relative()
            .flex()
            .items_start()
            .gap(space::SM)
            .pl(space::LG)
            .pr(space::LG)
            .pt(space::SM)
            .pb(space::MD)
            .bg(tokens::wash(colors.danger, tokens::PROBLEMS_ERROR_WASH))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(border::THICK)
                    .bg(colors.danger),
            )
            .child(
                Icon::new(IconName::TriangleAlert)
                    .size(icon::MD)
                    .color(colors.danger),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(space::HALF)
                    .child(
                        labeled_row(
                            "problems-config-error-title".to_owned(),
                            CONFIG_ERROR_TITLE.to_owned(),
                        )
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.strong)
                        .child(CONFIG_ERROR_TITLE),
                    )
                    .child(
                        labeled_row("problems-config-error-detail".to_owned(), error.to_owned())
                            .font_family(family::MONO)
                            .text_size(tokens::PROBLEMS_DETAIL_TEXT)
                            .text_color(colors.danger)
                            .child(error.to_owned()),
                    ),
            ),
        )
}

/// A warning: the warning icon, then its file, when it names one, over what happened.
fn warning_row(index: usize, warning: &Problem, cx: &Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let label = match &warning.location {
        Some(location) => format!("{location}: {}", warning.message),
        None => warning.message.clone(),
    };
    labeled_row(format!("problems-warning-{index}"), label)
        .flex()
        .items_start()
        .gap(space::SM)
        .pl(space::LG)
        .pr(space::LG)
        .py(space::SM)
        .border_t(border::THIN)
        .border_color(colors.row_divider)
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(icon::CHIP)
                .color(colors.warning),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .text_size(tokens::PROBLEMS_DETAIL_TEXT)
                .children(warning.location.clone().map(|location| {
                    labeled_row(
                        format!("problems-warning-{index}-location"),
                        location.clone(),
                    )
                    .font_family(family::MONO)
                    .child(location)
                }))
                .child(
                    labeled_row(
                        format!("problems-warning-{index}-message"),
                        warning.message.clone(),
                    )
                    .text_color(colors.muted)
                    .child(warning.message.clone()),
                ),
        )
}
