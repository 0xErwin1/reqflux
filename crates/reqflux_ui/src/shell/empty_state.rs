//! The document island without a workspace (recent-workspaces S5): `Open folder…`
//! (`empty-open`), `New workspace…` (`empty-new`, which opens the workspace menu's name
//! field), `New workspace in folder…` (`empty-new-folder`) and the recent workspaces
//! (`empty-recent-<n>`), over the reason there is none, `document-message`, which keeps the
//! `reqflux <directory>` hint as secondary text.

use bolt::primitives::TextVariant;
use bolt::tokens::{ShellMetrics, Spacing};
use gpui::{Context, IntoElement, ParentElement, Styled, div};
use gpui_kit_assets::IconName;

use super::request::ui;
use super::workspace_menu::{
    Action, NEW_WORKSPACE, NEW_WORKSPACE_IN_FOLDER, OPEN_FOLDER, action_row, error_label,
    is_current, recent_row,
};
use super::{Shell, region, text_region};

pub(super) fn render(shell: &Shell, message: String, cx: &mut Context<Shell>) -> impl IntoElement {
    let current = shell.workspace_path.as_deref();
    let mut column = region("empty-state")
        .w(crate::tokens::EMPTY_STATE_WIDTH)
        .flex()
        .flex_col()
        .gap(Spacing::SM)
        .child(action_row(
            "empty-open",
            IconName::Folder,
            OPEN_FOLDER,
            Action::Open,
            cx,
        ))
        .child(action_row(
            "empty-new",
            IconName::Plus,
            NEW_WORKSPACE,
            Action::New,
            cx,
        ))
        .child(action_row(
            "empty-new-folder",
            IconName::FolderPlus,
            NEW_WORKSPACE_IN_FOLDER,
            Action::NewInFolder,
            cx,
        ));
    // The menu shows the error while open; one place at a time keeps its id unique.
    if let (Some(error), false) = (&shell.workspace_error, shell.workspace_menu_open) {
        column = column.child(error_label(error, cx));
    }
    if !shell.recents.is_empty() {
        let mut recents = div().flex().flex_col().child(
            ui::role(div(), TextVariant::Label, cx)
                .text_size(ShellMetrics::SECTION_LABEL_FONT)
                .pl(ShellMetrics::CARD_PADDING_X)
                .pr(ShellMetrics::CARD_PADDING_X)
                .pt(ShellMetrics::CARD_LABEL_PADDING_TOP)
                .pb(ShellMetrics::CARD_LABEL_PADDING_BOTTOM)
                .child("RECENT"),
        );
        for (index, recent) in shell.recents.iter().enumerate() {
            let selected = is_current(recent, current);
            let detail = shell.recent_detail(recent);
            recents = recents.child(recent_row("empty", index, recent, detail, selected, cx));
        }
        column = column.child(recents);
    }
    column.child(
        ui::role(text_region("document-message"), TextVariant::BodySm, cx)
            .pl(ShellMetrics::CARD_PADDING_X)
            .pr(ShellMetrics::CARD_PADDING_X)
            .child(message),
    )
}
