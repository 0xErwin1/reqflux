//! The History panel of the sidebar island (design/history/01-sidebar-readonly-tab): the
//! recorded sends of the workspace, newest first, grouped by local day under a filter field,
//! shown in place of the collection tree while the rail's History view is active.
//!
//! The sidebar holds only the rows the shell hands it through [`Sidebar::set_history`]; a
//! click on a row emits [`OpenHistory`] and the shell, which owns the app state, opens the
//! entry's history tab. The filter narrows the rows already loaded, by name, URL or status;
//! its text field lives in the window's element state under [`FILTER_KEY`], so it lasts while
//! the panel is on screen.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::Keycap;
use bolt_legacy::tokens::{Colors, border, chrome, cut, family, field, icon, space, text};
use bolt_legacy::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement, RenderOnce,
    Role, Styled, Window, div, prelude::*,
};
use gpui_component::input::{Input, InputState};

use super::request::StatusClass;
use super::rows::{labeled_row, part};
use super::sidebar::Sidebar;
use super::{region, text as text_role, text_region};
use crate::tokens::{self, method_color};

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
            .h(tokens::SIDEBAR_HEADER)
            .flex()
            .items_center()
            .pl(space::LG)
            .pr(space::MD)
            .child(text_role::role(text_region("history-label"), text::LABEL, cx).child("HISTORY"));
        if self.history.is_empty() {
            return vec![header.into_any_element(), empty(cx).into_any_element()];
        }
        let footer = text_role::role(text_region("history-footer"), text::CAPTION, cx)
            .flex_none()
            .h(tokens::SIDEBAR_FOOTER)
            .flex()
            .items_center()
            .pl(space::LG)
            .pr(space::LG)
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

/// The empty state: the History icon on a raised square, the title, the explanation and the
/// send hint with its keycap.
fn empty(cx: &App) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    region("history-empty")
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(space::MD)
        .pl(space::XL)
        .pr(space::XL)
        .child(
            div()
                .relative()
                .size(chrome::CONTROL_LARGE)
                .flex()
                .items_center()
                .justify_center()
                .child(Chamfer::new(cut::LARGE_CONTROL).fill(colors.raised))
                .child(
                    Icon::new(IconName::RotateCcwClock)
                        .size(tokens::HISTORY_EMPTY_ICON)
                        .color(colors.muted),
                ),
        )
        .child(
            text_role::role(text_region("history-empty-title"), text::BODY, cx)
                .aria_label(EMPTY)
                .text_color(colors.strong)
                .font_weight(FontWeight::SEMIBOLD)
                .child(EMPTY),
        )
        .child(
            text_role::role(text_region("history-empty-text"), text::CAPTION, cx)
                .aria_label(EMPTY_TEXT)
                .text_center()
                .child(EMPTY_TEXT),
        )
        .child(
            text_role::role(text_region("history-empty-hint"), text::CAPTION, cx)
                .aria_label(EMPTY_HINT)
                .flex()
                .items_center()
                .gap(space::HALF)
                .child(EMPTY_HINT)
                .child(Keycap::new("history-empty-hint-keycap", "Ctrl ↵")),
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
                text_role::role(text_region("history-no-match"), text::CAPTION, cx)
                    .aria_label(NO_MATCH)
                    .pl(space::LG)
                    .pr(space::LG)
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

/// The filter field, `history-filter`: the search icon and the text field on the field well.
fn filter(input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let colors = bolt_legacy::colors(cx);
    div()
        .flex_none()
        .pl(space::MD)
        .pr(space::MD)
        .pb(tokens::SIDEBAR_FILTER_BOTTOM)
        .child(
            region("history-filter")
                .role(Role::TextInput)
                .aria_label(FILTER_PLACEHOLDER)
                .relative()
                .flex()
                .items_center()
                .gap(field::GAP)
                .h(chrome::CONTROL)
                .pl(field::PADDING_X)
                .pr(field::PADDING_X)
                .font_family(family::INTERFACE)
                .text_size(field::FONT)
                .child(
                    Chamfer::new(cut::CONTROL)
                        .fill(colors.background)
                        .edge(colors.line, border::THIN),
                )
                .child(
                    Icon::new(IconName::Search)
                        .size(icon::MD)
                        .color(colors.muted),
                )
                .child(
                    Input::new(input)
                        .appearance(false)
                        .bordered(false)
                        .flex_1()
                        .min_w_0()
                        .text_size(field::FONT)
                        .text_color(colors.foreground),
                ),
        )
}

/// The label of the `n`th day group, `history-group-<n>`, with its count,
/// `history-group-<n>-count`.
fn group_label(n: usize, group: &Group<'_>, cx: &App) -> impl IntoElement {
    let count = group.rows.len().to_string();
    text_role::role(
        labeled_row(format!("history-group-{n}"), group.day.to_owned()),
        text::LABEL,
        cx,
    )
    .flex_none()
    .h(tokens::HISTORY_GROUP_ROW)
    .flex()
    .items_center()
    .pl(space::LG)
    .pr(space::LG)
    .child(group.day.to_owned())
    .child(div().flex_1())
    .child(
        part(format!("history-group-{n}-count"))
            .role(Role::Label)
            .aria_label(count.clone())
            .font_family(family::MONO)
            .text_size(tokens::TREE_COUNT_SIZE)
            .font_weight(FontWeight::NORMAL)
            .child(count),
    )
}

/// The `n`th row, `history-row-<n>`: the method `-method`, the name `-name` (struck through
/// with `-deleted` when the request no longer exists), the time `-time` and the status
/// `-status` in its class color.
fn history_row(n: usize, row: &HistoryRow, sidebar: &Entity<Sidebar>, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    let id = format!("history-row-{n}");
    let status = row.status_text();
    let status_color = row.status.map_or(colors.danger, |code| {
        class_color(StatusClass::of(code), colors)
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
    let hover = colors.hover;
    let selected = bolt_legacy::accent_washes(cx).selected_item;
    text_role::role(
        labeled_row(id.clone(), format!("{} · {}", row.text(), row.time)),
        text::BODY,
        cx,
    )
    .flex_none()
    .h(chrome::TREE_ROW)
    .flex()
    .items_center()
    .gap(tokens::TREE_ROW_GAP)
    .pl(space::LG)
    .pr(space::LG)
    .whitespace_nowrap()
    .cursor_pointer()
    .when(row.selected, |element| element.bg(selected))
    .hover(|style| style.bg(hover))
    .child(
        part(format!("{id}-method"))
            .role(Role::Label)
            .aria_label(row.method.clone())
            .flex_none()
            .w(tokens::TREE_METHOD_COLUMN)
            .font_family(family::MONO)
            .text_size(tokens::METHOD_LABEL_SIZE)
            .font_weight(FontWeight::BOLD)
            .text_color(method_color(&row.method, bolt_legacy::palette(cx)))
            .child(row.method.clone()),
    )
    .child(name)
    .child(
        part(format!("{id}-time"))
            .role(Role::Label)
            .aria_label(row.time.clone())
            .flex_none()
            .font_family(family::MONO)
            .text_size(tokens::TREE_COUNT_SIZE)
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
            .font_family(family::MONO)
            .text_size(tokens::TREE_COUNT_SIZE)
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
