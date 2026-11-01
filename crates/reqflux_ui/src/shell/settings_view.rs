//! Drawing the workspace settings page (design-catch-up T7,
//! design/variables/01-settings-page.html), in place of the sidebar and the document islands
//! beside the rail: a header row with the workspace and `settings-close`, the section nav
//! island (`settings-nav`) and the content island with the section title, the environment
//! list in the Environments section and the variables column of [`super::variables_document`].
//!
//! It draws with DBFlux's `bolt`: `Island`s on the desk, the breadcrumb sizes of
//! `NavigationMetrics`, nav entries and environment rows as `ListRow`, `Button` and `Input`.

use bolt::composites::{Island, ListRow};
use bolt::controls::{Button, GpuiInput, InputEnter, InputEscape};
use bolt::primitives::{Chamfer, ChamferRing, Icon, TextVariant};
use bolt::tokens::{
    Borders, ChamferCut, Fields, HeaderMetrics, Heights, IslandMetrics, MasterListMetrics,
    NavigationMetrics, ShellMetrics, Spacing, TreeMetrics,
};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, Stateful, Styled, Window, div, prelude::*,
};
use reqflux_core::model::{Id, Workspace};
use reqflux_engine::SecretScope;

use super::request::{option_menu, ui};
use super::rows::labeled_row;
use super::settings_page::Section;
use super::variables_model::{
    ScopeEntry, collections, environments, scope_file, scope_name, variable_count,
};
use super::{Shell, region, text_region, variables_document};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The note at the foot of the nav.
const RESOLUTION: &str =
    "Resolution order: Globals, collection, environment. Current values win over initial ones.";

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
                .gap(IslandMetrics::GAP)
                .pb(IslandMetrics::GAP)
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

/// The header row, a breadcrumb: `Workspace settings` › the workspace and its path, and the
/// close button.
fn header(shell: &Shell, workspace: &Workspace, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let path = shell
        .workspace_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let name = workspace.manifest.name.clone();
    div()
        .flex_none()
        .h(HeaderMetrics::PANEL_HEIGHT)
        .flex()
        .items_center()
        .gap(NavigationMetrics::BREADCRUMB_GAP)
        .pl(Spacing::SM)
        .text_size(NavigationMetrics::BREADCRUMB_FONT)
        .child(
            Icon::new(ui::icon(IconName::Settings))
                .size(NavigationMetrics::BREADCRUMB_ICON)
                .color(tint),
        )
        .child(
            ui::role(text_region("settings-title"), TextVariant::Body, cx)
                .aria_label("Workspace settings")
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.strong)
                .child("Workspace settings"),
        )
        .child(
            Icon::new(ui::icon(IconName::ChevronRight))
                .size(NavigationMetrics::BREADCRUMB_CHEVRON)
                .color(colors.line_2),
        )
        .child(
            Icon::new(ui::icon(IconName::LayoutGrid))
                .size(NavigationMetrics::BREADCRUMB_ICON)
                .color(tint),
        )
        .child(
            labeled_row("settings-workspace".to_owned(), name.clone())
                .text_color(colors.foreground)
                .child(name),
        )
        .child(
            labeled_row("settings-workspace-path".to_owned(), path.clone())
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(NavigationMetrics::BREADCRUMB_META_FONT)
                .text_color(colors.muted)
                .child(path),
        )
        .child(div().flex_1())
        .child(
            Button::new("settings-close", "Close settings")
                .ghost()
                .icon(ui::icon(IconName::Close))
                .icon_only()
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
            ui::role(div(), TextVariant::Caption, cx)
                .pl(HeaderMetrics::PANEL_PADDING_LEFT)
                .pr(HeaderMetrics::PANEL_PADDING_LEFT)
                .py(Spacing::MD)
                .child(RESOLUTION),
        )
}

/// A section caption of the nav, as the sidebar draws its section labels.
fn caption(label: &'static str, cx: &App) -> Div {
    ui::role(div(), TextVariant::Label, cx)
        .text_size(ShellMetrics::SECTION_LABEL_FONT)
        .pt(HeaderMetrics::LABEL_PADDING_TOP)
        .pb(HeaderMetrics::LABEL_PADDING_BOTTOM)
        .pl(HeaderMetrics::PANEL_PADDING_LEFT)
        .pr(HeaderMetrics::PANEL_PADDING_LEFT)
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
    let tint = bolt::brand::brand(cx).accent_tint;
    let element = ListRow::new(id)
        .selected(active)
        .selection_bar(true)
        .build(cx)
        .role(Role::Link)
        .aria_label(format!("{name} {count}"))
        .aria_selected(active);
    ui::role(element, TextVariant::Body, cx)
        .flex_none()
        .h(Heights::TOOLBAR)
        .flex()
        .items_center()
        .gap(TreeMetrics::GAP)
        .pl(HeaderMetrics::PANEL_PADDING_LEFT)
        .pr(HeaderMetrics::PANEL_PADDING_LEFT)
        .when(active, |entry| entry.text_color(colors.strong))
        .child(
            Icon::new(ui::icon(icon_name))
                .size(TreeMetrics::ICON)
                .color(if active { tint } else { colors.muted }),
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
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TreeMetrics::META_FONT)
                .text_color(colors.muted)
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
    let colors = bolt::colors(cx);
    let title = div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(HeaderMetrics::SECTION_GAP)
        .pt(HeaderMetrics::SECTION_PADDING_TOP)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .pb(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            ui::role(text_region("settings-heading"), TextVariant::Title, cx)
                .aria_label(heading.clone())
                .child(heading),
        )
        .child(
            ui::role(text_region("settings-description"), TextVariant::Body, cx)
                .aria_label(description)
                .text_color(colors.muted)
                .child(description),
        );
    let variables = match scope {
        Some(scope) => {
            let name = scope_name(workspace, scope).unwrap_or_default();
            let file = scope_file(workspace, scope).unwrap_or_default();
            variables_document::render(shell, scope, &name, &file, cx)
        }
        None => ui::role(
            text_region("variables-no-environment"),
            TextVariant::Caption,
            cx,
        )
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
        ui::role(text_region(id), TextVariant::Caption, cx)
            .aria_label(message.clone())
            .flex_none()
            .pt(Spacing::SM)
            .pl(HeaderMetrics::SECTION_PADDING_X)
            .pr(HeaderMetrics::SECTION_PADDING_X)
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
        .border_r(Borders::THIN)
        .border_color(line)
        .child(
            div().flex_none().p(Spacing::MD).child(
                Button::new("variables-new-environment", "New environment")
                    .primary()
                    .icon(ui::icon(IconName::Plus))
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
    let colors = bolt::colors(cx);
    if let Some(field) = rename_field(shell, id, cx) {
        return div()
            .flex_none()
            .flex()
            .flex_col()
            .pl(MasterListMetrics::ROW_PADDING_X)
            .pr(MasterListMetrics::ROW_PADDING_X)
            .py(Spacing::SM)
            .border_b(Borders::THIN)
            .border_color(colors.row_divider)
            .child(field)
            .into_any_element();
    }
    let active = selected == Some(id);
    let meta = format!("{} · {}.env.toml", variable_count(entry.count), entry.slug);
    let menu_button = Button::new(
        gpui::ElementId::Name(format!("variables-scope-menu-{id}").into()),
        format!("Actions for {}", entry.name),
    )
    .icon(ui::icon(IconName::Ellipsis))
    .icon_only()
    .inline()
    .ghost()
    .tab_stop(false)
    .on_click(
        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
            cx.stop_propagation();
            this.open_settings_menu(id, event.position(), window, cx);
        }),
    );
    let menu = (shell.settings.menu == Some(id)).then(|| environment_menu(shell, id, window, cx));
    let element = ListRow::new(gpui::ElementId::Name(
        format!("variables-scope-environment-{id}").into(),
    ))
    .selected(active)
    .selection_bar(true)
    .build(cx)
    .role(Role::ListItem)
    .aria_label(entry.name.clone())
    .aria_selected(active);
    ui::role(element, TextVariant::Body, cx)
        .flex_none()
        .flex()
        .flex_col()
        .gap(MasterListMetrics::ROW_LINE_GAP)
        .pl(MasterListMetrics::ROW_PADDING_X)
        .pr(MasterListMetrics::ROW_PADDING_X)
        .py(MasterListMetrics::ROW_PADDING_Y)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .text_color(colors.strong)
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
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(MasterListMetrics::ROW_DETAIL_FONT)
                .text_color(colors.muted)
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
// bolt gap: bolt's frameless Input draws its text in the medium weight with no option, so
// the field draws its focused frame around gpui-component's frameless input, as the tree's
// rename does.
fn rename_field(shell: &Shell, id: Id, cx: &mut Context<Shell>) -> Option<Div> {
    let rename = shell
        .settings
        .rename
        .as_ref()
        .filter(|rename| rename.id == id)?;
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let hint = format!(
        "Enter renames {} and its file · Esc cancels",
        rename.original
    );
    let field = region("variables-rename-input")
        .role(Role::TextInput)
        .relative()
        .h(Fields::HEIGHT)
        .flex()
        .items_center()
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line)
                .ring(ChamferRing::focus(tint)),
        )
        .on_action(cx.listener(|this, _: &InputEnter, window, cx| {
            this.commit_environment_rename(window, cx);
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            this.cancel_environment_rename(window, cx);
        }))
        .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
            this.commit_environment_rename(window, cx);
        }))
        .child(
            GpuiInput::new(&rename.input)
                .appearance(false)
                .bordered(false)
                .p_0()
                .flex_1()
                .min_w_0()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(Fields::TEXT)
                .text_color(colors.strong),
        );
    Some(
        div().flex().flex_col().gap(Spacing::XS).child(field).child(
            ui::role(
                text_region("variables-rename-hint"),
                TextVariant::Caption,
                cx,
            )
            .aria_label(hint.clone())
            .text_size(MasterListMetrics::ROW_DETAIL_FONT)
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
    Some(
        ui::role(div(), TextVariant::Body, cx)
            .flex_none()
            .flex()
            .flex_col()
            .gap(Spacing::SM)
            .pl(MasterListMetrics::ROW_PADDING_X)
            .pr(MasterListMetrics::ROW_PADDING_X)
            .py(MasterListMetrics::ROW_PADDING_Y)
            .border_b(Borders::THIN)
            .border_color(colors.row_divider)
            .child(
                region("variables-delete-prompt")
                    .role(Role::Dialog)
                    .aria_label(question.clone())
                    .flex()
                    .flex_col()
                    .gap(Spacing::XS)
                    .child(div().text_color(colors.strong).child(question))
                    .child(ui::role(div(), TextVariant::Caption, cx).child(detail)),
            )
            .child(
                div()
                    .flex()
                    .gap(Spacing::XXS)
                    .child(
                        Button::new("variables-delete-confirm", "Delete")
                            .danger()
                            .inline()
                            .on_click(cx.listener(|this, _event, window, cx| {
                                this.confirm_environment_delete(window, cx);
                            })),
                    )
                    .child(
                        Button::new("variables-delete-cancel", "Cancel")
                            .secondary()
                            .inline()
                            .on_click(cx.listener(|this, _event, window, cx| {
                                this.cancel_environment_delete(window, cx);
                            })),
                    ),
            ),
    )
}
