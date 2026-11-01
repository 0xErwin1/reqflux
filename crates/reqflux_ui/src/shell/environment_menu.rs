//! The environment menu under the title bar environment button: an `ENVIRONMENTS` header, a
//! `No environment` row, then the workspace environments by name. Clicking a row makes it the
//! window's active environment; Escape or a click outside closes the menu.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::tokens::{border, cut, icon, space, text};
use bolt_legacy::{Icon, IconName};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Point, Role, Styled, Window, anchored, deferred, div, prelude::*,
};

use super::rows::labeled_row;
use super::text as text_role;
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

/// The row that makes `slug` active, `None` being No environment.
fn option(
    slug: Option<String>,
    name: String,
    selected: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    let tint = bolt_legacy::brand(cx).accent_tint;
    // `environment-none` cannot collide with an environment, whose slug may well be `none`.
    let id = slug.as_deref().map_or_else(
        || "environment-none".to_owned(),
        |slug| format!("environment-option-{slug}"),
    );
    text_role::role(labeled_row(id, name.clone()), text::BODY, cx)
        .role(Role::MenuItem)
        .aria_selected(selected)
        .relative()
        .flex()
        .items_center()
        .gap(space::MD)
        .h(tokens::ENVIRONMENT_OPTION)
        .pl(space::MD)
        .pr(space::MD)
        .cursor_pointer()
        .hover(|style| style.bg(colors.hover))
        .when(selected, |row| {
            row.bg(bolt_legacy::accent_washes(cx).selected_item)
                .text_color(colors.strong)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(border::THICK)
                        .bg(tint),
                )
        })
        .child(div().flex_1().min_w_0().overflow_hidden().child(name))
        .when(selected, |row| {
            row.child(Icon::new(IconName::Check).size(icon::MD).color(tint))
        })
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.close_environment_menu(window, cx);
            this.set_environment(slug.clone(), cx);
        }))
}

/// The open menu, drawn above the islands and anchored under the button, over a backdrop
/// the size of the window that closes it on a click outside. The backdrop also covers the
/// button, so a click on it closes the menu instead of reopening it.
pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    let active = shell.active_environment.as_deref();
    let mut menu = region("environment-menu")
        .role(Role::Menu)
        .track_focus(&shell.environment_menu_focus)
        .occlude()
        .relative()
        .w(tokens::ENVIRONMENT_MENU_WIDTH)
        .py(space::HALF)
        .flex()
        .flex_col()
        .child(
            Chamfer::new(cut::OVERLAY)
                .fill(colors.raised)
                .edge(colors.line_2, border::THIN),
        )
        .child(
            text_role::role(div(), text::LABEL, cx)
                .pl(space::MD)
                .pr(space::MD)
                .py(space::HALF)
                .child("ENVIRONMENTS"),
        )
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
