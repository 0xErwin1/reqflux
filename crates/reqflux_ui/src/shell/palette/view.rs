//! The palette as drawn in design/command-palette/02-scope-tabs.html: the query row with the
//! count, the scope tabs, the rows under their headers, the empty and no-match states and the
//! key hint footer.
//!
//! It draws as DBFlux's command palette (`PaletteMetrics`): the 56 px query row, 38 px rows
//! as `ListRow` with the selection bar, section headers in the label role, `Kbd` keycaps and
//! the 38 px footer; the scope tabs are `bolt` document tabs.

use bolt::composites::{EmptyState, ListRow, document_tab, document_tab_title};
use bolt::controls::{
    GpuiInput, InputEnter as Enter, InputEscape as Escape, InputIndentInline as IndentInline,
    InputMoveDown as MoveDown, InputMoveUp as MoveUp, InputOutdentInline as OutdentInline,
};
use bolt::primitives::{Chamfer, Icon, Kbd, TextVariant};
use bolt::tokens::{
    Borders, ChamferCut, HeaderMetrics, PaletteMetrics, ShellMetrics, Spacing, TabMetrics,
};
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div, point,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::super::commands::{CommandId, command, key_label};
use super::super::request::ui;
use super::super::rows::{labeled_row, part};
use super::super::{Shell, region, text_region};
use super::model::{Header, Listing, PaletteRow, Scope, VariableValue};
use crate::tokens::{self, method_hue};

/// The longest row label the automation tree gets; longer ones are cut.
const MAX_LABEL: usize = 200;

/// What a masked secret shows in place of its value.
const MASK: &str = "••••••••";

/// The single-line label the automation tree reads for a row, cut to [`MAX_LABEL`].
fn row_label(title: &str, detail: &str) -> String {
    let label = format!("{title} {detail}");
    label
        .trim_end()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_LABEL)
        .collect()
}

/// The icon of a command row.
fn command_icon(id: CommandId) -> IconName {
    match id {
        CommandId::SendRequest => IconName::Send,
        CommandId::CancelSend | CommandId::CloseWorkspace | CommandId::CloseTab => IconName::Close,
        CommandId::SaveRequest => IconName::Save,
        CommandId::RevertRequest => IconName::RotateCcw,
        CommandId::NewRequest | CommandId::NewEnvironment => IconName::Plus,
        CommandId::NewCollection | CommandId::ShowCollections => IconName::Box,
        CommandId::SwitchEnvironment | CommandId::ShowEnvironments => IconName::Layers,
        CommandId::WorkspaceSettings => IconName::Settings,
        CommandId::OpenWorkspace => IconName::Folder,
        CommandId::NewWorkspace => IconName::LayoutGrid,
        CommandId::NextTab | CommandId::PreviousTab => IconName::Columns2,
        CommandId::ShowHistory => IconName::RotateCcwClock,
        CommandId::ToggleTheme => IconName::Eye,
        CommandId::OpenApprovals => IconName::Bot,
    }
}

/// `bolt`'s keycap `key`, found by tests as `id`.
fn keycap(id: String, key: String) -> AnyElement {
    Kbd::new(key)
        .id(gpui::ElementId::Name(id.into()))
        .into_any_element()
}

/// The keycaps of `key`, one per part as DBFlux's palette draws them: `ctrl-shift-r` is
/// `Ctrl`, `Shift`, `R`.
fn keycaps(id: &str, key: &str) -> Vec<AnyElement> {
    key_label(key)
        .split('+')
        .enumerate()
        .map(|(index, part)| keycap(format!("{id}-key-{}", index + 1), part.to_owned()))
        .collect()
}

/// A caption in the muted role, at the palette's qualifier size.
fn caption(content: impl IntoElement, cx: &Context<Shell>) -> Div {
    ui::role(div(), TextVariant::Caption, cx)
        .text_size(PaletteMetrics::QUALIFIER_FONT)
        .flex_none()
        .whitespace_nowrap()
        .child(content)
}

/// A qualifier in the mono key hint role.
fn mono(content: impl IntoElement, cx: &Context<Shell>) -> Div {
    ui::role(div(), TextVariant::KeyHint, cx)
        .text_size(PaletteMetrics::QUALIFIER_FONT)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(content)
}

/// The row `palette-row-<n>`, from 1, of `row`. An unavailable command is dimmed, inert and
/// says why in `palette-row-<n>-reason`; a variable's value is `palette-row-<n>-value`.
fn render_row(
    index: usize,
    row: &PaletteRow,
    highlighted: bool,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let id = format!("palette-row-{}", index + 1);
    let mut enabled = true;
    let mut label = String::new();
    let mut lead: Option<AnyElement> = None;
    let mut title = String::new();
    let mut title_mono = false;
    let mut detail: Option<AnyElement> = None;
    let mut trailing: Vec<AnyElement> = Vec::new();
    match row {
        PaletteRow::Request(entry) => {
            label = row_label(&entry.title(), &entry.path);
            lead = Some(
                div()
                    .flex_none()
                    .w(tokens::TREE_METHOD_COLUMN)
                    .font_family(bolt::fonts::editor_family(cx))
                    .font_weight(FontWeight::BOLD)
                    .text_size(tokens::METHOD_LABEL_SIZE)
                    .text_color(method_hue(&entry.method, cx))
                    .child(entry.method.clone())
                    .into_any_element(),
            );
            title.clone_from(&entry.name);
            detail = Some(caption(entry.path.clone(), cx).into_any_element());
            trailing.push(mono(entry.url.clone(), cx).min_w_0().into_any_element());
        }
        PaletteRow::Command(command_id) => {
            if let Some(command) = command(*command_id) {
                label = row_label(command.label, &key_label(command.key));
                lead = Some(
                    Icon::new(ui::icon(command_icon(*command_id)))
                        .size(PaletteMetrics::ROW_ICON)
                        .color(if highlighted { tint } else { colors.muted })
                        .into_any_element(),
                );
                title = command.label.to_owned();
                if let Err(reason) = (command.available)(shell) {
                    enabled = false;
                    detail = Some(
                        caption(reason, cx)
                            .id(format!("{id}-reason"))
                            .role(Role::Label)
                            .aria_label(reason)
                            .into_any_element(),
                    );
                }
                trailing = keycaps(&id, command.key);
            }
        }
        PaletteRow::Variable(entry) => {
            label = row_label(
                &entry.title(),
                &format!("{} {}", entry.scope, entry.value_label()),
            );
            lead = Some(
                Icon::new(ui::icon(IconName::Braces))
                    .size(PaletteMetrics::ROW_ICON)
                    .color(tint)
                    .into_any_element(),
            );
            title = entry.title();
            title_mono = true;
            detail = Some(caption(entry.scope.clone(), cx).into_any_element());
            let value = part(format!("{id}-value")).role(Role::Label);
            trailing.push(match &entry.value {
                VariableValue::Secret => value
                    .aria_label("secret value hidden")
                    .flex()
                    .items_center()
                    .gap(Spacing::SM)
                    .child(mono(MASK, cx))
                    .child(
                        caption("secret", cx)
                            .flex()
                            .items_center()
                            .gap(Spacing::XS)
                            .text_color(tint)
                            .child(
                                Icon::new(ui::icon(IconName::KeyRound))
                                    .size(PaletteMetrics::QUALIFIER_FONT)
                                    .color(tint),
                            ),
                    )
                    .into_any_element(),
                VariableValue::Empty => value
                    .aria_label("empty")
                    .child(caption("empty", cx))
                    .into_any_element(),
                VariableValue::Plain(text) => value
                    .aria_label(text.clone())
                    .min_w_0()
                    .child(mono(text.clone(), cx))
                    .into_any_element(),
            });
        }
        PaletteRow::SearchEverything(text) => {
            title = format!("Search everything for \u{201c}{text}\u{201d}");
            label = row_label(&title, "");
            lead = Some(
                Icon::new(ui::icon(IconName::Search))
                    .size(PaletteMetrics::ROW_ICON)
                    .color(colors.muted)
                    .into_any_element(),
            );
        }
    }
    let title_color = match (enabled, highlighted) {
        (false, _) => colors.muted,
        (true, true) => colors.strong,
        (true, false) => colors.foreground,
    };
    let element = ListRow::new(id)
        .selected(highlighted)
        .selection_bar(true)
        .interactive(enabled)
        .build(cx)
        .role(Role::MenuItem)
        .aria_label(label)
        .aria_selected(highlighted)
        .aria_disabled(!enabled);
    ui::role(element, TextVariant::Body, cx)
        .flex_none()
        .flex()
        .items_center()
        .gap(PaletteMetrics::ROW_GAP)
        .h(PaletteMetrics::ROW_HEIGHT)
        .pl(PaletteMetrics::PADDING_X)
        .pr(PaletteMetrics::PADDING_X)
        .children(lead)
        .child(
            div()
                .flex_none()
                .max_w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(PaletteMetrics::ROW_FONT)
                .text_color(title_color)
                .when(title_mono, |title| {
                    title.font_family(bolt::fonts::editor_family(cx))
                })
                .child(title),
        )
        .children(detail)
        .child(div().flex_1())
        .children(trailing)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.run_palette_row(index, window, cx);
        }))
        .into_any_element()
}

/// The section header `palette-group-<slug>`: the title in the label role and its hint.
fn render_header(header: &Header, cx: &Context<Shell>) -> AnyElement {
    let id = format!("palette-group-{}", header.slug);
    ui::role(
        labeled_row(id, header.title.clone()),
        TextVariant::Label,
        cx,
    )
    .text_size(PaletteMetrics::SECTION_FONT)
    .flex()
    .items_center()
    .gap(Spacing::SM)
    .pt(PaletteMetrics::SECTION_PADDING_TOP)
    .pb(PaletteMetrics::SECTION_PADDING_BOTTOM)
    .pl(PaletteMetrics::PADDING_X)
    .pr(PaletteMetrics::PADDING_X)
    .child(header.title.clone())
    .child(div().flex_1())
    .children(header.hint.clone().map(|hint| caption(hint, cx)))
    .into_any_element()
}

/// The scope tab `palette-tab-<slug>`, a `bolt` document tab labeled `<Title> <count>`,
/// selected when active.
fn render_tab(scope: Scope, active: bool, count: usize, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("palette-tab-{}", scope.slug());
    let label = format!("{} {count}", scope.title());
    document_tab(id, active, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(active)
        .font_family(bolt::fonts::ui_family(cx))
        .children(scope.prefix().map(|prefix| {
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_color(if active {
                    bolt::brand::brand(cx).accent_tint
                } else {
                    colors.line_2
                })
                .child(prefix.to_string())
        }))
        .child(document_tab_title(scope.title(), active, cx))
        .child(
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TabMetrics::RESULT_META_SIZE)
                .text_color(colors.muted)
                .child(count.to_string()),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.pick_scope(scope, window, cx);
        }))
        .into_any_element()
}

/// The empty or no-match state `palette-empty`, `bolt`'s `EmptyState`, its title
/// `palette-empty-title`.
fn render_empty(listing: &Listing, workspace_open: bool) -> AnyElement {
    let noun = listing.scope.noun();
    let (title, hint) = if listing.text.is_empty() {
        let hint = if workspace_open {
            format!("This workspace has no {noun}.")
        } else {
            format!("Open a workspace to list its {noun}.")
        };
        (format!("No {noun}"), hint)
    } else {
        let quoted = format!("\u{201c}{}\u{201d}", listing.text);
        let title = match listing.scope {
            Scope::All => format!("Nothing matches {quoted}"),
            scope => format!("Nothing matches {quoted} in {}", scope.noun()),
        };
        (
            title,
            "Requests are searched by name, method and URL, commands by name and variables \
             by key."
                .to_owned(),
        )
    };
    div()
        .flex()
        .flex_col()
        .py(Spacing::XL)
        .pl(PaletteMetrics::PADDING_X)
        .pr(PaletteMetrics::PADDING_X)
        .child(
            EmptyState::new(ui::icon(IconName::Search), hint)
                .title(title)
                .id("palette-empty"),
        )
        .into_any_element()
}

/// A key hint in the footer: the keycap, then what it does.
fn hint(id: &str, key: &str, does: &str, cx: &Context<Shell>) -> [AnyElement; 2] {
    [
        keycap(format!("palette-hint-{id}"), key.to_owned()),
        ui::role(div(), TextVariant::Caption, cx)
            .text_size(PaletteMetrics::FOOTER_FONT)
            .flex_none()
            .whitespace_nowrap()
            .child(does.to_owned())
            .into_any_element(),
    ]
}

/// The open palette, centered under the title bar over a backdrop the size of the window
/// that closes it on a click outside.
pub(in crate::shell) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let palette = shell.palette.as_ref()?;
    let highlight = palette.highlight;
    let input = palette.input.clone();
    let listing = shell.palette_listing(cx);
    let mut list: Vec<AnyElement> = Vec::new();
    let empty = listing.is_empty();
    if empty {
        list.push(render_empty(
            &listing,
            matches!(
                shell.workspace,
                crate::workspace::OpenedWorkspace::Loaded(_)
            ),
        ));
    }
    let mut index = 0;
    for section in &listing.sections {
        if let Some(header) = &section.header {
            list.push(render_header(header, cx));
        }
        for row in &section.rows {
            list.push(render_row(index, row, index == highlight, shell, cx));
            index += 1;
        }
    }

    let mut tabs: Vec<AnyElement> = Vec::new();
    for scope in Scope::ORDER {
        let count = listing.matched.of(scope);
        tabs.push(render_tab(scope, scope == listing.scope, count, cx));
    }
    let colors = bolt::colors(cx);
    let line = colors.line;

    let query = region("palette-query")
        .flex_none()
        .flex()
        .items_center()
        .gap(PaletteMetrics::SEARCH_GAP)
        .h(PaletteMetrics::SEARCH_HEIGHT)
        .pl(PaletteMetrics::PADDING_X)
        .pr(PaletteMetrics::PADDING_X)
        .border_b(Borders::THIN)
        .border_color(line)
        .child(
            Icon::new(ui::icon(IconName::Search))
                .size(PaletteMetrics::SEARCH_ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(
            region("palette-input")
                .role(Role::TextInput)
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .child(
                    GpuiInput::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .px_0()
                        .flex_1()
                        .min_w_0()
                        .text_size(PaletteMetrics::QUERY_FONT)
                        .text_color(colors.strong),
                ),
        )
        .child({
            let count = listing.count_text();
            ui::role(text_region("palette-count"), TextVariant::KeyHint, cx)
                .text_size(PaletteMetrics::COUNT_FONT)
                .whitespace_nowrap()
                .aria_label(count.clone())
                .flex_none()
                .child(count)
        })
        .child(keycap("palette-hint-escape".to_owned(), "Esc".to_owned()));

    let tab_row = region("palette-tabs")
        .role(Role::TabList)
        .flex_none()
        .flex()
        .items_center()
        .gap(TabMetrics::DOCUMENT_BAR_GAP)
        .h(HeaderMetrics::PANEL_HEIGHT)
        .pl(TabMetrics::DOCUMENT_BAR_PADDING_X)
        .pr(TabMetrics::DOCUMENT_BAR_PADDING_X)
        .border_b(Borders::THIN)
        .border_color(line)
        .children(tabs)
        .child(div().flex_1())
        .child(div().flex().items_center().gap(Spacing::XXS).children(hint(
            "tab",
            "Tab",
            "next scope",
            cx,
        )));

    let mut footer_hints: Vec<AnyElement> = Vec::new();
    footer_hints.extend(hint("move", "\u{2191}\u{2193}", "move", cx));
    footer_hints.extend(hint("run", "\u{21b5}", "run", cx));
    footer_hints.push(div().flex_1().into_any_element());
    for scope in [Scope::Commands, Scope::Requests, Scope::Variables] {
        if let Some(prefix) = scope.prefix() {
            footer_hints.extend(hint(scope.slug(), &prefix.to_string(), scope.noun(), cx));
        }
    }
    let footer = region("palette-footer")
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .h(PaletteMetrics::FOOTER_HEIGHT)
        .pl(PaletteMetrics::PADDING_X)
        .pr(PaletteMetrics::PADDING_X)
        .border_t(Borders::THIN)
        .border_color(line)
        .children(footer_hints);

    let width = bolt::fonts::ui_px(cx, PaletteMetrics::WIDTH);
    let panel = region("palette")
        .role(Role::Dialog)
        .occlude()
        .relative()
        .w(width)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            Chamfer::new(ChamferCut::CARD)
                .fill(colors.panel)
                .border(colors.line_2),
        )
        .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
            cx.stop_propagation();
            this.step_palette(false, cx);
        }))
        .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
            cx.stop_propagation();
            this.step_palette(true, cx);
        }))
        .capture_action(cx.listener(|this, _: &IndentInline, window, cx| {
            cx.stop_propagation();
            this.step_scope(true, window, cx);
        }))
        .capture_action(cx.listener(|this, _: &OutdentInline, window, cx| {
            cx.stop_propagation();
            this.step_scope(false, window, cx);
        }))
        .on_action(cx.listener(|this, _: &Enter, window, cx| {
            let index = this.palette.as_ref().map_or(0, |palette| palette.highlight);
            this.run_palette_row(index, window, cx);
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.cancel_active(window, cx);
        }))
        .child(query)
        .child(tab_row)
        .child(rows_region(list))
        .child(footer);
    let viewport = window.viewport_size();
    let left = ((viewport.width - width) / 2.).max(gpui::Pixels::ZERO);
    let top = bolt::fonts::ui_px(cx, ShellMetrics::TITLE_BAR_HEIGHT);
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
            .child(deferred(anchored().position(point(left, top)).child(panel)).with_priority(4))
            .into_any_element(),
    )
}

/// The scrolling list `palette-rows`.
fn rows_region(list: Vec<AnyElement>) -> Stateful<Div> {
    region("palette-rows")
        .flex()
        .flex_col()
        .pb(PaletteMetrics::LIST_PADDING_BOTTOM)
        .max_h(tokens::PALETTE_LIST_HEIGHT)
        .overflow_y_scroll()
        .children(list)
}
