//! The workspace menu under the title bar workspace selector (recent-workspaces S3, S6;
//! design-catch-up T4, workspace-menu/01-refined-dropdown with the bolt-v2/02 copy): the
//! `Workspaces` header over a legend of the row icons, the recent workspaces by name over
//! their path (`In ReqFlux` for a workspace the app keeps), the current one marked, each with
//! `Remove`; then `New workspace…` (an inline name field, standalone-workspaces S3),
//! `New workspace in folder…`, `Open workspace…` and, while one is open, `Close workspace`,
//! over the footer note. A recent workspace that does not load keeps the menu open and shows
//! its error in its row, `workspace-error`, with `Retry`; any other error shows under the
//! recents. Escape or a click outside closes the menu.

use std::path::Path;

use bolt::chamfer::Chamfer;
use bolt::controls::{IconButton, Keycap};
use bolt::tokens::{border, chrome, cut, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div, prelude::*,
};
use reqflux_engine::state::RecentWorkspace;

use super::commands::{CommandId, command, key_label};
use super::rows::{labeled_row, part};
use super::text as text_role;
use super::workspace_new::IN_REQFLUX;
use super::{Shell, text_region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The menu header.
pub(crate) const HEADER: &str = "Workspaces";

/// `Open workspace…`: asks for a workspace folder and opens it.
pub(crate) const OPEN_WORKSPACE: &str = "Open workspace…";

/// The action that opens a workspace folder, under its earlier name for the empty state.
pub(crate) const OPEN_FOLDER: &str = OPEN_WORKSPACE;

/// `New workspace…`.
pub(crate) const NEW_WORKSPACE: &str = "New workspace…";

/// `New workspace in folder…`.
pub(crate) const NEW_WORKSPACE_IN_FOLDER: &str = "New workspace in folder…";

/// `Close workspace`.
pub(crate) const CLOSE_WORKSPACE: &str = "Close workspace";

/// The note at the foot of the menu.
pub(crate) const FOOTER_NOTE: &str = "One workspace is open at a time. Drafts stay with their \
                                      workspace and come back when you switch to it.";

/// `Retry` on a recent workspace that did not load.
const RETRY: &str = "Retry";

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

/// A menu action row, followed by the key of the command `key` runs, `<id>-keycap`.
fn menu_action(
    id: &str,
    icon_name: IconName,
    label: &'static str,
    action: Action,
    key: Option<CommandId>,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let row = action_row(id, icon_name, label, action, cx);
    match key.and_then(command) {
        Some(command) => row.child(div().flex_1()).child(Keycap::new(
            format!("{id}-keycap"),
            command
                .key
                .split('-')
                .map(key_label)
                .collect::<Vec<_>>()
                .join(" "),
        )),
        None => row,
    }
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
    failable_recent_row(prefix, index, recent, detail, current, None, cx)
}

/// [`recent_row`], or with `failure`, the error of its last open: the alert icon
/// `<prefix>-recent-failed-<n>`, the error in place of the detail as `<prefix>-error` and
/// `Retry`, `<prefix>-recent-retry-<n>`, over a danger wash.
fn failable_recent_row(
    prefix: &'static str,
    index: usize,
    recent: &RecentWorkspace,
    detail: String,
    current: bool,
    failure: Option<&str>,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let n = index + 1;
    let label = format!("{} {}", recent.name, failure.unwrap_or(&detail));
    let lead = match failure {
        Some(_) => part(format!("{prefix}-recent-failed-{n}")).child(
            Icon::new(IconName::TriangleAlert)
                .size(icon::MD)
                .color(colors.danger),
        ),
        None => {
            let name = if detail == IN_REQFLUX {
                IconName::LayoutGrid
            } else {
                IconName::Folder
            };
            let color = if current { tint } else { colors.muted };
            part(format!("{prefix}-recent-icon-{n}"))
                .child(Icon::new(name).size(icon::MD).color(color))
        }
    };
    let detail_line = match failure {
        Some(error) => text_role::role(
            part(format!("{prefix}-error")).role(Role::Label),
            text::CAPTION,
            cx,
        )
        .aria_label(error.to_owned())
        .text_color(colors.danger)
        .child(error.to_owned()),
        None => text_role::role(
            part(format!("{prefix}-recent-detail-{n}")),
            text::CAPTION,
            cx,
        )
        .child(detail),
    };
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
    .child(detail_line.overflow_hidden().whitespace_nowrap())
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
        .when(failure.is_some(), |row| {
            row.bg(tokens::failed_recent_wash(colors.danger))
        })
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
        .child(lead)
        .child(open)
        .when(current, |row| {
            row.child(
                part(format!("{prefix}-recent-current-{n}"))
                    .child(Icon::new(IconName::Check).size(icon::MD).color(tint)),
            )
        })
        .when(failure.is_some(), |row| {
            row.child(retry_button(
                &format!("{prefix}-recent-retry-{n}"),
                index,
                cx,
            ))
        })
        .child(remove)
}

/// `Retry` with element id `id`: opens the recent workspace at `index` again.
fn retry_button(id: &str, index: usize, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    text_role::role(
        labeled_row(id.to_owned(), RETRY.to_owned()),
        text::BODY_SMALL,
        cx,
    )
    .role(Role::Button)
    .relative()
    .flex_none()
    .flex()
    .items_center()
    .gap(space::XS)
    .h(chrome::CONTROL_INLINE)
    .pl(space::SM)
    .pr(space::SM)
    .cursor_pointer()
    .text_color(colors.foreground)
    .child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
    .child(
        Icon::new(IconName::RefreshCw)
            .size(icon::SM)
            .color(colors.muted),
    )
    .child(RETRY)
    .on_click(cx.listener(move |this, _event, window, cx| this.open_recent(index, window, cx)))
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

/// The header, `workspace-menu-header`, in capitals, with the legend of the row icons,
/// `workspace-legend`, when there are recents.
fn header(legend: bool, cx: &App) -> impl IntoElement {
    let muted = bolt::colors(cx).muted;
    let title = text_role::role(
        labeled_row("workspace-menu-header".to_owned(), HEADER.to_owned()),
        text::LABEL,
        cx,
    )
    .child(HEADER.to_uppercase());
    div()
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .py(space::HALF)
        .child(title)
        .child(div().flex_1())
        .when(legend, |row| {
            row.child(
                text_role::role(
                    labeled_row(
                        "workspace-legend".to_owned(),
                        format!("folder · {}", IN_REQFLUX.to_lowercase()),
                    ),
                    text::CAPTION,
                    cx,
                )
                .flex()
                .items_center()
                .gap(space::XS)
                .child(Icon::new(IconName::Folder).size(icon::SM).color(muted))
                .child("folder ·")
                .child(Icon::new(IconName::LayoutGrid).size(icon::SM).color(muted))
                .child(IN_REQFLUX.to_lowercase()),
            )
        })
}

/// A line between groups of the menu.
fn separator(cx: &App) -> Div {
    div()
        .h(border::THIN)
        .bg(bolt::colors(cx).line)
        .my(space::HALF)
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
    menu = menu.child(header(!shell.recents.is_empty(), cx));
    let current = shell.workspace_path.as_deref();
    let error = shell.workspace_error.as_deref();
    let failed = shell.failed_recent.as_deref();
    let mut error_in_row = false;
    for (index, recent) in shell.recents.iter().enumerate() {
        let selected = is_current(recent, current);
        let detail = shell.recent_detail(recent);
        let failure = error.filter(|_| failed == Some(recent.path.as_path()));
        error_in_row |= failure.is_some();
        menu = menu.child(failable_recent_row(
            "workspace",
            index,
            recent,
            detail,
            selected,
            failure,
            cx,
        ));
    }
    if let (Some(error), false) = (error, error_in_row) {
        menu = menu.child(error_label(error, cx));
    }
    menu = menu
        .child(separator(cx))
        .child(menu_action(
            "workspace-new",
            IconName::Plus,
            NEW_WORKSPACE,
            Action::New,
            Some(CommandId::NewWorkspace),
            cx,
        ))
        .children(shell.new_name_field(cx))
        .child(menu_action(
            "workspace-new-folder",
            IconName::FolderPlus,
            NEW_WORKSPACE_IN_FOLDER,
            Action::NewInFolder,
            None,
            cx,
        ))
        .child(menu_action(
            "workspace-open",
            IconName::ExternalLink,
            OPEN_WORKSPACE,
            Action::Open,
            Some(CommandId::OpenWorkspace),
            cx,
        ));
    if matches!(shell.workspace, OpenedWorkspace::Loaded(_)) {
        menu = menu.child(separator(cx)).child(menu_action(
            "workspace-close",
            IconName::LogOut,
            CLOSE_WORKSPACE,
            Action::Close,
            None,
            cx,
        ));
    }
    menu = menu.child(
        text_role::role(
            labeled_row("workspace-menu-note".to_owned(), FOOTER_NOTE.to_owned()),
            text::CAPTION,
            cx,
        )
        .pl(space::MD)
        .pr(space::MD)
        .pt(space::SM)
        .pb(space::HALF)
        .child(FOOTER_NOTE),
    );
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

#[cfg(test)]
#[path = "workspace_menu_tests.rs"]
mod tests;
