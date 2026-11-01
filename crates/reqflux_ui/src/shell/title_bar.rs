//! The title bar: the workspace and environment over the sidebar column, the command search
//! at the center, and the approvals bell on the right. The environment selector opens the
//! environment menu, the workspace selector the workspace menu and the search the command
//! palette; the bell opens the MCP approvals view and carries the pending count as a badge,
//! `approvals-badge`.
//!
//! The command search and the bell are `bolt`'s, as DBFlux's title bar draws them.

use bolt::composites::{BellUrgency, CommandSearch, NotificationBell};
use bolt::primitives::{Icon, status_diamond};
use bolt::tokens::{Borders, Feedback, Fields, IslandMetrics, ShellMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Role, Stateful,
    StatefulInteractiveElement, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::request::ui;
use super::{Shell, environment_menu, region, text_region, workspace_menu};

/// Shown in place of the environment when none is active.
pub(crate) const ENVIRONMENT_PLACEHOLDER: &str = "No environment";

/// Shown in place of the workspace name when none is loaded.
pub(crate) const WORKSPACE_PLACEHOLDER: &str = "No workspace";

/// The placeholder of the command search field.
pub(crate) const COMMAND_SEARCH: &str = "Search or run a command";

/// The shortcut printed on the command search field.
pub(crate) const COMMAND_SEARCH_KEYS: &str = "Ctrl K";

/// The left inset of the title bar: the desk gap, the activity rail and the gap before the
/// sidebar island, so the workspace selector sits over the sidebar column.
fn sidebar_column(cx: &App) -> Pixels {
    IslandMetrics::GAP + bolt::fonts::ui_px(cx, ShellMetrics::RAIL_WIDTH) + IslandMetrics::GAP
}

/// The accessible label of the environment selector for the active environment `name`.
pub(super) fn environment_label(name: Option<&str>) -> String {
    format!("Environment: {}", name.unwrap_or("none"))
}

/// A selector of the title bar: `leading`, then `label` in `color`, then a chevron.
fn selector(
    id: &'static str,
    label_id: &'static str,
    leading: Option<AnyElement>,
    label: String,
    color: Hsla,
    weight: FontWeight,
    cx: &App,
) -> Stateful<gpui::Div> {
    let colors = bolt::colors(cx);
    region(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(Fields::GAP)
        .h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Spacing::SM)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(Fields::TEXT)
        .children(leading)
        .child(
            text_region(label_id)
                .text_color(color)
                .font_weight(weight)
                .whitespace_nowrap()
                .child(label),
        )
        .child(
            Icon::new(ui::icon(IconName::ChevronDown))
                .size(Fields::CHEVRON)
                .color(colors.muted),
        )
}

/// The workspace and environment selectors. The zone keeps its content width, so on a narrow
/// window the command search shrinks instead of covering it.
fn context(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let mut zone = region("title-context")
        .min_w_0()
        .flex()
        .items_center()
        .gap(Borders::MEDIUM);
    match shell.workspace.name() {
        Some(name) => {
            let button = selector(
                "workspace-selector",
                "workspace-name",
                Some(
                    Icon::new(ui::icon(IconName::LayoutGrid))
                        .size(Fields::LEADING_ICON)
                        .color(colors.muted)
                        .into_any_element(),
                ),
                name.to_owned(),
                colors.strong,
                FontWeight::SEMIBOLD,
                cx,
            );
            zone = zone
                .child(workspace_selector(shell, button, window, cx))
                .child(
                    Icon::new(ui::icon(IconName::ChevronRight))
                        .size(Fields::CHEVRON)
                        .color(colors.line_2),
                )
                .child(environment_selector(shell, window, cx));
        }
        None => {
            let button = selector(
                "workspace-selector",
                "workspace-name",
                None,
                WORKSPACE_PLACEHOLDER.to_owned(),
                colors.muted,
                FontWeight::NORMAL,
                cx,
            );
            zone = zone.child(workspace_selector(shell, button, window, cx));
        }
    }
    div()
        .flex_1()
        .flex()
        .child(div().flex().pl(sidebar_column(cx)).child(zone))
}

/// The workspace selector `button`, with the workspace menu under it while open.
fn workspace_selector(
    shell: &Shell,
    button: Stateful<gpui::Div>,
    window: &Window,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let button = button
        .role(Role::Button)
        .aria_label(format!(
            "Workspace: {}",
            shell.workspace.name().unwrap_or("none")
        ))
        .aria_expanded(shell.workspace_menu_open)
        .cursor_pointer()
        .on_click(cx.listener(|this, _event, window, cx| {
            this.toggle_workspace_menu(window, cx);
        }));
    div()
        .relative()
        .child(button)
        .when(shell.workspace_menu_open, |anchor| {
            anchor.child(workspace_menu::render(shell, window, cx))
        })
}

/// The environment selector, with the environment menu under it while open.
fn environment_selector(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let name = shell.environment_name();
    let (diamond, label, color, weight) = match &name {
        Some(name) => (
            bolt::brand::brand(cx).accent_tint,
            name.clone(),
            colors.strong,
            FontWeight::SEMIBOLD,
        ),
        None => (
            colors.muted,
            ENVIRONMENT_PLACEHOLDER.to_owned(),
            colors.muted,
            FontWeight::NORMAL,
        ),
    };
    let button = selector(
        "environment-selector",
        "environment-name",
        Some(status_diamond(diamond, Feedback::STATUS_DIAMOND).into_any_element()),
        label,
        color,
        weight,
        cx,
    )
    .role(Role::Button)
    .aria_label(environment_label(name.as_deref()))
    .aria_expanded(shell.environment_menu_open)
    .cursor_pointer()
    .on_click(cx.listener(|this, _event, window, cx| {
        this.toggle_environment_menu(window, cx);
    }));
    div()
        .relative()
        .child(button)
        .when(shell.environment_menu_open, |anchor| {
            anchor.child(environment_menu::render(shell, window, cx))
        })
}

pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> impl IntoElement {
    region("title-bar")
        .flex_none()
        .h(ShellMetrics::TITLE_BAR_HEIGHT)
        .w_full()
        .flex()
        .items_center()
        .child(context(shell, window, cx))
        .child(command_search(cx))
        .child(
            div().flex_1().min_w_0().flex().justify_end().child(
                div()
                    .pr(ShellMetrics::TITLE_BAR_PADDING_X)
                    .child(bell(shell.approvals.pending(), cx)),
            ),
        )
}

/// The command search, `command-search`: a trigger, not an editor, that opens the command
/// palette, with the palette's keys on its keycap `command-search-key`. The wrapper lets it
/// shrink on a narrow window instead of covering the workspace selector.
fn command_search(cx: &mut Context<Shell>) -> impl IntoElement {
    let shell = cx.entity().downgrade();
    div().flex_shrink(1.0).min_w_0().overflow_hidden().child(
        CommandSearch::new("command-search", COMMAND_SEARCH)
            .shortcut([COMMAND_SEARCH_KEYS])
            .on_open(move |window, cx| {
                if let Err(error) = shell.update(cx, |this, cx| this.open_palette(window, cx)) {
                    log::warn!("the command search outlived the shell: {error}");
                }
            }),
    )
}

/// The approvals bell, `approvals`: `bolt`'s notification bell with the pending count on its
/// badge, `approvals-badge`, in the accent while an approval waits.
// bolt gap: NotificationBell sets no Button role, so it leaves the automation tree; a labeled
// button wrapper carries the id and the role until it does.
fn bell(pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let urgency = if pending > 0 {
        BellUrgency::Approval
    } else {
        BellUrgency::None
    };
    region("approvals")
        .role(Role::Button)
        .aria_label("Approvals")
        .flex_none()
        .child(
            NotificationBell::new("approvals-bell", "Approvals", pending)
                .badge_id("approvals-badge")
                .urgency(urgency)
                .on_click(cx.listener(|this, _event, window, cx| this.open_approvals(window, cx))),
        )
}
