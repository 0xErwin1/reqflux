//! Drawing the App section of the settings page (app-pending S3,
//! design/app-settings/02-workspace-page-app-section.html): the `Settings` header with the
//! config path (`config-path`), the `APP` nav entries (`nav-general`, `nav-mcp-policy`) and
//! the two sections. General holds the theme (`theme`) and the interface font size
//! (`font-size`); MCP policy the four rules (`rule-read`, `rule-write`, `rule-execute`,
//! `rule-destructive`), the host and environment lists (`hosts-list`, `environments-list`)
//! and the read-only workspace overrides (`overrides-table`, `overrides-open-config`). A file
//! that does not parse shows `config-error` with `config-open` and `config-reload`, and every
//! control is drawn dimmed and does not write.
//!
//! It draws with `bolt`: `ListRow` nav entries, `SegmentedControl`, `Dropdown`, `Input`,
//! `Button`, `Badge` and `BannerBlock`.

use bolt::composites::ListRow;
use bolt::controls::{Button, Input};
use bolt::primitives::{
    Badge, BadgeTone, BannerBlock, BannerVariant, Icon, SegmentedItem, TextVariant,
};
use bolt::tokens::{
    Borders, GridMetrics, HeaderMetrics, Heights, NavigationMetrics, Spacing, TreeMetrics,
};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, IntoElement, ParentElement, Pixels, Role,
    SharedString, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::Workspace;
use reqflux_engine::config::{ConfigError, McpConfig, McpList, McpOverride, Rule, ThemeMode};
use reqflux_engine::policy::ToolClass;

use super::super::request::{segmented, ui};
use super::super::rows::labeled_row;
use super::super::settings_page::AppSection;
use super::super::settings_page::app_settings::font_size_label;
use super::super::{Shell, region, text_region};
use super::{caption, message};

use crate::tokens::{
    APP_SETTINGS_COLUMN_GAP as POLICY_COLUMN_GAP, APP_SETTINGS_FONT_SIZE_WIDTH as FONT_SIZE_WIDTH,
    APP_SETTINGS_LABEL_WIDTH as GENERAL_LABEL_WIDTH,
    APP_SETTINGS_OVERRIDE_RULE as OVERRIDE_RULE_WIDTH,
    APP_SETTINGS_OVERRIDE_WORKSPACE as OVERRIDE_WORKSPACE_WIDTH,
    APP_SETTINGS_RULE_LABEL_WIDTH as RULE_LABEL_WIDTH,
};
/// What the dimmed controls of a file that does not parse are drawn at
/// (bolt gap: `SegmentedControl` has no disabled state).
const READ_ONLY_OPACITY: f32 = 0.45;

/// The rules in the order the page draws them, with their labels and built-in defaults.
const RULES: [(ToolClass, &str, Rule); 4] = [
    (ToolClass::Read, "Read", Rule::Allow),
    (ToolClass::Write, "Write", Rule::Ask),
    (ToolClass::Execute, "Execute", Rule::Ask),
    (ToolClass::Destructive, "Destructive", Rule::Ask),
];

fn rule_label(rule: Rule) -> &'static str {
    match rule {
        Rule::Allow => "Allow",
        Rule::Ask => "Ask",
        Rule::Deny => "Deny",
    }
}

fn rule_id(rule: Rule) -> &'static str {
    match rule {
        Rule::Allow => "allow",
        Rule::Ask => "ask",
        Rule::Deny => "deny",
    }
}

/// The header row of the App sections: `Settings`, where app settings live and the close
/// button.
pub(super) fn header(shell: &Shell, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let path = shell
        .config_file()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "no config directory".to_owned());
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
                .aria_label("Settings")
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.strong)
                .child("Settings"),
        )
        .child(
            ui::role(div(), TextVariant::Caption, cx)
                .flex_none()
                .child("App: this machine,"),
        )
        .child(
            labeled_row("config-path".to_owned(), path.clone())
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(NavigationMetrics::BREADCRUMB_META_FONT)
                .text_color(colors.foreground)
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

/// The `APP` caption and its two entries; General carries a warning icon while the file
/// is not used.
pub(super) fn nav_entries(
    shell: &Shell,
    active: Option<AppSection>,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let failed = shell.settings.app.error().is_some();
    vec![
        caption("APP".into(), cx).into_any_element(),
        nav_entry(
            "nav-general",
            IconName::Sun,
            "General",
            failed,
            active == Some(AppSection::General),
            AppSection::General,
            cx,
        ),
        nav_entry(
            "nav-mcp-policy",
            IconName::Bot,
            "MCP policy",
            false,
            active == Some(AppSection::McpPolicy),
            AppSection::McpPolicy,
            cx,
        ),
    ]
}

fn nav_entry(
    id: &'static str,
    icon_name: IconName,
    name: &'static str,
    warning: bool,
    active: bool,
    target: AppSection,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let element = ListRow::new(id)
        .selected(active)
        .selection_bar(true)
        .build(cx)
        .role(Role::Link)
        .aria_label(name)
        .aria_selected(active);
    let alert = warning.then(|| {
        Icon::new(ui::icon(IconName::TriangleAlert))
            .size(TreeMetrics::ICON)
            .color(colors.danger)
    });
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
        .child(div().flex_1().min_w_0().child(name))
        .children(alert)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.ensure_app_controls(window, cx);
            this.select_settings_section(super::Section::App(target), window, cx);
        }))
        .into_any_element()
}

/// The content island of an App section.
pub(super) fn content(
    shell: &Shell,
    section: AppSection,
    workspace: Option<&Workspace>,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Div {
    let (heading, description) = match section {
        AppSection::General => ("General", "How ReqFlux looks on this machine."),
        AppSection::McpPolicy => (
            "MCP policy",
            "What agents may do through reqflux mcp. Changes save to config.toml at once; \
             comments and key order stay.",
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
                .aria_label(heading)
                .child(heading),
        )
        .child(
            ui::role(text_region("settings-description"), TextVariant::Body, cx)
                .aria_label(description)
                .text_color(colors.muted)
                .child(description),
        );
    let banner = shell
        .settings
        .app
        .error()
        .map(|error| config_error(shell, error, cx));
    let body = match section {
        AppSection::General => general(shell, window, cx),
        AppSection::McpPolicy => policy(shell, workspace, window, cx),
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(title)
        .children(message(shell, cx))
        .children(banner)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .pl(HeaderMetrics::SECTION_PADDING_X)
                .pr(HeaderMetrics::SECTION_PADDING_X)
                .child(body),
        )
}

/// `config-error`: why the file is not used, never quoting it, with Open file and Reload.
fn config_error(shell: &Shell, error: &ConfigError, cx: &mut Context<Shell>) -> AnyElement {
    let (title, detail) = match error {
        ConfigError::Parse { path, location } => (
            "config.toml does not parse, so ReqFlux runs on the defaults.",
            format!("{}, {location}.", path.display()),
        ),
        other => (
            "config.toml cannot be read, so ReqFlux runs on the defaults.",
            format!("{other}."),
        ),
    };
    let body = format!(
        "{detail} App settings are read-only until it parses, so nothing overwrites your \
         file. Workspace settings still work."
    );
    let can_open = shell.config_file().is_some();
    let actions = div()
        .flex()
        .gap(Spacing::SM)
        .child(
            Button::new("config-open", "Open file")
                .secondary()
                .icon(ui::icon(IconName::ExternalLink))
                .disabled(!can_open)
                .on_click(cx.listener(|this, _event, _window, cx| this.open_config_file(cx))),
        )
        .child(
            Button::new("config-reload", "Reload")
                .secondary()
                .icon(ui::icon(IconName::RefreshCw))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.reload_app_config(window, cx);
                })),
        );
    region("config-error")
        .role(Role::Alert)
        .aria_label(format!("{title} {body}"))
        .flex_none()
        .pt(Spacing::LG)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .child(
            BannerBlock::new(BannerVariant::Danger, title)
                .default_icon(ui::icon(IconName::TriangleAlert))
                .with_body(body)
                .with_actions(actions),
        )
        .into_any_element()
}

/// A section caption, as `h2.cap` of the artboard.
fn section_caption(label: &'static str, cx: &App) -> Div {
    ui::role(div(), TextVariant::Label, cx)
        .pt(Spacing::LG)
        .pb(Spacing::XS)
        .child(label)
}

/// A field row: the label and its note on the left, the control on the right.
fn field_row(
    label: &'static str,
    note: String,
    width: Pixels,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    let colors = bolt::colors(cx);
    div()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .py(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex_none()
                .w(width)
                .flex()
                .flex_col()
                .gap(Spacing::XXS)
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.strong)
                        .child(label),
                )
                .child(ui::role(div(), TextVariant::Caption, cx).child(note)),
        )
        .child(div().flex_1().min_w_0().flex().child(control))
}

/// A control drawn dimmed while the file is read-only.
fn dimmed(editable: bool, control: impl IntoElement) -> Div {
    div()
        .flex_none()
        .when(!editable, |control| control.opacity(READ_ONLY_OPACITY))
        .child(control)
}

/// General: the theme and the interface font size.
fn general(shell: &Shell, _window: &Window, cx: &mut Context<Shell>) -> Div {
    let app = &shell.settings.app;
    let editable = app.editable();
    let theme = app
        .config()
        .and_then(|config| config.theme)
        .unwrap_or_default();
    let shell_entity = cx.entity().downgrade();
    let items = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System]
        .into_iter()
        .map(|mode| {
            let label = match mode {
                ThemeMode::Dark => "Dark",
                ThemeMode::Light => "Light",
                ThemeMode::System => "System",
            };
            SegmentedItem::new(mode.name(), label).element_id(format!("theme-{}", mode.name()))
        })
        .collect();
    let control = segmented::control(
        "theme",
        Some("Theme".into()),
        false,
        items,
        theme.name(),
        move |id: &SharedString, window, cx| {
            let Some(mode) = ThemeMode::from_name(id) else {
                return;
            };
            let updated = shell_entity.update(cx, |this, cx| {
                this.select_app_theme(mode, window, cx);
            });
            if let Err(error) = updated {
                log::warn!("the theme control outlived the shell: {error}");
            }
        },
    );
    let font_size = app
        .controls
        .as_ref()
        .map(|controls| div().w(FONT_SIZE_WIDTH).child(controls.font_size.clone()));
    let size = app
        .config()
        .map_or(reqflux_engine::config::DEFAULT_FONT_SIZE, |config| {
            config.interface_font_size()
        });
    div()
        .flex()
        .flex_col()
        .child(section_caption("APPEARANCE", cx))
        .child(field_row(
            "Theme",
            "System follows the OS appearance".to_owned(),
            GENERAL_LABEL_WIDTH,
            dimmed(editable, control),
            cx,
        ))
        .child(field_row(
            "Interface font size",
            format!(
                "Panels, tabs and menus; data follows it. Now {}",
                font_size_label(size)
            ),
            GENERAL_LABEL_WIDTH,
            dimmed(editable, div().children(font_size)),
            cx,
        ))
}

/// MCP policy: the rules and the host list on the left, the environment list and the
/// workspace overrides on the right.
fn policy(
    shell: &Shell,
    workspace: Option<&Workspace>,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Div {
    let app = &shell.settings.app;
    let editable = app.editable();
    let mcp = app
        .config()
        .map(|config| config.mcp.clone())
        .unwrap_or_default();
    let rules: Vec<_> = RULES
        .iter()
        .map(|&(class, label, default)| {
            let current = match class {
                ToolClass::Read => mcp.read,
                ToolClass::Write => mcp.write,
                ToolClass::Execute => mcp.execute,
                ToolClass::Destructive => mcp.destructive,
            }
            .unwrap_or(default);
            field_row(
                label,
                format!("Default {}", rule_label(default)),
                RULE_LABEL_WIDTH,
                dimmed(editable, rule_control(class, current, cx)),
                cx,
            )
        })
        .collect();
    let left = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(section_caption("TOOL CLASSES", cx))
        .children(rules)
        .child(section_caption("HOSTS EXECUTE MAY REACH", cx))
        .child(list(
            shell,
            McpList::Hosts,
            mcp.hosts.as_deref(),
            window,
            cx,
        ))
        .child(
            ui::role(div(), TextVariant::Caption, cx)
                .pt(Spacing::SM)
                .child("Remove every host to allow any host."),
        );
    let right = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(section_caption("ENVIRONMENTS EXECUTE MAY USE", cx))
        .child(list(
            shell,
            McpList::Environments,
            mcp.environments.as_deref(),
            window,
            cx,
        ))
        .child(overrides(&mcp, workspace, cx));
    div().flex().gap(POLICY_COLUMN_GAP).child(left).child(right)
}

/// The Allow / Ask / Deny control `rule-<class>`.
fn rule_control(class: ToolClass, current: Rule, cx: &mut Context<Shell>) -> AnyElement {
    let group = format!("rule-{}", class.name());
    let items = [Rule::Allow, Rule::Ask, Rule::Deny]
        .into_iter()
        .map(|rule| {
            SegmentedItem::new(rule_id(rule), rule_label(rule))
                .element_id(format!("{group}-{}", rule_id(rule)))
        })
        .collect();
    let shell = cx.entity().downgrade();
    let label = RULES
        .iter()
        .find(|(candidate, _, _)| *candidate == class)
        .map_or("", |(_, label, _)| *label);
    segmented::control(
        &group,
        Some(label.into()),
        false,
        items,
        rule_id(current),
        move |id: &SharedString, _window, cx| {
            let rule = match id.as_ref() {
                "allow" => Rule::Allow,
                "ask" => Rule::Ask,
                "deny" => Rule::Deny,
                _ => return,
            };
            let updated = shell.update(cx, |this, cx| this.select_app_rule(class, rule, cx));
            if let Err(error) = updated {
                log::warn!("the rule control outlived the shell: {error}");
            }
        },
    )
    .into_any_element()
}

/// `hosts-list` or `environments-list`: a row per entry with its Remove, and the field
/// with its Add button.
fn list(
    shell: &Shell,
    list: McpList,
    entries: Option<&[String]>,
    _window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let editable = shell.settings.app.editable();
    let name = list.key();
    let (region_id, add_label, placeholder) = match list {
        McpList::Hosts => ("hosts-list", "Add host", "api.example.com"),
        McpList::Environments => ("environments-list", "Add environment", "environment slug"),
    };
    let rows: Vec<_> = entries
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let removed = entry.clone();
            labeled_row(format!("{name}-row-{index}"), entry.clone())
                .h(GridMetrics::ROW_HEIGHT)
                .flex()
                .items_center()
                .border_b(Borders::THIN)
                .border_color(colors.row_divider)
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(GridMetrics::FONT)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.strong)
                        .child(entry.clone()),
                )
                .child(
                    Button::new(
                        gpui::ElementId::Name(format!("{name}-remove-{index}").into()),
                        format!("Remove {entry}"),
                    )
                    .ghost()
                    .inline()
                    .icon(ui::icon(IconName::Trash))
                    .icon_only()
                    .disabled(!editable)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.remove_app_list_entry(list, &removed, cx);
                    })),
                )
                .into_any_element()
        })
        .collect();
    let field = shell.settings.app.controls.as_ref().map(|controls| {
        let state = match list {
            McpList::Hosts => &controls.hosts,
            McpList::Environments => &controls.environments,
        };
        Input::new(state)
            .id(gpui::ElementId::Name(format!("{name}-add").into()))
            .aria_label(placeholder)
            .disabled(!editable)
    });
    region(region_id)
        .role(Role::List)
        .flex()
        .flex_col()
        .children(rows)
        .child(
            div()
                .flex()
                .gap(Spacing::SM)
                .pt(Spacing::SM)
                .child(div().flex_1().min_w_0().children(field))
                .child(
                    Button::new(
                        gpui::ElementId::Name(format!("{name}-add-button").into()),
                        add_label,
                    )
                    .secondary()
                    .icon(ui::icon(IconName::Plus))
                    .disabled(!editable)
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.add_app_list_entry(list, window, cx);
                    })),
                ),
        )
        .into_any_element()
}

/// The overrides caption with its READ-ONLY badge and "Open config.toml", and
/// `overrides-table`: a row per `[mcp.workspaces."<id>"]` table.
fn overrides(mcp: &McpConfig, workspace: Option<&Workspace>, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let open_id = workspace.map(|workspace| workspace.manifest.id.to_string());
    let header_cell = |text: &'static str, width: Option<Pixels>| {
        div()
            .pl(GridMetrics::CELL_PADDING_X)
            .pr(GridMetrics::CELL_PADDING_X)
            .when_some(width, |cell, width| cell.flex_none().w(width))
            .when(width.is_none(), |cell| cell.flex_1())
            .child(text)
    };
    let header = div()
        .flex()
        .items_center()
        .h(GridMetrics::HEADER_HEIGHT)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(header_cell("workspace", Some(OVERRIDE_WORKSPACE_WIDTH)))
        .children(
            RULES
                .iter()
                .map(|(class, _, _)| header_cell(class.name(), Some(OVERRIDE_RULE_WIDTH))),
        )
        .child(header_cell("lists", None));
    let rows: Vec<_> = mcp
        .workspaces
        .iter()
        .map(|(key, level)| {
            let name = match (&open_id, workspace) {
                (Some(id), Some(workspace)) if id.eq_ignore_ascii_case(key) => {
                    format!("{} (open)", workspace.manifest.name)
                }
                _ => key.clone(),
            };
            override_row(key, &name, level, cx)
        })
        .collect();
    let empty = rows.is_empty().then(|| {
        ui::role(div(), TextVariant::Caption, cx)
            .py(Spacing::SM)
            .child("No workspace overrides.")
    });
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .pt(Spacing::LG)
                .pb(Spacing::XS)
                .child(ui::role(div(), TextVariant::Label, cx).child("WORKSPACE OVERRIDES"))
                .child(Badge::new("READ-ONLY", BadgeTone::Neutral).icon(ui::icon(IconName::Lock)))
                .child(div().flex_1())
                .child(
                    Button::new("overrides-open-config", "Open config.toml")
                        .secondary()
                        .inline()
                        .icon(ui::icon(IconName::ExternalLink))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.open_config_file(cx);
                        })),
                ),
        )
        .child(
            region("overrides-table")
                .role(Role::Table)
                .aria_label("Workspace overrides")
                .flex()
                .flex_col()
                .child(header)
                .children(rows)
                .children(empty),
        )
        .child(
            ui::role(div(), TextVariant::Caption, cx)
                .pt(Spacing::SM)
                .child(
                    "Each row is a [mcp.workspaces.\"<id>\"] table. Overrides can only \
                     restrict the rules above; \"\u{2013}\" keeps them.",
                ),
        )
}

/// One row of the overrides table, `overrides-row-<key>`, named by everything it shows.
fn override_row(key: &str, name: &str, level: &McpOverride, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let dash = "\u{2013}";
    let rules = [level.read, level.write, level.execute, level.destructive];
    let lists: Vec<String> = [
        ("hosts", &level.hosts),
        ("environments", &level.environments),
    ]
    .into_iter()
    .filter_map(|(label, list)| list.as_ref().map(|list| format!("{label} {}", list.len())))
    .collect();
    let lists = if lists.is_empty() {
        dash.to_owned()
    } else {
        lists.join(", ")
    };
    let spoken: Vec<String> = RULES
        .iter()
        .zip(rules)
        .map(|((class, _, _), rule)| format!("{} {}", class.name(), rule.map_or(dash, rule_id)))
        .collect();
    let label = format!("{name}: {}, {lists}", spoken.join(", "));
    let cell = |width: Option<Pixels>| {
        div()
            .pl(GridMetrics::CELL_PADDING_X)
            .pr(GridMetrics::CELL_PADDING_X)
            .overflow_hidden()
            .whitespace_nowrap()
            .when_some(width, |cell, width| cell.flex_none().w(width))
            .when(width.is_none(), |cell| cell.flex_1())
    };
    let rule_cell = |rule: Option<Rule>| {
        let color = match rule {
            None => colors.muted,
            Some(Rule::Allow) => colors.success,
            Some(Rule::Ask) => colors.warning,
            Some(Rule::Deny) => colors.danger,
        };
        cell(Some(OVERRIDE_RULE_WIDTH))
            .text_color(color)
            .child(rule.map_or(dash, rule_id))
    };
    labeled_row(format!("overrides-row-{key}"), label)
        .role(Role::Row)
        .h(GridMetrics::ROW_HEIGHT)
        .flex()
        .items_center()
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .child(
            cell(Some(OVERRIDE_WORKSPACE_WIDTH))
                .text_color(colors.strong)
                .child(name.to_owned()),
        )
        .children(rules.into_iter().map(rule_cell))
        .child(cell(None).text_color(colors.muted).child(lists))
        .into_any_element()
}
