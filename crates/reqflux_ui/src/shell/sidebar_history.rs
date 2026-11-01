//! The History panel of the sidebar island (design/history/01-sidebar-readonly-tab): the
//! recorded sends of the workspace, newest first, grouped by local day under a filter field,
//! shown in place of the collection tree while the rail's History view is active.
//!
//! The sidebar holds only the rows the shell hands it through [`Sidebar::set_history`]; a
//! click on a row emits [`OpenHistory`] and the shell, which owns the app state, opens the
//! entry's history tab. The filter narrows the rows already loaded, by name, URL or status;
//! its text field lives in the window's element state under [`FILTER_KEY`], so it lasts while
//! the panel is on screen.
//!
//! It draws with DBFlux's `bolt` as the collection tree does: the sidebar's `ShellMetrics`
//! header (46), filter and footer (46), rows as `ListRow` at `TreeMetrics` (26), and
//! `EmptyState` for the panel without sends.

use bolt::Colors;
use bolt::composites::{EmptyState, EmptyStateAction, ListRow};
use bolt::controls::{Input, InputState};
use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{HeaderMetrics, ShellMetrics, Spacing, TreeMetrics};
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement, RenderOnce,
    Role, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::request::{StatusClass, ui};
use super::rows::{labeled_row, part};
use super::sidebar::Sidebar;
use super::{region, text_region};
use crate::tokens::{self, method_hue};

/// The title of the History panel without entries.
pub(crate) const EMPTY: &str = "No requests sent yet";
/// The explanation under [`EMPTY`].
pub(crate) const EMPTY_TEXT: &str =
    "Every send is recorded here with its response, grouped by day. Secrets are never stored.";
/// The hint under [`EMPTY_TEXT`], beside the Ctrl Enter keycap.
pub(crate) const EMPTY_HINT: &str = "Send the open request";
/// The placeholder of the filter field.
pub(crate) const FILTER_PLACEHOLDER: &str = "Filter by name, URL or status";
/// Shown when the filter matches no row.
pub(crate) const NO_MATCH: &str = "No sends match the filter";
/// The element state key of the filter's text field.
const FILTER_KEY: &str = "history-filter-state";

/// What the sidebar island shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Panel {
    /// The collection tree.
    #[default]
    Collections,
    /// The History panel.
    History,
    /// The workspace settings page, which the shell shows in place of the sidebar and the
    /// document islands.
    Environments,
    /// The MCP approvals view, which the shell shows in the document island while the
    /// sidebar keeps the collection tree.
    Approvals,
}

/// One row of the History panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryRow {
    /// The id of the history entry.
    pub(crate) entry: i64,
    /// The uppercase HTTP method as sent.
    pub(crate) method: String,
    /// The request's name when it was sent.
    pub(crate) name: String,
    /// The URL as sent, secrets masked.
    pub(crate) url: String,
    /// The response status; `None` when the send failed.
    pub(crate) status: Option<u16>,
    /// The local day group: `Today`, `Yesterday` or a date.
    pub(crate) day: String,
    /// The local time of day, `HH:MM`.
    pub(crate) time: String,
    /// Whether the request no longer exists: its name is struck through.
    pub(crate) deleted: bool,
    /// Whether the entry is the one open in the history tab.
    pub(crate) selected: bool,
}

impl HistoryRow {
    /// The status code, or `failed`.
    pub(crate) fn status_text(&self) -> String {
        self.status
            .map_or_else(|| "failed".to_owned(), |status| status.to_string())
    }

    /// `METHOD name · status`.
    pub(crate) fn text(&self) -> String {
        format!("{} {} · {}", self.method, self.name, self.status_text())
    }

    /// Whether the row matches `query`: its name or URL contains it, ignoring case, or its
    /// status starts with it. An empty query matches every row.
    pub(crate) fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self.url.to_lowercase().contains(&query)
            || self.status_text().starts_with(&query)
    }
}

/// A day of the History panel: its label and its rows matching the filter, top first.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Group<'a> {
    pub(crate) day: &'a str,
    pub(crate) rows: Vec<&'a HistoryRow>,
}

/// The rows matching `query`, in order, grouped by consecutive day.
pub(crate) fn groups<'a>(rows: &'a [HistoryRow], query: &str) -> Vec<Group<'a>> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    for row in rows.iter().filter(|row| row.matches(query)) {
        match groups.last_mut() {
            Some(group) if group.day == row.day => group.rows.push(row),
            _ => groups.push(Group {
                day: &row.day,
                rows: vec![row],
            }),
        }
    }
    groups
}

/// `<n> sends`, or `1 send`.
fn sends_text(count: usize) -> String {
    if count == 1 {
        "1 send".to_owned()
    } else {
        format!("{count} sends")
    }
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

    /// The texts of the History panel rows, top first, unfiltered.
    #[cfg(test)]
    pub(crate) fn history_texts(&self) -> Vec<String> {
        self.history.iter().map(HistoryRow::text).collect()
    }

    /// The History panel: the `HISTORY` header, then the filter `history-filter`, the day
    /// groups `history-group-<n>` and the rows `history-row-<n>` from 1 at the top, and the
    /// footer `history-footer`; or the empty state `history-empty`.
    pub(super) fn history_panel(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let header = region("history-header")
            .flex_none()
            .h(ShellMetrics::SIDEBAR_HEADER_HEIGHT)
            .flex()
            .items_center()
            .gap(HeaderMetrics::PANEL_GAP)
            .pl(HeaderMetrics::PANEL_PADDING_LEFT)
            .pr(HeaderMetrics::PANEL_PADDING_RIGHT)
            .child(
                ui::role(text_region("history-label"), TextVariant::Label, cx)
                    .text_size(ShellMetrics::SECTION_LABEL_FONT)
                    .child("HISTORY"),
            );
        if self.history.is_empty() {
            return vec![header.into_any_element(), empty().into_any_element()];
        }
        let footer = text_region("history-footer")
            .flex_none()
            .h(ShellMetrics::SIDEBAR_FOOTER_HEIGHT)
            .flex()
            .items_center()
            .pl(ShellMetrics::SIDEBAR_FOOTER_PADDING_X)
            .pr(ShellMetrics::SIDEBAR_FOOTER_PADDING_X)
            .font_family(bolt::fonts::ui_family(cx))
            .text_size(ShellMetrics::SIDEBAR_FOOTER_FONT)
            .text_color(bolt::colors(cx).muted)
            .whitespace_nowrap()
            .child(sends_text(self.history.len()));
        vec![
            header.into_any_element(),
            HistoryList {
                rows: self.history.clone(),
                sidebar: cx.entity(),
            }
            .into_any_element(),
            footer.into_any_element(),
        ]
    }
}

/// The empty state `history-empty`, `bolt`'s `EmptyState` with the History icon, the title
/// `history-empty-title` and the explanation `history-empty-text`, with the send hint as its
/// action, `history-empty-action-0`, and its keycap `history-empty-key-0`.
fn empty() -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .pl(Spacing::XL)
        .pr(Spacing::XL)
        .child(
            div().flex_none().child(
                EmptyState::new(ui::icon(IconName::RotateCcwClock), EMPTY_TEXT)
                    .title(EMPTY)
                    .action(EmptyStateAction::new(
                        ui::icon(IconName::Send),
                        EMPTY_HINT,
                        ["Ctrl ↵"],
                    ))
                    .id("history-empty"),
            ),
        )
}

/// The filter and the grouped rows. A [`RenderOnce`] so it reaches the window's element
/// state, where the filter's text field lives.
#[derive(IntoElement)]
struct HistoryList {
    rows: Vec<HistoryRow>,
    sidebar: Entity<Sidebar>,
}

impl RenderOnce for HistoryList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = window.use_keyed_state(FILTER_KEY, cx, |window, cx| {
            InputState::new(window, cx).placeholder(FILTER_PLACEHOLDER)
        });
        let query = input.read(cx).value().to_string();
        let groups = groups(&self.rows, &query);
        let mut list = region("history-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if groups.is_empty() {
            list = list.child(
                ui::role(text_region("history-no-match"), TextVariant::Caption, cx)
                    .aria_label(NO_MATCH)
                    .pl(HeaderMetrics::PANEL_PADDING_LEFT)
                    .pr(HeaderMetrics::PANEL_PADDING_LEFT)
                    .child(NO_MATCH),
            );
        }
        let mut n = 0;
        for (index, group) in groups.iter().enumerate() {
            list = list.child(group_label(index + 1, group, cx));
            for row in &group.rows {
                n += 1;
                list = list.child(history_row(n, row, &self.sidebar, cx));
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(filter(&input, cx))
            .child(list)
    }
}

/// The filter field, `history-filter`: `bolt`'s field with the search icon before the text.
fn filter(input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    div()
        .flex_none()
        .pl(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
        .pr(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
        .pb(ShellMetrics::SIDEBAR_FILTER_PADDING_BOTTOM)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            Input::new(input)
                .id("history-filter")
                .aria_label(FILTER_PLACEHOLDER)
                .prefix(
                    Icon::new(ui::icon(IconName::Search))
                        .size(ShellMetrics::SIDEBAR_FILTER_ICON)
                        .color(colors.muted),
                ),
        )
}

/// The label of the `n`th day group, `history-group-<n>`, with its count,
/// `history-group-<n>-count`.
fn group_label(n: usize, group: &Group<'_>, cx: &App) -> impl IntoElement {
    let count = group.rows.len().to_string();
    ui::role(
        labeled_row(format!("history-group-{n}"), group.day.to_owned()),
        TextVariant::Label,
        cx,
    )
    .text_size(ShellMetrics::SECTION_LABEL_FONT)
    .flex_none()
    .flex()
    .items_center()
    .pl(HeaderMetrics::PANEL_PADDING_LEFT)
    .pr(HeaderMetrics::PANEL_PADDING_LEFT)
    .pt(HeaderMetrics::LABEL_PADDING_TOP)
    .pb(HeaderMetrics::LABEL_PADDING_BOTTOM)
    .child(group.day.to_owned())
    .child(div().flex_1())
    .child(
        part(format!("history-group-{n}-count"))
            .role(Role::Label)
            .aria_label(count.clone())
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(TreeMetrics::META_FONT)
            .font_weight(FontWeight::NORMAL)
            .child(count),
    )
}

/// The `n`th row, `history-row-<n>`: the method `-method`, the name `-name` (struck through
/// with `-deleted` when the request no longer exists), the time `-time` and the status
/// `-status` in its class color.
fn history_row(n: usize, row: &HistoryRow, sidebar: &Entity<Sidebar>, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("history-row-{n}");
    let status = row.status_text();
    let status_color = row.status.map_or(colors.danger, |code| {
        class_color(StatusClass::of(code), &colors)
    });
    let name = part(format!("{id}-name"))
        .role(Role::Label)
        .aria_label(row.name.clone())
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .when(row.deleted, |name| {
            name.line_through().text_color(colors.muted).child(
                part(format!("{id}-deleted"))
                    .role(Role::Label)
                    .aria_label("Request deleted"),
            )
        })
        .child(row.name.clone());
    let entry = row.entry;
    let sidebar = sidebar.clone();
    let mono = bolt::fonts::editor_family(cx);
    let element = ListRow::new(id.clone())
        .selected(row.selected)
        .selection_bar(true)
        .build(cx)
        .role(Role::Label)
        .aria_label(format!("{} · {}", row.text(), row.time))
        .aria_selected(row.selected);
    ui::role(element, TextVariant::Body, cx)
        .flex_none()
        .h(TreeMetrics::ROW_HEIGHT)
        .flex()
        .items_center()
        .gap(TreeMetrics::GAP)
        .pl(TreeMetrics::PADDING_X)
        .pr(TreeMetrics::PADDING_X)
        .whitespace_nowrap()
        .child(
            part(format!("{id}-method"))
                .role(Role::Label)
                .aria_label(row.method.clone())
                .flex_none()
                .w(tokens::TREE_METHOD_COLUMN)
                .font_family(mono.clone())
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(&row.method, cx))
                .child(row.method.clone()),
        )
        .child(name)
        .child(
            part(format!("{id}-time"))
                .role(Role::Label)
                .aria_label(row.time.clone())
                .flex_none()
                .font_family(mono.clone())
                .text_size(TreeMetrics::META_FONT)
                .text_color(colors.muted)
                .child(row.time.clone()),
        )
        .child(
            part(format!("{id}-status"))
                .role(Role::Label)
                .aria_label(status.clone())
                .flex_none()
                .w(tokens::HISTORY_STATUS_COLUMN)
                .text_right()
                .font_family(mono)
                .text_size(TreeMetrics::META_FONT)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(status_color)
                .child(status),
        )
        .on_click(move |_event, _window, cx| {
            sidebar.update(cx, |_, cx| cx.emit(OpenHistory(entry)));
        })
        .into_any_element()
}

/// The color of a status class.
pub(super) fn class_color(class: StatusClass, colors: &Colors) -> Hsla {
    match class {
        StatusClass::Success => colors.success,
        StatusClass::Info => colors.info,
        StatusClass::Warning => colors.warning,
        StatusClass::Danger => colors.danger,
    }
}

#[cfg(test)]
#[path = "sidebar_history_tests.rs"]
mod tests;
