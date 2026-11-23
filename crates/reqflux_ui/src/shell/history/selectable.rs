//! The selectable recorded values of the history tab (history-copy-and-scratch S1): the URL,
//! each key and value of the Params and Headers grids and the Body are read-only text
//! fields, so they can be focused, selected with the mouse or the keyboard and copied, and
//! nothing typed into them changes the record.
//!
//! A single-line value is gpui-component's input with `readonly(true)`, which keeps focus,
//! selection and copy (`disabled` would drop them); the body is the read-only code viewer.
//! The fields need the window to exist, so the shell builds them before it draws
//! ([`Shell::sync_recorded_text`]) and rebuilds the grid cells only when the entry or the
//! selected view changes, which keeps a selection across frames.

use bolt::controls::{GpuiInput, InputState};
use gpui::{App, AppContext as _, Context, Entity, Styled, Window};
use reqflux_engine::SentBody;

use super::view::params;
use super::{HistoryTab, RecordedTab};
use crate::shell::Shell;
use crate::shell::code_view::CodeView;

/// The read-only fields of the open history tab.
pub(crate) struct RecordedText {
    /// The history entry they show.
    entry: i64,
    /// The view the grid cells were built for.
    tab: RecordedTab,
    url: Entity<InputState>,
    /// The key and value fields of each grid row, top first.
    cells: Vec<[Entity<InputState>; 2]>,
    body: CodeView,
}

impl RecordedText {
    /// Whether they show the history entry `entry`.
    pub(crate) fn shows(&self, entry: i64) -> bool {
        self.entry == entry
    }

    /// The URL field.
    pub(crate) fn url(&self) -> &Entity<InputState> {
        &self.url
    }

    /// The key (`0`) or value (`1`) field of the 0-based grid row `row`.
    pub(crate) fn cell(&self, row: usize, column: usize) -> Option<&Entity<InputState>> {
        self.cells.get(row).and_then(|cells| cells.get(column))
    }

    /// The body viewer.
    pub(crate) fn body(&self) -> &CodeView {
        &self.body
    }
}

/// A read-only field holding `text`.
fn field(text: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let text = text.to_owned();
    cx.new(|cx| InputState::new(window, cx).default_value(text))
}

/// The grid rows of the view `tab` of `history`; empty for the Body.
fn grid_rows(history: &HistoryTab) -> Vec<(String, String)> {
    match history.tab {
        RecordedTab::Params => params(&history.sent.url),
        RecordedTab::Headers => history.headers().to_vec(),
        RecordedTab::Body => Vec::new(),
    }
}

fn cells_of(
    history: &HistoryTab,
    window: &mut Window,
    cx: &mut App,
) -> Vec<[Entity<InputState>; 2]> {
    grid_rows(history)
        .iter()
        .map(|(key, value)| [field(key, window, cx), field(value, window, cx)])
        .collect()
}

/// The read-only single-line field drawing `state`, in the face and size its caller sets.
pub(crate) fn input(state: &Entity<InputState>) -> GpuiInput {
    GpuiInput::new(state)
        .readonly(true)
        .appearance(false)
        .bordered(false)
        .p_0()
        .h_full()
        .flex_1()
        .min_w_0()
}

impl Shell {
    /// Builds the read-only fields of the history tab when its entry changed, and its grid
    /// cells when the selected view changed; drops them without a history tab.
    pub(in crate::shell) fn sync_recorded_text(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(history) = self.orphan.as_ref() else {
            self.recorded_text = None;
            return;
        };
        match self.recorded_text.as_mut() {
            Some(text) if text.entry == history.entry => {
                if text.tab != history.tab {
                    text.cells = cells_of(history, window, cx);
                    text.tab = history.tab;
                }
            }
            _ => {
                let mut body = CodeView::new(window, cx);
                if let SentBody::Text(text) = &history.sent.body {
                    body.show(text, None, window, cx);
                }
                self.recorded_text = Some(RecordedText {
                    entry: history.entry,
                    tab: history.tab,
                    url: field(&history.sent.url, window, cx),
                    cells: cells_of(history, window, cx),
                    body,
                });
            }
        }
    }
}
