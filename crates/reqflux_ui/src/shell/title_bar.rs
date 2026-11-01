//! The title bar: the workspace and environment over the sidebar column, the command search
//! at the center, and the approvals bell on the right. The environment selector opens the
//! environment menu, the workspace selector the workspace menu and the search the command
//! palette; the bell opens the MCP approvals view and carries the pending count as a badge,
//! `approvals-badge`.
//!
//! The bar, the command search and the bell are drawn as DBFlux's title bar draws them.
// bolt gap: the command search and the notification bell are DBFlux's `dbflux_components`
// composites, not `bolt` ones; `command_search` and `bell` go once `bolt` has them.

use bolt::primitives::{Chamfer, Icon, Kbd, status_diamond};
use bolt::tokens::{
    Borders, ButtonMetrics, ChamferCut, Feedback, Fields, IslandMetrics, ShellMetrics, Spacing,
};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Role, Stateful,
    StatefulInteractiveElement, Styled, Window, div, prelude::*,
};
use gpui_component::tooltip::Tooltip;

use super::rail::{badge_text, count_badge};
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
/// palette. DBFlux's: 420 by 30 px, cut 6, the well fill with a line border, the search icon,
/// the placeholder and the palette's keys as one keycap.
fn command_search(cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let shape = Chamfer::new(ChamferCut::CONTROL)
        .fill(colors.background)
        .border(colors.line)
        .fill_hover(colors.raised)
        .fill_active(colors.raised)
        .interactive("command-search-chamfer");
    region("command-search")
        .role(Role::Button)
        .aria_label(COMMAND_SEARCH)
        .relative()
        .flex()
        .flex_shrink(1.0)
        .min_w_0()
        .w(ShellMetrics::COMMAND_SEARCH_WIDTH)
        .items_center()
        .gap(ShellMetrics::COMMAND_SEARCH_GAP)
        .h(ShellMetrics::COMMAND_SEARCH_HEIGHT)
        .pl(ShellMetrics::COMMAND_SEARCH_PADDING_X)
        .pr(ShellMetrics::COMMAND_SEARCH_PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ShellMetrics::COMMAND_SEARCH_FONT)
        .text_color(colors.muted)
        .cursor_pointer()
        .child(shape)
        .child(
            Icon::new(ui::icon(IconName::Search))
                .size(ShellMetrics::COMMAND_SEARCH_ICON)
                .color(colors.muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(COMMAND_SEARCH),
        )
        // bolt gap: Kbd takes no element id; the wrapper keeps `command-search-keycap`.
        .child(
            text_region("command-search-keycap")
                .aria_label(COMMAND_SEARCH_KEYS)
                .flex_none()
                .child(Kbd::new(COMMAND_SEARCH_KEYS)),
        )
        .on_click(cx.listener(|this, _event, window, cx| this.open_palette(window, cx)))
}

/// The approvals bell, `approvals`, drawn as DBFlux's notification bell: a muted ghost button
/// with nothing pending, else on the tint wash with a tint icon and the pending count on its
/// badge.
fn bell(pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let badge = badge_text(pending);
    let (shape, icon_color) = match badge {
        None => (
            Chamfer::new(ChamferCut::CONTROL)
                .fill_hover(colors.raised)
                .fill_active(colors.raised),
            colors.muted,
        ),
        Some(_) => (
            Chamfer::new(ChamferCut::CONTROL)
                .fill(tint.opacity(ButtonMetrics::SOFT_FILL_REST))
                .fill_hover(tint.opacity(ButtonMetrics::SOFT_FILL_HOVER))
                .fill_active(tint.opacity(ButtonMetrics::SOFT_FILL_PRESSED)),
            tint,
        ),
    };
    region("approvals")
        .role(Role::Button)
        .aria_label("Approvals")
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(ShellMetrics::BELL_WIDTH)
        .h(ShellMetrics::BELL_HEIGHT)
        .cursor_pointer()
        .child(shape.interactive("approvals-bell-chamfer"))
        .child(
            Icon::new(ui::icon(IconName::Bell))
                .size(ShellMetrics::BELL_ICON)
                .color(icon_color),
        )
        .children(badge.map(|text| {
            count_badge("approvals-badge", text, cx)
                .top(ShellMetrics::BELL_BADGE_OFFSET)
                .right(ShellMetrics::BELL_BADGE_OFFSET)
        }))
        .tooltip(|window, cx| Tooltip::new("Approvals").build(window, cx))
        .on_click(cx.listener(|this, _event, window, cx| this.open_approvals(window, cx)))
}
