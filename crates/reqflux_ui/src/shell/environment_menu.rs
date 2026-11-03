//! The environment menu under the title bar environment button: `bolt`'s menu frame with an
//! `ENVIRONMENTS` header, a `No environment` row, then the workspace environments by name,
//! and under a separator `New environment…` and `Manage environments…` with their keys, as
//! the workspace menu ends with its actions (workspace-menu/01). Clicking an environment makes
//! it the window's active environment; Escape or a click outside closes the menu.

use bolt::composites::{MenuItem, menu_frame, menu_row, render_menu_header, render_separator};
use bolt::primitives::Kbd;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Point, Role, Styled, Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::commands::{CommandId, effective_key, key_label};
use super::request::ui;
use super::title_bar::ENVIRONMENT_PLACEHOLDER;
use super::{Shell, region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The workspace environments as `(slug, name)`, sorted by name.
fn environments(workspace: &OpenedWorkspace) -> Vec<(String, String)> {
    let mut found: Vec<_> = match workspace {
        OpenedWorkspace::Loaded(engine) => engine
            .workspace()
            .environments
            .iter()
            .map(|node| (node.slug.clone(), node.environment.name.clone()))
            .collect(),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
    };
    found.sort_by(|left, right| left.1.cmp(&right.1));
    found
}

/// The row that makes `slug` active, `None` being No environment, checked when `selected`.
fn option(
    slug: Option<String>,
    name: String,
    selected: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    // `environment-none` cannot collide with an environment, whose slug may well be `none`.
    let id = slug.as_deref().map_or_else(
        || "environment-none".to_owned(),
        |slug| format!("environment-option-{slug}"),
    );
    menu_row(
        id,
        &MenuItem::new(name.clone()).checked(selected),
        selected,
        cx,
    )
    .aria_label(name)
    .aria_selected(selected)
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.close_environment_menu(window, cx);
        this.set_environment(slug.clone(), cx);
    }))
}

/// The action row `id` that closes the menu and runs `command`, with its key as
/// `<id>-keycap` when it has one.
fn action(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    command: CommandId,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let key = effective_key(cx, command);
    let row = menu_row(
        id.to_owned(),
        &MenuItem::new(label).icon(ui::icon(icon)),
        false,
        cx,
    )
    .aria_label(label)
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.close_environment_menu(window, cx);
        this.run_command(command, window, cx);
    }));
    if key.is_empty() {
        return row;
    }
    row.child(
        Kbd::new(key.split('-').map(key_label).collect::<Vec<_>>().join(" "))
            .id(gpui::ElementId::Name(format!("{id}-keycap").into())),
    )
}

/// The open menu, drawn above the islands and anchored under the button, over a backdrop
/// the size of the window that closes it on a click outside. The backdrop also covers the
/// button, so a click on it closes the menu instead of reopening it.
pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let active = shell.active_environment.as_deref();
    let mut menu = menu_frame(cx)
        .id("environment-menu")
        .debug_selector(|| "environment-menu".to_owned())
        .role(Role::Menu)
        .track_focus(&shell.environment_menu_focus)
        .occlude()
        .w(tokens::ENVIRONMENT_MENU_WIDTH)
        .child(render_menu_header(&MenuItem::header("ENVIRONMENTS"), cx))
        .child(option(
            None,
            ENVIRONMENT_PLACEHOLDER.to_owned(),
            active.is_none(),
            cx,
        ))
        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                this.close_environment_menu(window, cx);
            }
        }));
    for (slug, name) in environments(&shell.workspace) {
        let selected = active == Some(slug.as_str());
        menu = menu.child(option(Some(slug), name, selected, cx));
    }
    if matches!(shell.workspace, OpenedWorkspace::Loaded(_)) {
        menu = menu
            .child(render_separator(cx))
            .child(action(
                "environment-new",
                IconName::Plus,
                "New environment…",
                CommandId::NewEnvironment,
                cx,
            ))
            .child(action(
                "environment-manage",
                IconName::Settings,
                "Manage environments…",
                CommandId::ShowEnvironments,
                cx,
            ));
    }
    let backdrop = region("environment-menu-backdrop")
        .occlude()
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| this.close_environment_menu(window, cx)),
        );
    div()
        .absolute()
        .top_full()
        .left_0()
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(deferred(anchored().child(menu)).with_priority(2))
        .into_any_element()
}
