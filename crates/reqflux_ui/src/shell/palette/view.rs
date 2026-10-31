//! The palette as drawn in design/command-palette/02-scope-tabs.html: the query row with the
//! count, the scope tabs, the rows under their headers, the empty and no-match states and the
//! key hint footer.

use bolt::chamfer::Chamfer;
use bolt::controls::Keycap;
use bolt::tokens::{border, chrome, cut, family, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div, point,
    prelude::*,
};
use gpui_component::input::{Enter, Escape, IndentInline, Input, MoveDown, MoveUp, OutdentInline};

use super::super::commands::{CommandId, command, key_label};
use super::super::rows::{labeled_row, part};
use super::super::{Shell, region, text as text_role};
use super::metrics;
use super::model::{Header, Listing, PaletteRow, Scope, VariableValue};
use crate::tokens::{self, method_color};

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

/// The keycaps of `key`, one per part: `ctrl-shift-r` is `Ctrl`, `Shift`, `R`.
fn keycaps(id: &str, key: &str) -> Vec<AnyElement> {
    key_label(key)
        .split('+')
        .enumerate()
        .map(|(index, part)| {
            Keycap::new(format!("{id}-key-{}", index + 1), part.to_owned()).into_any_element()
        })
        .collect()
}

/// A caption in the muted role.
fn caption(content: impl IntoElement, cx: &Context<Shell>) -> Div {
    text_role::role(div(), text::CAPTION, cx)
        .flex_none()
        .whitespace_nowrap()
        .child(content)
}

/// A caption in the mono key hint role.
fn mono(content: impl IntoElement, cx: &Context<Shell>) -> Div {
    text_role::role(div(), text::KEY_HINT, cx)
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
    let tint = bolt::brand(cx).accent_tint;
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
                    .w(metrics::METHOD_COLUMN)
                    .font_family(family::MONO)
                    .font_weight(FontWeight::BOLD)
                    .text_size(tokens::METHOD_LABEL_SIZE)
                    .text_color(method_color(&entry.method, bolt::palette(cx)))
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
                    Icon::new(command_icon(*command_id))
                        .size(icon::MD)
                        .color(colors.muted)
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
                Icon::new(IconName::Braces)
                    .size(icon::MD)
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
                    .gap(space::SM)
                    .child(mono(MASK, cx))
                    .child(
                        caption("secret", cx)
                            .flex()
                            .items_center()
                            .gap(space::XS)
                            .text_color(tint)
                            .child(Icon::new(IconName::KeyRound).size(icon::SM).color(tint)),
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
                Icon::new(IconName::Search)
                    .size(icon::MD)
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
    text_role::role(labeled_row(id, label), text::BODY, cx)
        .role(Role::MenuItem)
        .aria_selected(highlighted)
        .aria_disabled(!enabled)
        .flex()
        .items_center()
        .gap(space::MD)
        .h(metrics::ROW)
        .pl(metrics::INSET)
        .pr(metrics::INSET)
        .when(highlighted, |row| {
            row.bg(bolt::accent_washes(cx).selected_item)
                .border_l(border::THICK)
                .border_color(tint)
        })
        .when(enabled, |row| {
            row.cursor_pointer().hover(|style| style.bg(colors.hover))
        })
        .children(lead)
        .child(
            div()
                .flex_none()
                .max_w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(title_color)
                .when(title_mono, |title| title.font_family(family::MONO))
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
    text_role::role(labeled_row(id, header.title.clone()), text::LABEL, cx)
        .flex()
        .items_center()
        .gap(space::SM)
        .pt(space::MD)
        .pb(space::HALF)
        .pl(metrics::INSET)
        .pr(metrics::INSET)
        .child(header.title.clone())
        .child(div().flex_1())
        .children(header.hint.clone().map(|hint| caption(hint, cx)))
        .into_any_element()
}

/// The scope tab `palette-tab-<slug>`, labeled `<Title> <count>`, selected when active.
fn render_tab(scope: Scope, active: bool, count: usize, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("palette-tab-{}", scope.slug());
    let label = format!("{} {count}", scope.title());
    text_role::role(labeled_row(id, label), text::BODY_SMALL, cx)
        .role(Role::Tab)
        .aria_selected(active)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::HALF)
        .h(metrics::TAB)
        .pl(space::SM)
        .pr(space::SM)
        .cursor_pointer()
        .text_color(if active { colors.strong } else { colors.muted })
        .when(active, |tab| {
            tab.font_weight(FontWeight::SEMIBOLD)
                .child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .children(scope.prefix().map(|prefix| {
            div()
                .font_family(family::MONO)
                .text_color(if active {
                    bolt::brand(cx).accent_tint
                } else {
                    colors.line_2
                })
                .child(prefix.to_string())
        }))
        .child(scope.title())
        .child(mono(count.to_string(), cx))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.pick_scope(scope, window, cx);
        }))
        .into_any_element()
}

/// The empty or no-match state `palette-empty`, its title `palette-empty-title`.
fn render_empty(listing: &Listing, workspace_open: bool, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
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
    region("palette-empty")
        .flex()
        .flex_col()
        .items_center()
        .gap(space::HALF)
        .pt(space::XL)
        .pb(space::MD)
        .pl(metrics::INSET)
        .pr(metrics::INSET)
        .child(
            div()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .size(metrics::EMPTY_ICON_BOX)
                .child(Chamfer::new(cut::LARGE_CONTROL).fill(colors.raised))
                .child(
                    Icon::new(IconName::Search)
                        .size(icon::LG)
                        .color(colors.muted),
                ),
        )
        .child(
            text_role::role(
                labeled_row("palette-empty-title".to_owned(), title.clone()),
                text::BODY,
                cx,
            )
            .text_color(colors.strong)
            .font_weight(FontWeight::SEMIBOLD)
            .child(title),
        )
        .child(caption(hint, cx))
        .into_any_element()
}

/// A key hint in the footer: the keycap, then what it does.
fn hint(id: &str, key: &str, does: &str, cx: &Context<Shell>) -> [AnyElement; 2] {
    [
        Keycap::new(format!("palette-hint-{id}"), key.to_owned()).into_any_element(),
        caption(does.to_owned(), cx).into_any_element(),
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
            cx,
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
        .gap(space::MD)
        .h(metrics::QUERY_ROW)
        .pl(metrics::INSET)
        .pr(metrics::INSET)
        .border_b(border::THIN)
        .border_color(line)
        .child(
            Icon::new(IconName::Search)
                .size(icon::LG)
                .color(colors.foreground),
        )
        .child(
            region("palette-input")
                .role(Role::TextInput)
                .flex_1()
                .min_w_0()
                .h(chrome::CONTROL)
                .flex()
                .items_center()
                .child(
                    Input::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .flex_1()
                        .min_w_0()
                        .text_color(colors.strong),
                ),
        )
        .child({
            let count = listing.count_text();
            mono(count.clone(), cx)
                .id("palette-count")
                .role(Role::Label)
                .aria_label(count)
                .flex_none()
        })
        .child(Keycap::new("palette-hint-escape", "Esc"));

    let tab_row = region("palette-tabs")
        .role(Role::TabList)
        .flex_none()
        .flex()
        .items_center()
        .gap(space::XS)
        .h(metrics::TAB_ROW)
        .pl(space::MD)
        .pr(space::MD)
        .border_b(border::THIN)
        .border_color(line)
        .children(tabs)
        .child(div().flex_1())
        .child(div().flex().items_center().gap(space::HALF).children(hint(
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
        .gap(space::SM)
        .h(metrics::FOOTER)
        .pl(metrics::INSET)
        .pr(metrics::INSET)
        .border_t(border::THIN)
        .border_color(line)
        .children(footer_hints);

    let panel = region("palette")
        .role(Role::Dialog)
        .occlude()
        .relative()
        .w(tokens::PALETTE_WIDTH)
        .flex()
        .flex_col()
        .child(
            Chamfer::new(cut::CARD)
                .fill(colors.panel)
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

/// The scrolling list `palette-rows`.
fn rows_region(list: Vec<AnyElement>) -> Stateful<Div> {
    region("palette-rows")
        .flex()
        .flex_col()
        .pb(space::SM)
        .max_h(tokens::PALETTE_LIST_HEIGHT)
        .overflow_y_scroll()
        .children(list)
}
