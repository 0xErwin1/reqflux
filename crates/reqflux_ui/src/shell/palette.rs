//! The command palette (command-palette S1-S3): a query field over the window that finds
//! requests by name, URL or method and runs commands. Up and Down move the highlight, Enter
//! or a click runs a row, Escape or a click outside closes it and gives the focus back.

use bolt::chamfer::Chamfer;
use bolt::tokens::{border, chrome, cut, space, text};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Point, Role, Styled, Subscription, Window, anchored, deferred, div, point,
    prelude::*,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};

use super::commands::{COMMANDS, CommandId, command, key_label};
use super::palette_match::{RequestEntry, requests};
use super::rows::labeled_row;
use super::sidebar_history::Panel;
use super::{Shell, region, text as text_role};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The placeholder of the query field.
pub(crate) const PLACEHOLDER: &str = "Search requests or run a command";

/// The most rows the palette lists.
const MAX_ROWS: usize = 50;

/// The longest row label the automation tree gets; longer ones are cut.
const MAX_LABEL: usize = 200;

/// The open palette: its query field and the highlighted row, 0-based.
pub(crate) struct Palette {
    pub(crate) input: Entity<InputState>,
    pub(crate) highlight: usize,
    /// Resets the highlight when the query changes.
    _change: Subscription,
}

/// A row of the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaletteRow {
    /// Opens the request.
    Request(RequestEntry),
    /// Runs the command.
    Command(CommandId),
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

    /// The rows for the palette's current query.
    pub(crate) fn palette_rows(&self, cx: &gpui::App) -> Vec<PaletteRow> {
        let query = self
            .palette
            .as_ref()
            .map(|palette| palette.input.read(cx).value().to_string())
            .unwrap_or_default();
        let requests = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => requests(engine.workspace()),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
        };
        rows(&query, requests)
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
    /// shown, an enabled command runs. A disabled command does nothing and keeps the
    /// palette open.
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
                if !command(id).is_some_and(|command| (command.enabled)(self)) {
                    return;
                }
                self.close_palette(window, cx);
                self.run_command(id, window, cx);
            }
        }
    }
}

/// The rows for `query` over `requests`: every command with an empty query; else the
/// requests and commands that match, best first, ties in list order.
pub(crate) fn rows(query: &str, requests: Vec<RequestEntry>) -> Vec<PaletteRow> {
    if query.trim().is_empty() {
        return COMMANDS
            .iter()
            .map(|command| PaletteRow::Command(command.id))
            .collect();
    }
    let mut scored: Vec<(u32, PaletteRow)> = requests
        .into_iter()
        .filter_map(|entry| Some((entry.score(query)?, PaletteRow::Request(entry))))
        .chain(COMMANDS.iter().filter_map(|command| {
            let score = super::palette_match::score(query, command.label)?;
            Some((score, PaletteRow::Command(command.id)))
        }))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MAX_ROWS)
        .map(|(_, row)| row)
        .collect()
}

/// The single-line label the automation tree reads for a row, cut to [`MAX_LABEL`].
fn row_label(title: &str, detail: &str) -> String {
    let label = format!("{title} {detail}");
    let cut: String = label
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_LABEL)
        .collect();
    cut
}

/// The row `palette-row-<n>`, from 1, of `row`.
fn render_row(
    index: usize,
    row: &PaletteRow,
    highlighted: bool,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let (title, detail, enabled) = match row {
        PaletteRow::Request(entry) => (entry.title(), entry.path.clone(), true),
        PaletteRow::Command(id) => match command(*id) {
            Some(command) => (
                command.label.to_owned(),
                key_label(command.key),
                (command.enabled)(shell),
            ),
            None => (String::new(), String::new(), false),
        },
    };
    let label = row_label(&title, &detail);
    text_role::role(
        labeled_row(format!("palette-row-{}", index + 1), label),
        text::BODY,
        cx,
    )
    .role(Role::MenuItem)
    .aria_selected(highlighted)
    .aria_disabled(!enabled)
    .flex()
    .items_center()
    .gap(space::MD)
    .h(tokens::ENVIRONMENT_OPTION)
    .pl(space::MD)
    .pr(space::MD)
    .when(highlighted, |row| {
        row.bg(bolt::accent_washes(cx).selected_item)
    })
    .when(enabled, |row| {
        row.cursor_pointer()
            .text_color(colors.strong)
            .hover(|style| style.bg(colors.hover))
    })
    .when(!enabled, |row| row.text_color(colors.muted))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(title),
    )
    .child(
        text_role::role(div(), text::CAPTION, cx)
            .flex_none()
            .whitespace_nowrap()
            .child(detail),
    )
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.run_palette_row(index, window, cx);
    }))
    .into_any_element()
}

/// The open palette, centered under the title bar over a backdrop the size of the window
/// that closes it on a click outside.
pub(super) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let palette = shell.palette.as_ref()?;
    let highlight = palette.highlight;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (index, row) in shell.palette_rows(cx).iter().enumerate() {
        rows.push(render_row(index, row, index == highlight, shell, cx));
    }
    let colors = bolt::colors(cx);
    let field = region("palette-input")
        .role(Role::TextInput)
        .flex_none()
        .h(chrome::CONTROL)
        .flex()
        .items_center()
        .pl(space::MD)
        .pr(space::MD)
        .child(
            Input::new(&palette.input)
                .appearance(false)
                .bordered(false)
                .flex_1()
                .min_w_0()
                .text_color(colors.strong),
        );
    let panel = region("palette")
        .role(Role::Dialog)
        .occlude()
        .relative()
        .w(tokens::PALETTE_WIDTH)
        .py(space::HALF)
        .flex()
        .flex_col()
        .child(
            Chamfer::new(cut::OVERLAY)
                .fill(colors.raised)
                .edge(colors.line_2, border::THIN),
        )
        .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
            cx.stop_propagation();
            this.step_palette(false, cx);
        }))
        .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
            cx.stop_propagation();
            this.step_palette(true, cx);
        }))
        .on_action(cx.listener(|this, _: &Enter, window, cx| {
            let index = this.palette.as_ref().map_or(0, |palette| palette.highlight);
            this.run_palette_row(index, window, cx);
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.cancel_active(window, cx);
        }))
        .child(field)
        .child(
            region("palette-rows")
                .flex()
                .flex_col()
                .max_h(tokens::PALETTE_LIST_HEIGHT)
                .overflow_y_scroll()
                .children(rows),
        );
    let viewport = window.viewport_size();
    let left = ((viewport.width - tokens::PALETTE_WIDTH) / 2.).max(gpui::Pixels::ZERO);
    let backdrop = region("palette-backdrop")
        .occlude()
        .w(viewport.width)
        .h(viewport.height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| this.close_palette(window, cx)),
        );
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(3))
            .child(
                deferred(
                    anchored()
                        .position(point(left, chrome::TITLE_BAR))
                        .child(panel),
                )
                .with_priority(4),
            )
            .into_any_element(),
    )
}
