//! Drawing the workspace settings page (design-catch-up T7,
//! design/variables/01-settings-page.html), in place of the sidebar and the document islands
//! beside the rail: a header row with the workspace and `settings-close`, the section nav
//! island (`settings-nav`) and the content island with the section title, the environment
//! list in the Environments section and the variables column of [`super::variables_document`].

use bolt::chamfer::Chamfer;
use bolt::controls::IconButton;
use bolt::tokens::{border, chrome, cut, family, icon, space, text};
use bolt::{Icon, IconName, Island};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, Role, Stateful, Styled, Window, div, prelude::*, transparent_black,
};
use gpui_component::input::{Enter, Escape, Input};
use reqflux_core::model::{Id, Workspace};
use reqflux_engine::SecretScope;

use super::request::option_menu;
use super::rows::labeled_row;
use super::settings_page::Section;
use super::variables_model::{
    ScopeEntry, collections, environments, scope_file, scope_name, variable_count,
};
use super::{Shell, region, text as text_role, text_region, variables_document};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The note at the foot of the nav.
const RESOLUTION: &str =
    "Resolution order: Globals, collection, environment. Current values win over initial ones.";

/// The tone of a page button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ButtonTone {
    /// Filled with the accent.
    Primary,
    /// On the raised fill.
    Neutral,
    /// Danger on its wash, with a danger edge.
    Danger,
}

/// The button `id`, `height` tall, with `icon` and `label`; the caller adds the click.
pub(super) fn button(
    id: &str,
    icon_name: Option<IconName>,
    label: &'static str,
    tone: ButtonTone,
    height: Pixels,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let (chamfer, color, weight) = match tone {
        ButtonTone::Primary => (
            Chamfer::new(cut::CONTROL).fill(brand.accent),
            brand.on_accent,
            FontWeight::BOLD,
        ),
        ButtonTone::Neutral => (
            Chamfer::new(cut::CONTROL).fill(colors.raised),
            colors.foreground,
            FontWeight::MEDIUM,
        ),
        ButtonTone::Danger => (
            Chamfer::new(cut::CONTROL)
                .fill(tokens::wash(colors.danger, tokens::APPROVAL_DENY_WASH))
                .edge(colors.danger, border::THIN),
            colors.danger,
            FontWeight::SEMIBOLD,
        ),
    };
    labeled_row(id.to_owned(), label.to_owned())
        .role(Role::Button)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(height)
        .pl(space::MD)
        .pr(space::MD)
        .cursor_pointer()
        .font_family(family::INTERFACE)
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(weight)
        .text_color(color)
        .whitespace_nowrap()
        .child(chamfer)
        .children(icon_name.map(|name| Icon::new(name).size(icon::MD).color(color)))
        .child(label)
}

/// The page, `settings-page`.
pub(super) fn render(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
        return div().into_any_element();
    };
    let workspace = engine.workspace();
    let section = shell.settings_section();
    let header = header(shell, workspace, cx);
    let nav = nav(workspace, section, cx);
    let content = content(shell, workspace, section, window, cx);
    region("settings-page")
        .role(Role::Group)
        .aria_label("Workspace settings")
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(header)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(chrome::ISLAND_GAP)
                .pb(chrome::ISLAND_GAP)
                .child(
                    Island::new()
                        .flex_none()
                        .w(tokens::SETTINGS_NAV_WIDTH)
                        .h_full()
                        .child(nav),
                )
                .child(Island::new().flex_1().min_w_0().h_full().child(content)),
        )
        .into_any_element()
}

/// The header row: `Workspace settings` › the workspace and its path, and the close button.
fn header(shell: &Shell, workspace: &Workspace, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let (strong, foreground, muted, line_2) = (
        colors.strong,
        colors.foreground,
        colors.muted,
        colors.line_2,
    );
    let path = shell
        .workspace_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let name = workspace.manifest.name.clone();
    div()
        .flex_none()
        .h(tokens::SETTINGS_HEADER)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::SM)
        .child(Icon::new(IconName::Settings).size(icon::ITEM).color(tint))
        .child(
            text_role::role(text_region("settings-title"), text::BODY, cx)
                .aria_label("Workspace settings")
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(strong)
                .child("Workspace settings"),
        )
        .child(
            Icon::new(IconName::ChevronRight)
                .size(icon::SM)
                .color(line_2),
        )
        .child(Icon::new(IconName::LayoutGrid).size(icon::CHIP).color(tint))
        .child(
            labeled_row("settings-workspace".to_owned(), name.clone())
                .text_size(tokens::REQUEST_LINE_TEXT)
                .text_color(foreground)
                .child(name),
        )
        .child(
            labeled_row("settings-workspace-path".to_owned(), path.clone())
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(family::MONO)
                .text_size(tokens::SETTINGS_PATH_TEXT)
                .text_color(muted)
                .child(path),
        )
        .child(div().flex_1())
        .child(
            IconButton::new("settings-close", IconName::Close, "Close settings")
                .on_click(cx.listener(|this, _event, window, cx| this.close_settings(window, cx))),
        )
}

/// The section nav, `settings-nav`: Globals and Environments under `WORKSPACE`, one entry per
/// collection under `COLLECTION VARIABLES`, and the resolution order at the foot.
fn nav(workspace: &Workspace, section: Section, cx: &mut Context<Shell>) -> Stateful<Div> {
    let environments = environments(workspace);
    let collections = collections(workspace);
    let entries: Vec<_> = collections
        .iter()
        .map(|entry| {
            nav_entry(
                format!("variables-scope-collection-{}", entry.id),
                IconName::Box,
                entry.name.clone(),
                entry.count,
                section == Section::Collection(entry.id),
                Section::Collection(entry.id),
                cx,
            )
        })
        .collect();
    let collections_caption = (!entries.is_empty()).then(|| caption("COLLECTION VARIABLES", cx));
    region("settings-nav")
        .role(Role::Navigation)
        .aria_label("Workspace settings sections")
        .size_full()
        .flex()
        .flex_col()
        .pt(space::SM)
        .child(caption("WORKSPACE", cx))
        .child(nav_entry(
            "variables-scope-globals".to_owned(),
            IconName::Variable,
            "Globals".to_owned(),
            workspace.manifest.globals.len(),
            section == Section::Globals,
            Section::Globals,
            cx,
        ))
        .child(nav_entry(
            "settings-nav-environments".to_owned(),
            IconName::Layers,
            "Environments".to_owned(),
            environments.len(),
            matches!(section, Section::Environments(_)),
            Section::Environments(None),
            cx,
        ))
        .children(collections_caption)
        .children(entries)
        .child(div().flex_1())
        .child(
            text_role::role(div(), text::CAPTION, cx)
                .p(space::LG)
                .pt(space::MD)
                .pb(space::MD)
                .child(RESOLUTION),
        )
}

fn caption(label: &'static str, cx: &App) -> Div {
    text_role::role(div(), text::LABEL, cx)
        .pt(space::MD)
        .pb(space::HALF)
        .pl(space::LG)
        .pr(space::LG)
        .child(label)
}

fn nav_entry(
    id: String,
    icon_name: IconName,
    name: String,
    count: usize,
    active: bool,
    target: Section,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let wash = bolt::accent_washes(cx).selected_item;
    let (hover, strong, muted) = (colors.hover, colors.strong, colors.muted);
    text_role::role(labeled_row(id, format!("{name} {count}")), text::BODY, cx)
        .role(Role::Link)
        .aria_selected(active)
        .flex_none()
        .h(tokens::SETTINGS_NAV_ITEM)
        .flex()
        .items_center()
        .gap(space::MD)
        .pl(space::LG)
        .pr(space::LG)
        .border_l(border::THICK)
        .border_color(transparent_black())
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(active, |entry| {
            entry.bg(wash).border_color(tint).text_color(strong)
        })
        .child(
            Icon::new(icon_name)
                .size(icon::ITEM)
                .color(if active { tint } else { muted }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(name),
        )
        .child(
            div()
                .font_family(family::MONO)
                .text_size(tokens::SETTINGS_COUNT_TEXT)
                .text_color(muted)
                .child(count.to_string()),
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.select_settings_section(target, window, cx);
        }))
        .into_any_element()
}

/// The content island: the section title and description, the message of the last write,
/// and the environment list beside the variables column.
fn content(
    shell: &Shell,
    workspace: &Workspace,
    section: Section,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Div {
    let scope = section.scope();
    let (heading, description) = match section {
        Section::Globals => (
            "Globals".to_owned(),
            "Initial values live in reqflux.toml and are committed. Current values stay on \
             this machine. Secrets live in the OS keyring.",
        ),
        Section::Environments(_) => (
            "Environments".to_owned(),
            "Initial values live in the environment file and are committed. Current values \
             stay on this machine. Secrets live in the OS keyring.",
        ),
        Section::Collection(id) => (
            scope_name(workspace, SecretScope::Collection(id)).unwrap_or_default(),
            "Initial values live in the collection file and are committed. Current values \
             stay on this machine. Secrets live in the OS keyring.",
        ),
    };
    let line = bolt::colors(cx).line;
    let title = div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(space::XS)
        .pt(tokens::SETTINGS_TITLE_TOP)
        .pl(tokens::SETTINGS_TITLE_INSET)
        .pr(tokens::SETTINGS_TITLE_INSET)
        .pb(tokens::SETTINGS_TITLE_BOTTOM)
        .border_b(border::THIN)
        .border_color(line)
        .child(
            text_role::role(text_region("settings-heading"), text::TITLE, cx)
                .aria_label(heading.clone())
                .child(heading),
        )
        .child(
            text_role::role(text_region("settings-description"), text::BODY, cx)
                .aria_label(description)
                .text_color(bolt::colors(cx).muted)
                .child(description),
        );
    let variables = match scope {
        Some(scope) => {
            let name = scope_name(workspace, scope).unwrap_or_default();
            let file = scope_file(workspace, scope).unwrap_or_default();
            variables_document::render(shell, scope, &name, &file, cx)
        }
        None => text_role::role(text_region("variables-no-environment"), text::CAPTION, cx)
            .aria_label("No environments yet")
            .p(tokens::VARIABLES_PANEL_INSET)
            .child("No environments yet")
            .into_any_element(),
    };
    let list = matches!(section, Section::Environments(_))
        .then(|| environment_list(shell, workspace, section, window, cx));
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(title)
        .children(message(shell, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .children(list)
                .child(variables),
        )
}

/// The error of the last failed write, `variables-error`, in the danger tone, or the warning
/// of the last one that succeeded, `variables-warning`.
fn message(shell: &Shell, cx: &App) -> Option<AnyElement> {
    let colors = bolt::colors(cx);
    let (id, message, color) = match (&shell.settings.error, &shell.settings.warning) {
        (Some(error), _) => ("variables-error", error.clone(), colors.danger),
        (None, Some(warning)) => ("variables-warning", warning.clone(), colors.warning),
        (None, None) => return None,
    };
    Some(
        text_role::role(text_region(id), text::CAPTION, cx)
            .aria_label(message.clone())
            .flex_none()
            .pt(space::SM)
            .pl(tokens::SETTINGS_TITLE_INSET)
            .pr(tokens::SETTINGS_TITLE_INSET)
            .text_color(color)
            .child(message)
            .into_any_element(),
    )
}

/// The environment list: `New environment`, the delete confirmation, and one row per
/// environment by name.
fn environment_list(
    shell: &Shell,
    workspace: &Workspace,
    section: Section,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Div {
    let line = bolt::colors(cx).line;
    let selected = match section {
        Section::Environments(id) => id,
        Section::Globals | Section::Collection(_) => None,
    };
    let entries = environments(workspace);
    let prompt = environment_delete_prompt(shell, &entries, cx);
    let rows: Vec<_> = entries
        .into_iter()
        .map(|entry| environment_row(shell, entry, selected, window, cx))
        .collect();
    div()
        .flex_none()
        .w(tokens::SETTINGS_ENVIRONMENT_LIST)
        .h_full()
        .flex()
        .flex_col()
        .border_r(border::THIN)
        .border_color(line)
        .child(
            div().flex_none().p(space::MD).child(
                button(
                    "variables-new-environment",
                    Some(IconName::Plus),
                    "New environment",
                    ButtonTone::Primary,
                    chrome::CONTROL,
                    cx,
                )
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.new_environment(window, cx);
                })),
            ),
        )
        .children(prompt)
        .child(
            region("variables-environments")
                .role(Role::List)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(rows),
        )
}

fn environment_row(
    shell: &Shell,
    entry: ScopeEntry,
    selected: Option<Id>,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let id = entry.id;
    let divider = bolt::colors(cx).row_divider;
    if let Some(field) = rename_field(shell, id, cx) {
        return div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(space::HALF)
            .pt(space::SM)
            .pb(space::SM)
            .pl(tokens::SETTINGS_ENVIRONMENT_INSET)
            .pr(tokens::SETTINGS_ENVIRONMENT_INSET)
            .border_b(border::THIN)
            .border_color(divider)
            .child(field)
            .into_any_element();
    }
    let active = selected == Some(id);
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let wash = bolt::accent_washes(cx).selected_item;
    let (hover, strong, muted) = (colors.hover, colors.strong, colors.muted);
    let meta = format!("{} · {}.env.toml", variable_count(entry.count), entry.slug);
    let menu_button = super::rows::part(format!("variables-scope-menu-{id}"))
        .role(Role::Button)
        .aria_label(format!("Actions for {}", entry.name))
        .flex_none()
        .cursor_pointer()
        .child(Icon::new(IconName::Ellipsis).size(icon::MD).color(muted))
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                this.open_settings_menu(id, event.position(), window, cx);
            }),
        );
    let menu = (shell.settings.menu == Some(id)).then(|| environment_menu(shell, id, window, cx));
    text_role::role(
        labeled_row(
            format!("variables-scope-environment-{id}"),
            entry.name.clone(),
        ),
        text::BODY,
        cx,
    )
    .role(Role::ListItem)
    .aria_selected(active)
    .relative()
    .flex_none()
    .flex()
    .flex_col()
    .gap(space::XS)
    .pt(space::SM)
    .pb(space::SM)
    .pl(tokens::SETTINGS_ENVIRONMENT_INSET)
    .pr(tokens::SETTINGS_ENVIRONMENT_INSET)
    .border_b(border::THIN)
    .border_color(divider)
    .cursor_pointer()
    .hover(move |style| style.bg(hover))
    // The left edge marks the selected environment, as the nav marks its section.
    .when(active, |row| {
        row.bg(wash).child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(border::THICK)
                .bg(tint),
        )
    })
    .child(
        div()
            .flex()
            .items_center()
            .gap(space::SM)
            .text_color(strong)
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(entry.name.clone()),
            )
            .child(menu_button)
            .children(menu),
    )
    .child(
        labeled_row(format!("variables-environment-meta-{id}"), meta.clone())
            .font_family(family::MONO)
            .text_size(tokens::SETTINGS_ENVIRONMENT_META_TEXT)
            .text_color(muted)
            .child(meta),
    )
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.select_settings_section(Section::Environments(Some(id)), window, cx);
    }))
    .into_any_element()
}

/// The `…` menu of the environment `id`: Rename and Delete environment….
fn environment_menu(shell: &Shell, id: Id, window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let items = vec![
        option_menu::option(
            "variables-menu-rename".to_owned(),
            "Rename".to_owned(),
            false,
            move |this: &mut Shell, window, cx| this.start_environment_rename(id, window, cx),
            cx,
        ),
        option_menu::option(
            "variables-menu-delete".to_owned(),
            "Delete environment…".to_owned(),
            false,
            move |this: &mut Shell, window, cx| this.ask_environment_delete(id, window, cx),
            cx,
        ),
    ];
    option_menu::render_at(
        ("variables-menu", "variables-menu-backdrop"),
        shell.settings.menu_at,
        &shell.settings.menu_focus,
        items,
        Shell::close_settings_menu,
        window,
        cx,
    )
}

/// The rename field of the environment `id`, `variables-rename-input`, over its hint.
fn rename_field(shell: &Shell, id: Id, cx: &mut Context<Shell>) -> Option<Div> {
    let rename = shell
        .settings
        .rename
        .as_ref()
        .filter(|rename| rename.id == id)?;
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let hint = format!(
        "Enter renames {} and its file · Esc cancels",
        rename.original
    );
    let field = region("variables-rename-input")
        .role(Role::TextInput)
        .relative()
        .h(tokens::SETTINGS_ENVIRONMENT_FIELD)
        .flex()
        .items_center()
        .pl(space::HALF)
        .pr(space::HALF)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(tint, border::FOCUS_RING),
        )
        .on_action(cx.listener(|this, _: &Enter, window, cx| {
            this.commit_environment_rename(window, cx);
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.cancel_environment_rename(window, cx);
        }))
        .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
            this.commit_environment_rename(window, cx);
        }))
        .child(
            Input::new(&rename.input)
                .appearance(false)
                .bordered(false)
                .flex_1()
                .min_w_0()
                .font_family(family::MONO)
                .text_color(colors.strong),
        );
    Some(
        div().flex().flex_col().gap(space::HALF).child(field).child(
            text_role::role(text_region("variables-rename-hint"), text::CAPTION, cx)
                .aria_label(hint.clone())
                .text_size(tokens::SETTINGS_ENVIRONMENT_META_TEXT)
                .child(hint),
        ),
    )
}

/// The delete confirmation, `variables-delete-prompt`, with `variables-delete-confirm` and
/// `variables-delete-cancel`.
fn environment_delete_prompt(
    shell: &Shell,
    entries: &[ScopeEntry],
    cx: &mut Context<Shell>,
) -> Option<Div> {
    let id = shell.settings.confirm?;
    let name = entries.iter().find(|entry| entry.id == id)?.name.clone();
    let question = format!("Delete environment \"{name}\"?");
    let detail = "Its file is deleted. Keyring entries of its secrets stay.";
    let colors = bolt::colors(cx);
    let (divider, strong) = (colors.row_divider, colors.strong);
    Some(
        text_role::role(div(), text::BODY, cx)
            .flex_none()
            .flex()
            .flex_col()
            .gap(space::SM)
            .p(tokens::SETTINGS_ENVIRONMENT_INSET)
            .border_b(border::THIN)
            .border_color(divider)
            .child(
                region("variables-delete-prompt")
                    .role(Role::Dialog)
                    .aria_label(question.clone())
                    .flex()
                    .flex_col()
                    .gap(space::XS)
                    .child(div().text_color(strong).child(question))
                    .child(text_role::role(div(), text::CAPTION, cx).child(detail)),
            )
            .child(
                div()
                    .flex()
                    .gap(space::HALF)
                    .child(
                        button(
                            "variables-delete-confirm",
                            None,
                            "Delete",
                            ButtonTone::Danger,
                            chrome::CONTROL_INLINE,
                            cx,
                        )
                        .on_click(cx.listener(
                            |this, _event, window, cx| {
                                this.confirm_environment_delete(window, cx);
                            },
                        )),
                    )
                    .child(
                        button(
                            "variables-delete-cancel",
                            None,
                            "Cancel",
                            ButtonTone::Neutral,
                            chrome::CONTROL_INLINE,
                            cx,
                        )
                        .on_click(cx.listener(
                            |this, _event, window, cx| {
                                this.cancel_environment_delete(window, cx);
                            },
                        )),
                    ),
            ),
    )
}
