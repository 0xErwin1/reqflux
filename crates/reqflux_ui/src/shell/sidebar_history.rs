//! The History panel of the sidebar island: the recorded sends of the workspace, newest
//! first, shown in place of the collection tree while the rail's History view is active.
//!
//! The sidebar holds only the row texts the shell hands it through
//! [`Sidebar::set_history`]; a click on a row emits [`OpenHistory`] and the shell, which owns
//! the app state, reopens the entry.

use bolt::tokens::{chrome, space, text};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, prelude::*};

use super::rows::labeled_row;
use super::sidebar::Sidebar;
use super::{region, text as text_role, text_region};
use crate::tokens;

/// The text of the History panel without entries.
pub(crate) const EMPTY: &str = "No requests sent yet";

/// What the sidebar island shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Panel {
    /// The collection tree.
    #[default]
    Collections,
    /// The History panel.
    History,
    /// The Variables panel.
    Variables,
    /// The MCP approvals view, which the shell shows in the document island while the
    /// sidebar keeps the collection tree.
    Approvals,
}

/// One row of the History panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryRow {
    /// The id of the history entry.
    pub(crate) entry: i64,
    /// `METHOD path · status · time ago`, or `failed` in place of the status.
    pub(crate) text: String,
}

/// Emitted when the user opens the history entry with this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenHistory(pub(crate) i64);

impl Sidebar {
    /// Shows `panel` in the island.
    pub(crate) fn set_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.panel = panel;
        cx.notify();
    }

    /// Replaces the rows of the History panel, newest first.
    pub(crate) fn set_history(&mut self, rows: Vec<HistoryRow>, cx: &mut Context<Self>) {
        self.history = rows;
        cx.notify();
    }

    /// Whether the History panel is shown instead of the tree.
    #[cfg(test)]
    pub(crate) fn showing_history(&self) -> bool {
        self.panel == Panel::History
    }

    /// The texts of the History panel rows, top first.
    #[cfg(test)]
    pub(crate) fn history_texts(&self) -> Vec<String> {
        self.history.iter().map(|row| row.text.clone()).collect()
    }

    /// The History panel: the `HISTORY` header, then the rows `history-row-<n>` from 1 at the
    /// top, or the empty state `history-empty`.
    pub(super) fn history_panel(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let header = region("history-header")
            .flex_none()
            .h(tokens::SIDEBAR_HEADER)
            .flex()
            .items_center()
            .pl(space::LG)
            .pr(space::MD)
            .child(text_role::role(text_region("history-label"), text::LABEL, cx).child("HISTORY"));
        let body = if self.history.is_empty() {
            text_role::role(text_region("history-empty"), text::CAPTION, cx)
                .aria_label(EMPTY)
                .flex_1()
                .pl(space::LG)
                .pr(space::LG)
                .child(EMPTY)
                .into_any_element()
        } else {
            let hover = bolt::colors(cx).hover;
            let rows = self.history.iter().enumerate().map(|(index, row)| {
                let entry = row.entry;
                text_role::role(
                    labeled_row(format!("history-row-{}", index + 1), row.text.clone()),
                    text::BODY,
                    cx,
                )
                .flex_none()
                .h(chrome::TREE_ROW)
                .flex()
                .items_center()
                .pl(space::LG)
                .pr(space::LG)
                .whitespace_nowrap()
                .overflow_hidden()
                .hover(|style| style.bg(hover))
                .child(row.text.clone())
                .on_click(cx.listener(move |_, _event, _, cx| cx.emit(OpenHistory(entry))))
            });
            region("history-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(rows)
                .into_any_element()
        };
        vec![header.into_any_element(), body]
    }
}
