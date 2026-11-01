//! The workspace menu under the title bar workspace selector (recent-workspaces S3, S6;
//! design-catch-up T4, workspace-menu/01-refined-dropdown with the bolt-v2/02 copy): the
//! `Workspaces` header over a legend of the row icons, the recent workspaces by name over
//! their path (`In ReqFlux` for a workspace the app keeps), the current one marked, each with
//! `Remove`; then `New workspace…` (an inline name field, standalone-workspaces S3),
//! `New workspace in folder…`, `Open workspace…` and, while one is open, `Workspace
//! settings…` (design-catch-up T7, the settings page) and `Close workspace`, over the footer
//! note. A recent workspace that does not load keeps the menu open and shows
//! its error in its row, `workspace-error`, with `Retry`; any other error shows under the
//! recents. Escape or a click outside closes the menu.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;

use bolt::composites::{ListRow, MenuItem, menu_frame, menu_row, render_separator};
use bolt::controls::Button;
use bolt::primitives::{Icon, Kbd, TextVariant};
use bolt::tokens::{MenuMetrics, ShellMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div,
    prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::state::RecentWorkspace;

use super::commands::{CommandId, command, key_label};
use super::request::ui;
use super::rows::{labeled_row, part};
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

/// `Import Postman collection…`: opens the import dialog over the open workspace.
pub(crate) const IMPORT_POSTMAN: &str = "Import Postman collection…";

/// `Close workspace`.
pub(crate) const CLOSE_WORKSPACE: &str = "Close workspace";

/// `Workspace settings…`: opens the workspace settings page.
pub(crate) const WORKSPACE_SETTINGS: &str = "Workspace settings…";

/// The note at the foot of the menu.
pub(crate) const FOOTER_NOTE: &str = "One workspace is open at a time. Drafts stay with their \
                                      workspace and come back when you switch to it.";

/// `Retry` on a recent workspace that did not load.
const RETRY: &str = "Retry";

/// The focus of each recent row by its element id, made the first time the row draws, so a
/// row reached with Tab keeps its focus across frames and shows its `Remove`.
#[derive(Default)]
pub(super) struct RecentFocus(Cell<HashMap<String, FocusHandle>>);

impl RecentFocus {
    /// The focus of the row `id`, made on first use.
    fn handle(&self, id: &str, cx: &mut App) -> FocusHandle {
        let mut handles = self.0.take();
        let handle = handles
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        self.0.set(handles);
        handle
    }
}

/// What a menu or empty state action does.
#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    New,
    NewInFolder,
    Import,
    Settings,
    Close,
}

impl Action {
    fn run(self, shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
        shell.workspace_menu_open = false;
        match self {
            Self::Open => shell.pick_folder(window, cx),
            Self::New => shell.start_new_workspace(window, cx),
            Self::NewInFolder => shell.pick_new(window, cx),
            Self::Import => shell.open_import(window, cx),
            Self::Settings => {
                shell.open_settings(super::settings_page::Section::Globals, cx);
                shell.restore_focus(window, cx);
            }
            Self::Close => shell.switch_workspace(None, window, cx),
        }
        cx.notify();
    }
}

/// A clickable action row `id`, `bolt`'s menu row with `icon` and `label`.
pub(super) fn action_row(
    id: &str,
    icon_name: IconName,
    label: &'static str,
    action: Action,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    menu_row(
        id.to_owned(),
        &MenuItem::new(label).icon(ui::icon(icon_name)),
        false,
        cx,
    )
    .aria_label(label)
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
        Some(command) => row.child(
            Kbd::new(
                command
                    .key
                    .split('-')
                    .map(key_label)
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .id(gpui::ElementId::Name(format!("{id}-keycap").into())),
        ),
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
    failable_recent_row(
        prefix,
        index,
        recent,
        detail,
        current,
        None,
        Reveal::Always,
        cx,
    )
}

/// When a recent row shows its `Remove`.
pub(super) enum Reveal {
    /// Always: the empty state, which has room for it.
    Always,
    /// While the row is hovered or holds the keyboard focus (the workspace menu, as
    /// workspace-menu/01 draws it); `focused` is whether `focus` holds it now.
    OnHover { focus: FocusHandle, focused: bool },
}

/// [`recent_row`], or with `failure`, the error of its last open: the alert icon
/// `<prefix>-recent-failed-<n>`, the error in place of the detail as `<prefix>-error` and
/// `Retry`, `<prefix>-recent-retry-<n>`, over a danger wash; a row that did not load shows
/// its `Remove` whatever `reveal` says.
#[expect(
    clippy::too_many_arguments,
    reason = "one row of the menu, its parts passed as the menu reads them"
)]
fn failable_recent_row(
    prefix: &'static str,
    index: usize,
    recent: &RecentWorkspace,
    detail: String,
    current: bool,
    failure: Option<&str>,
    reveal: Reveal,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let n = index + 1;
    let label = format!("{} {}", recent.name, failure.unwrap_or(&detail));
    let lead = match failure {
        Some(_) => part(format!("{prefix}-recent-failed-{n}")).child(
            Icon::new(ui::icon(IconName::TriangleAlert))
                .size(MenuMetrics::ICON)
                .color(colors.danger),
        ),
        None => {
            let name = if detail == IN_REQFLUX {
                IconName::LayoutGrid
            } else {
                IconName::Folder
            };
            let color = if current { tint } else { colors.muted };
            part(format!("{prefix}-recent-icon-{n}")).child(
                Icon::new(ui::icon(name))
                    .size(MenuMetrics::ICON)
                    .color(color),
            )
        }
    };
    let detail_line = match failure {
        Some(error) => ui::role(
            part(format!("{prefix}-error")).role(Role::Label),
            TextVariant::Caption,
            cx,
        )
        .text_size(MenuMetrics::DETAIL_FONT)
        .aria_label(error.to_owned())
        .text_color(colors.danger)
        .child(error.to_owned()),
        None => ui::role(
            part(format!("{prefix}-recent-detail-{n}")),
            TextVariant::Caption,
            cx,
        )
        .text_size(MenuMetrics::DETAIL_FONT)
        .child(detail),
    };
    let open = ui::role(
        labeled_row(format!("{prefix}-recent-{n}"), label),
        TextVariant::Body,
        cx,
    )
    .text_size(MenuMetrics::ROW_FONT)
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
    let remove = Button::new(format!("{prefix}-recent-remove-{n}"), "Remove")
        .ghost()
        .inline()
        .icon(ui::icon(IconName::Close))
        .icon_only()
        .on_click(cx.listener(move |this, _event, _window, cx| this.remove_recent(index, cx)));
    let group = format!("{prefix}-recent-row-{n}");
    let hidden = failure.is_none() && matches!(reveal, Reveal::OnHover { focused: false, .. });
    let remove = div()
        .flex_none()
        .when(hidden, |slot| {
            slot.invisible()
                .group_hover(group.clone(), |style| style.visible())
        })
        .child(remove);
    let row = ListRow::new(group.clone())
        .selected(current)
        .selection_bar(true)
        .build(cx);
    let row = match &reveal {
        Reveal::Always => row,
        Reveal::OnHover { focus, .. } => row.track_focus(focus),
    };
    row.group(group)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(MenuMetrics::ROW_GAP)
        .h(MenuMetrics::DETAIL_ROW_HEIGHT)
        .pl(MenuMetrics::ROW_PADDING_X)
        .pr(Spacing::SM)
        .when(failure.is_some(), |row| {
            row.bg(colors.danger.opacity(ShellMetrics::FAILURE_ALPHA))
        })
        .child(lead)
        .child(open)
        .when(current, |row| {
            row.child(
                part(format!("{prefix}-recent-current-{n}")).child(
                    Icon::new(ui::icon(IconName::Check))
                        .size(MenuMetrics::ICON)
                        .color(tint),
                ),
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
fn retry_button(id: &str, index: usize, cx: &mut Context<Shell>) -> Button {
    Button::new(id.to_owned(), RETRY)
        .secondary()
        .inline()
        .icon(ui::icon(IconName::RefreshCw))
        .on_click(cx.listener(move |this, _event, window, cx| this.open_recent(index, window, cx)))
}

/// Whether `recent` is the workspace at `current`.
pub(super) fn is_current(recent: &RecentWorkspace, current: Option<&Path>) -> bool {
    current == Some(recent.path.as_path())
}

/// The error of the last workspace that did not open, as `workspace-error`.
pub(super) fn error_label(error: &str, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    ui::role(text_region("workspace-error"), TextVariant::BodySm, cx)
        .aria_label(error.to_owned())
        .pl(MenuMetrics::HEADER_PADDING_X)
        .pr(MenuMetrics::HEADER_PADDING_X)
        .py(Spacing::XS)
        .text_color(colors.danger)
        .child(error.to_owned())
}

/// The header, `workspace-menu-header`, in capitals, with the legend of the row icons,
/// `workspace-legend`, when there are recents: `bolt`'s menu header geometry.
fn header(legend: bool, cx: &App) -> impl IntoElement {
    let muted = bolt::colors(cx).muted;
    let title = ui::role(
        labeled_row("workspace-menu-header".to_owned(), HEADER.to_owned()),
        TextVariant::Label,
        cx,
    )
    .child(HEADER.to_uppercase());
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(MenuMetrics::HEADER_GAP)
        .pt(MenuMetrics::HEADER_PADDING_TOP)
        .pb(MenuMetrics::HEADER_PADDING_BOTTOM)
        .pl(MenuMetrics::HEADER_PADDING_X)
        .pr(MenuMetrics::HEADER_PADDING_X)
        .child(title)
        .child(div().flex_1())
        .when(legend, |row| {
            row.child(
                ui::role(
                    labeled_row(
                        "workspace-legend".to_owned(),
                        format!("folder · {}", IN_REQFLUX.to_lowercase()),
                    ),
                    TextVariant::Caption,
                    cx,
                )
                .text_size(MenuMetrics::HEADER_FONT)
                .flex()
                .items_center()
                .gap(Spacing::XS)
                .child(
                    Icon::new(ui::icon(IconName::Folder))
                        .size(MenuMetrics::HEADER_ICON)
                        .color(muted),
                )
                .child("folder ·")
                .child(
                    Icon::new(ui::icon(IconName::LayoutGrid))
                        .size(MenuMetrics::HEADER_ICON)
                        .color(muted),
                )
                .child(IN_REQFLUX.to_lowercase()),
            )
        })
}

/// A line between groups of the menu.
fn separator(cx: &App) -> Div {
    render_separator(cx)
}

/// The open menu, drawn above the islands and anchored under the selector, over a backdrop
/// the size of the window that closes it on a click outside.
pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let mut menu = menu_frame(cx)
        .id("workspace-menu")
        .debug_selector(|| "workspace-menu".to_owned())
        .role(Role::Menu)
        .track_focus(&shell.workspace_menu_focus)
        .occlude()
        .w(tokens::WORKSPACE_MENU_WIDTH)
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
        let focus = shell
            .recent_focus
            .handle(&format!("workspace-recent-row-{}", index + 1), cx);
        let focused = focus.contains_focused(window, cx);
        menu = menu.child(failable_recent_row(
            "workspace",
            index,
            recent,
            detail,
            selected,
            failure,
            Reveal::OnHover { focus, focused },
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
        menu = menu.child(menu_action(
            "workspace-import",
            IconName::Import,
            IMPORT_POSTMAN,
            Action::Import,
            None,
            cx,
        ));
        menu = menu.child(separator(cx)).child(menu_action(
            "workspace-settings",
            IconName::Settings,
            WORKSPACE_SETTINGS,
            Action::Settings,
            Some(CommandId::WorkspaceSettings),
            cx,
        ));
        menu = menu.child(menu_action(
            "workspace-close",
            IconName::LogOut,
            CLOSE_WORKSPACE,
            Action::Close,
            None,
            cx,
        ));
    }
    menu = menu.child(
        ui::role(
            labeled_row("workspace-menu-note".to_owned(), FOOTER_NOTE.to_owned()),
            TextVariant::Caption,
            cx,
        )
        .text_size(MenuMetrics::DETAIL_FONT)
        .pl(MenuMetrics::HEADER_PADDING_X)
        .pr(MenuMetrics::HEADER_PADDING_X)
        .py(Spacing::XS)
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
        .child(
            deferred(
                anchored().child(
                    div()
                        .flex()
                        .items_end()
                        .gap(Spacing::XS)
                        .child(menu)
                        .children(shell.new_workspace_popover(cx)),
                ),
            )
            .with_priority(2),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "workspace_menu_tests.rs"]
mod tests;
