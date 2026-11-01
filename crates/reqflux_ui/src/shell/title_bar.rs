//! The title bar: the workspace and environment over the sidebar column, the command search
//! at the center, and the approvals bell on the right. The environment selector opens the
//! environment menu, the workspace selector the workspace menu and the search the command
//! palette; the bell opens the MCP approvals view and carries the pending count as a badge,
//! `approvals-badge`.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::badge_text;
use bolt_legacy::controls::{Field, IconButton};
use bolt_legacy::tokens::{border, chrome, cut, family, field, icon, rail, space};
use bolt_legacy::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Role, Stateful,
    Styled, Window, div, prelude::*,
};

use super::diamond::Diamond;
use super::{Shell, environment_menu, region, text_region, workspace_menu};
use crate::tokens;

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
fn sidebar_column() -> Pixels {
    chrome::ISLAND_GAP + chrome::RAIL_WIDTH + chrome::ISLAND_GAP
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
    let colors = bolt_legacy::colors(cx);
    region(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(space::SM)
        .h(chrome::CONTROL)
        .pl(field::PADDING_X)
        .pr(space::SM)
        .text_size(field::FONT)
        .children(leading)
        .child(
            text_region(label_id)
                .text_color(color)
                .font_weight(weight)
                .whitespace_nowrap()
                .child(label),
        )
        .child(
            Icon::new(IconName::ChevronDown)
                .size(icon::SM)
                .color(colors.muted),
        )
}

/// The workspace and environment selectors. The zone keeps its content width, so on a narrow
/// window the command search shrinks instead of covering it.
fn context(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    let mut zone = region("title-context")
        .min_w_0()
        .flex()
        .items_center()
        .gap(border::THICK);
    match shell.workspace.name() {
        Some(name) => {
            let button = selector(
                "workspace-selector",
                "workspace-name",
                Some(
                    Icon::new(IconName::LayoutGrid)
                        .size(icon::MD)
                        .color(colors.muted)
                        .into_any_element(),
                ),
                name.to_owned(),
                colors.strong,
                FontWeight::SEMIBOLD,
                cx,
            );
            let separator = colors.line_2;
            zone = zone
                .child(workspace_selector(shell, button, window, cx))
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size(icon::SM)
                        .color(separator),
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
        .child(div().flex().pl(sidebar_column()).child(zone))
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
    let colors = bolt_legacy::colors(cx);
    let name = shell.environment_name();
    let (diamond, label, color, weight) = match &name {
        Some(name) => (
            bolt_legacy::brand(cx).accent_tint,
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
        Some(Diamond::new(diamond).into_any_element()),
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
        .h(chrome::TITLE_BAR)
        .w_full()
        .flex()
        .items_center()
        .child(context(shell, window, cx))
        .child(
            div()
                .id("command-search-button")
                .w(tokens::COMMAND_SEARCH_WIDTH)
                .flex_shrink(1.0)
                .min_w_0()
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, window, cx| this.open_palette(window, cx)))
                .child(
                    Field::new("command-search", COMMAND_SEARCH)
                        .leading_icon(IconName::Search)
                        .trailing_keycap(COMMAND_SEARCH_KEYS)
                        .w_full(),
                ),
        )
        .child(
            div().flex_1().min_w_0().flex().justify_end().child(
                div()
                    .pr(space::MD)
                    .child(bell(shell.approvals.pending(), cx)),
            ),
        )
}

/// The approvals bell, `approvals`, with the pending count on its badge.
fn bell(pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let brand = bolt_legacy::brand(cx);
    let badge = badge_text(Some(pending)).map(|count| {
        text_region("approvals-badge")
            .aria_label(count.clone())
            .absolute()
            .top(rail::BADGE_INSET)
            .right(rail::BADGE_INSET)
            .pl(rail::BADGE_PADDING_X)
            .pr(rail::BADGE_PADDING_X)
            .font_family(family::INTERFACE)
            .text_size(rail::BADGE_FONT)
            .line_height(rail::BADGE_LINE_HEIGHT)
            .font_weight(FontWeight::BOLD)
            .text_color(brand.on_accent)
            .child(Chamfer::new(cut::KEYCAP).fill(brand.accent))
            .child(div().relative().child(count))
    });
    div()
        .relative()
        .child(
            IconButton::new("approvals", IconName::Bell, "Approvals")
                .on_click(cx.listener(|this, _event, window, cx| this.open_approvals(window, cx))),
        )
        .children(badge)
}
