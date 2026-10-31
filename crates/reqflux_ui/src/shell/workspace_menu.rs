//! The workspace menu under the title bar workspace selector (recent-workspaces S3, S6): the
//! recent workspaces by name over their path (`In ReqFlux` for a workspace the app keeps),
//! the current one marked, each with `Remove`; then `Open folder…`, `New workspace…` (an
//! inline name field, standalone-workspaces S3), `New workspace in folder…` and, while one is
//! open, `Close workspace`. A recent
//! entry that does not load keeps the menu open over its error, `workspace-error`. Escape or
//! a click outside closes the menu.

use std::path::Path;

use bolt::chamfer::Chamfer;
use bolt::controls::IconButton;
use bolt::tokens::{border, cut, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div, prelude::*,
};
use reqflux_engine::state::RecentWorkspace;

use super::rows::{labeled_row, part};
use super::text as text_role;
use super::{Shell, text_region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// `Open folder…`.
pub(crate) const OPEN_FOLDER: &str = "Open folder…";

/// `New workspace…`.
pub(crate) const NEW_WORKSPACE: &str = "New workspace…";

/// `New workspace in folder…`.
pub(crate) const NEW_WORKSPACE_IN_FOLDER: &str = "New workspace in folder…";

/// `Close workspace`.
pub(crate) const CLOSE_WORKSPACE: &str = "Close workspace";

/// What a menu or empty state action does.
#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    New,
    NewInFolder,
    Close,
}

impl Action {
    fn run(self, shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
        shell.workspace_menu_open = false;
        match self {
            Self::Open => shell.pick_folder(window, cx),
            Self::New => shell.start_new_workspace(window, cx),
            Self::NewInFolder => shell.pick_new(window, cx),
            Self::Close => shell.switch_workspace(None, window, cx),
        }
        cx.notify();
    }
}

/// A clickable action row `id` with `icon` and `label`.
pub(super) fn action_row(
    id: &str,
    icon_name: IconName,
    label: &'static str,
    action: Action,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    text_role::role(labeled_row(id.to_owned(), label.to_owned()), text::BODY, cx)
        .role(Role::MenuItem)
        .flex()
        .items_center()
        .gap(space::MD)
        .h(tokens::ENVIRONMENT_OPTION)
        .pl(space::MD)
        .pr(space::MD)
        .cursor_pointer()
        .hover(|style| style.bg(colors.hover))
        .child(Icon::new(icon_name).size(icon::MD).color(colors.muted))
        .child(label)
        .on_click(cx.listener(move |this, _event, window, cx| action.run(this, window, cx)))
}

/// The recent row `<prefix>-recent-<n>` of `recent`, with its `Remove` button
/// `<prefix>-recent-remove-<n>`, its name over `detail` (its path or `In ReqFlux`);
/// `current` marks the window's workspace.
pub(super) fn recent_row(
    prefix: &'static str,
    index: usize,
    recent: &RecentWorkspace,
    detail: String,
    current: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let n = index + 1;
    let label = format!("{} {detail}", recent.name);
    let open = text_role::role(
        labeled_row(format!("{prefix}-recent-{n}"), label),
        text::BODY,
        cx,
    )
    .role(Role::MenuItem)
    .aria_selected(current)
    .flex_1()
    .min_w_0()
    .h_full()
    .flex()
    .flex_col()
    .justify_center()
    .cursor_pointer()
    .child(
        div()
            .overflow_hidden()
            .whitespace_nowrap()
            .when(current, |name| name.text_color(colors.strong))
            .child(recent.name.clone()),
    )
    .child(
        text_role::role(div(), text::CAPTION, cx)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(detail),
    )
    .on_click(cx.listener(move |this, _event, window, cx| this.open_recent(index, window, cx)));
    let remove = IconButton::new(
        format!("{prefix}-recent-remove-{n}"),
        IconName::Close,
        "Remove",
    )
    .on_click(cx.listener(move |this, _event, _window, cx| this.remove_recent(index, cx)));
    div()
        .relative()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(tokens::WORKSPACE_RECENT_ROW)
        .pl(space::MD)
        .pr(space::SM)
        .hover(|style| style.bg(colors.hover))
        .when(current, |row| {
            row.bg(bolt::accent_washes(cx).selected_item).child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(border::THICK)
                    .bg(tint),
            )
        })
        .child(open)
        .when(current, |row| {
            row.child(
                part(format!("{prefix}-recent-current-{n}"))
                    .child(Icon::new(IconName::Check).size(icon::MD).color(tint)),
            )
        })
        .child(remove)
}

/// Whether `recent` is the workspace at `current`.
pub(super) fn is_current(recent: &RecentWorkspace, current: Option<&Path>) -> bool {
    current == Some(recent.path.as_path())
}

/// The error of the last workspace that did not open, as `workspace-error`.
pub(super) fn error_label(error: &str, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    text_role::role(text_region("workspace-error"), text::BODY_SMALL, cx)
        .aria_label(error.to_owned())
        .pl(space::MD)
        .pr(space::MD)
        .py(space::HALF)
        .text_color(colors.danger)
        .child(error.to_owned())
}

/// A section header in capitals.
fn header(label: &'static str, cx: &App) -> impl IntoElement {
    text_role::role(div(), text::LABEL, cx)
        .pl(space::MD)
        .pr(space::MD)
        .py(space::HALF)
        .child(label)
}

/// The open menu, drawn above the islands and anchored under the selector, over a backdrop
/// the size of the window that closes it on a click outside.
pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let mut menu = super::region("workspace-menu")
        .role(Role::Menu)
        .track_focus(&shell.workspace_menu_focus)
        .occlude()
        .relative()
        .w(tokens::WORKSPACE_MENU_WIDTH)
        .py(space::HALF)
        .flex()
        .flex_col()
        .child(
            Chamfer::new(cut::OVERLAY)
                .fill(colors.raised)
                .edge(colors.line_2, border::THIN),
        )
        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && !this.cancel_new_workspace(window, cx) {
                this.close_workspace_menu(window, cx);
            }
        }));
    if !shell.recents.is_empty() {
        menu = menu.child(header("RECENT", cx));
    }
    let current = shell.workspace_path.as_deref();
    for (index, recent) in shell.recents.iter().enumerate() {
        let selected = is_current(recent, current);
        let detail = shell.recent_detail(recent);
        menu = menu.child(recent_row("workspace", index, recent, detail, selected, cx));
    }
    let divider = bolt::colors(cx).line;
    if let Some(error) = &shell.workspace_error {
        menu = menu.child(error_label(error, cx));
    }
    menu = menu
        .child(div().h(border::THIN).bg(divider).my(space::HALF))
        .child(action_row(
            "workspace-open",
            IconName::Folder,
            OPEN_FOLDER,
            Action::Open,
            cx,
        ))
        .child(action_row(
            "workspace-new",
            IconName::Plus,
            NEW_WORKSPACE,
            Action::New,
            cx,
        ))
        .children(shell.new_name_field(cx))
        .child(action_row(
            "workspace-new-folder",
            IconName::FolderPlus,
            NEW_WORKSPACE_IN_FOLDER,
            Action::NewInFolder,
            cx,
        ));
    if matches!(shell.workspace, OpenedWorkspace::Loaded(_)) {
        menu = menu.child(action_row(
            "workspace-close",
            IconName::Close,
            CLOSE_WORKSPACE,
            Action::Close,
            cx,
        ));
    }
    let backdrop = super::region("workspace-menu-backdrop")
        .occlude()
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| this.close_workspace_menu(window, cx)),
        );
    div()
        .absolute()
        .top_full()
        .left_0()
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(deferred(anchored().child(menu)).with_priority(2))
        .into_any_element()
}
