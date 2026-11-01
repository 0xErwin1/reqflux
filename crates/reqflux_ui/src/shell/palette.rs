//! The command palette (command-palette S1-S3, design-catch-up T9): a query field over the
//! window that finds requests, commands and variables. The scope tabs All, Requests `@`,
//! Commands `>` and Variables `$` follow the query prefix; Tab and Shift+Tab step through
//! them. Up and Down move the highlight, Enter or a click runs a row, Escape or a click
//! outside closes it and gives the focus back.

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};

use super::Shell;
use super::commands::command;
use super::palette_match::requests;
use super::sidebar_history::Panel;
use crate::workspace::OpenedWorkspace;
use model::{Listing, PaletteRow, Scope, listing, parse, variables};

/// The placeholder of the query field.
pub(crate) const PLACEHOLDER: &str = "Search requests or run a command";

/// The open palette: its query field and the highlighted row, 0-based.
pub(crate) struct Palette {
    pub(crate) input: Entity<InputState>,
    pub(crate) highlight: usize,
    /// Resets the highlight when the query changes.
    _change: Subscription,
}

impl Shell {
    /// Opens the palette with an empty, focused query; does nothing while it is open.
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        self.workspace_menu_open = false;
        self.environment_menu_open = false;
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        input.update(cx, |state, cx| state.focus(window, cx));
        let change = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(palette) = this.palette.as_mut()
            {
                palette.highlight = 0;
                cx.notify();
            }
        });
        self.palette = Some(Palette {
            input,
            highlight: 0,
            _change: change,
        });
        cx.notify();
    }

    /// Closes the palette and gives the focus back.
    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            self.restore_focus(window, cx);
        }
    }

    /// The palette's current query, prefix included.
    fn palette_query(&self, cx: &gpui::App) -> String {
        self.palette
            .as_ref()
            .map(|palette| palette.input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// What the palette lists for its current query.
    pub(crate) fn palette_listing(&self, cx: &gpui::App) -> Listing {
        let (requests, groups) = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => (
                requests(engine.workspace()),
                variables(engine.workspace(), self.active_environment.as_deref()),
            ),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => (Vec::new(), Vec::new()),
        };
        listing(&self.palette_query(cx), requests, groups)
    }

    /// The rows for the palette's current query, as the highlight counts them.
    pub(crate) fn palette_rows(&self, cx: &gpui::App) -> Vec<PaletteRow> {
        self.palette_listing(cx).rows()
    }

    /// Replaces the query with `query`, the caret at its end.
    fn set_palette_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(palette) = self.palette.as_ref() {
            palette.input.update(cx, |state, cx| {
                state.set_value(query, window, cx);
            });
        }
        if let Some(palette) = self.palette.as_mut() {
            palette.highlight = 0;
        }
        cx.notify();
    }

    /// Picks `scope`, keeping the text after the prefix.
    fn pick_scope(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.palette_query(cx);
        let (_, text) = parse(&query);
        let query = scope.query(text);
        self.set_palette_query(query, window, cx);
    }

    /// Tab and Shift+Tab: the next or the previous scope.
    fn step_scope(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.palette_query(cx);
        let (scope, _) = parse(&query);
        self.pick_scope(scope.step(forward), window, cx);
    }

    /// Moves the highlight by one row, down or up, staying on the list.
    fn step_palette(&mut self, down: bool, cx: &mut Context<Self>) {
        let count = self.palette_rows(cx).len();
        if let Some(palette) = self.palette.as_mut() {
            palette.highlight = if down {
                (palette.highlight + 1).min(count.saturating_sub(1))
            } else {
                palette.highlight.saturating_sub(1)
            };
            cx.notify();
        }
    }

    /// Runs the 0-based row `index`: a request opens in its tab with the Collections panel
    /// shown, an enabled command runs, a variable opens the workspace settings page at
    /// Environments and Search
    /// everything leaves the scope. A disabled command does nothing and keeps the palette
    /// open.
    fn run_palette_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.palette_rows(cx).into_iter().nth(index) else {
            return;
        };
        match row {
            PaletteRow::Request(entry) => {
                self.close_palette(window, cx);
                self.show_panel(Panel::Collections, cx);
                self.open_document(&entry.id, cx);
            }
            PaletteRow::Command(id) => {
                if !command(id).is_some_and(|command| command.enabled(self)) {
                    return;
                }
                self.close_palette(window, cx);
                self.run_command(id, window, cx);
            }
            PaletteRow::Variable(_) => {
                self.close_palette(window, cx);
                self.open_settings(super::settings_page::Section::Environments(None), cx);
            }
            PaletteRow::SearchEverything(text) => self.set_palette_query(text, window, cx),
        }
    }
}

pub(crate) mod model;
mod view;

pub(super) use view::render;
