//! The App section of the settings page (app-pending S3, app-settings/02): the rail's
//! Settings opens it on App > General; theme, interface font size, the four MCP rules and
//! the host and environment lists write `config.toml` at once with the format-preserving
//! editor; workspace overrides are read-only; a file that does not parse leaves the section
//! read-only with Open file and Reload. Every test keeps its config file under a temp
//! `XDG_CONFIG_HOME` layout and never launches a program.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::config::{self, Rule, UserConfig};
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::toasts::tests as toasts;
use crate::shell::variables_tests::{DEV, variables_workspace};
use crate::theme::{ConfigReport, PaletteMode};

/// The id of the workspace `variables_workspace` writes.
const WORKSPACE_ID: &str = "01J00000000000000000000900";
const OTHER_ID: &str = "01J0000000000000000000000W";

/// A config file path as `$XDG_CONFIG_HOME/reqflux/config.toml` under a fresh temp
/// directory.
fn config_home() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = match config::config_path_in(Some(dir.path()), None) {
        Some(path) => path,
        None => panic!("no config path"),
    };
    (dir, path)
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create {}: {error}", parent.display());
    }
    if let Err(error) = fs::write(path, text) {
        panic!("write {}: {error}", path.display());
    }
}

fn read(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read {}: {error}", path.display()),
    }
}

fn loaded(path: &Path) -> UserConfig {
    match config::load(path) {
        Ok(config) => config,
        Err(error) => panic!("load: {error}"),
    }
}

fn automation(cx: &mut VisualTestContext) -> gpui_mcp::Automation {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    automation
}

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

/// The files the OS handler was asked to open.
type Opened = Rc<RefCell<Vec<PathBuf>>>;

/// Renders the shell over `config_path` with a recording file opener, so nothing launches.
fn render_app<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    config_path: &Path,
) -> (Entity<Shell>, &'a mut VisualTestContext, Opened) {
    let (shell, cx) = render(cx, dir, state_path);
    let opened: Opened = Rc::default();
    let record = opened.clone();
    cx.update(|_, cx| {
        crate::open_file::set_opener(
            cx,
            Rc::new(move |path: &Path| {
                record.borrow_mut().push(path.to_owned());
                Ok(())
            }),
        );
    });
    let path = config_path.to_owned();
    shell.update(cx, |shell, _| {
        shell.set_config(Some(path), ConfigReport::default());
    });
    (shell, cx, opened)
}

fn open_app(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_app_settings(window, cx)));
    cx.run_until_parked();
}

fn type_text(cx: &mut VisualTestContext, field: &str, text: &str) {
    click(cx, field);
    cx.simulate_input(text);
    cx.run_until_parked();
}

#[gpui::test]
fn the_rail_settings_button_opens_app_general(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    let automation = automation(cx);
    assert!(bounds(cx, "settings-page").is_none());

    click(cx, "rail-settings");
    present(cx, "settings-page");
    assert_eq!(
        label(&automation, "settings-heading").as_deref(),
        Some("General")
    );
    assert_eq!(
        label(&automation, "config-path"),
        Some(config_path.display().to_string())
    );
    let general = present(cx, "nav-general");
    let policy = present(cx, "nav-mcp-policy");
    let globals = present(cx, "variables-scope-globals");
    assert!(general.origin.y < policy.origin.y);
    assert!(
        policy.origin.y < globals.origin.y,
        "App sits above Workspace"
    );
    present(cx, "theme");
    present(cx, "font-size");
    assert!(bounds(cx, "config-error").is_none());
}

#[gpui::test]
fn the_theme_writes_config_toml_at_once_and_applies(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let mine = "# Mine.\ntheme = \"dark\" # set by the app\nzeta = 1\n";
    write(&config_path, mine);
    let (shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);

    click(cx, "theme-light");
    assert_eq!(read(&config_path), mine.replace("\"dark\"", "\"light\""));
    assert_eq!(
        cx.update(|_, cx| crate::theme::mode(cx)),
        PaletteMode::Light
    );

    click(cx, "theme-system");
    assert_eq!(loaded(&config_path).theme, Some(config::ThemeMode::System));
    assert_eq!(
        cx.update(|_, cx| crate::theme::mode(cx)),
        PaletteMode::System
    );
}

#[gpui::test]
fn the_font_size_writes_config_toml_and_resizes_the_interface(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    let dropdown = cx.update(|_, cx| shell.read(cx).settings.app.font_size_control().cloned());
    let Some(dropdown) = dropdown else {
        panic!("the font size control is missing");
    };
    let shown = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            dropdown
                .read(cx)
                .selected_label()
                .map(|label| label.to_string())
        })
    };
    assert_eq!(shown(cx).as_deref(), Some("13 px"));

    click(cx, "font-size");
    cx.update(|_, cx| {
        dropdown.update(cx, |dropdown, cx| {
            dropdown.select_next_item(cx);
            dropdown.accept_selection(cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(read(&config_path), "font_size = 14\n");
    assert_eq!(cx.update(|_, cx| bolt::fonts::current(cx).ui_size), 14.0);
    assert_eq!(shown(cx).as_deref(), Some("14 px"));
}

#[gpui::test]
fn each_rule_writes_its_key_in_mcp(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    write(&config_path, "[mcp]\n# Mine.\nwrite = \"ask\"\n");
    let (shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    let automation = automation(cx);
    click(cx, "nav-mcp-policy");
    assert_eq!(
        label(&automation, "settings-heading").as_deref(),
        Some("MCP policy")
    );

    click(cx, "rule-read-deny");
    click(cx, "rule-write-allow");
    click(cx, "rule-execute-deny");
    click(cx, "rule-destructive-allow");
    let mcp = loaded(&config_path).mcp;
    assert_eq!(
        (mcp.read, mcp.write, mcp.execute, mcp.destructive),
        (
            Some(Rule::Deny),
            Some(Rule::Allow),
            Some(Rule::Deny),
            Some(Rule::Allow)
        )
    );
    assert!(read(&config_path).contains("# Mine.\nwrite = \"allow\"\n"));
}

#[gpui::test]
fn hosts_and_environments_are_added_and_removed(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    click(cx, "nav-mcp-policy");
    present(cx, "hosts-list");
    present(cx, "environments-list");

    type_text(cx, "hosts-add", "api.example.com");
    click(cx, "hosts-add-button");
    type_text(cx, "hosts-add", "localhost:8080");
    click(cx, "hosts-add-button");
    type_text(cx, "environments-add", "dev");
    click(cx, "environments-add-button");
    let mcp = loaded(&config_path).mcp;
    assert_eq!(
        mcp.hosts,
        Some(vec![
            "api.example.com".to_owned(),
            "localhost:8080".to_owned()
        ])
    );
    assert_eq!(mcp.environments, Some(vec!["dev".to_owned()]));
    present(cx, "hosts-row-1");
    present(cx, "environments-row-0");

    click(cx, "hosts-remove-0");
    click(cx, "environments-remove-0");
    let mcp = loaded(&config_path).mcp;
    assert_eq!(mcp.hosts, Some(vec!["localhost:8080".to_owned()]));
    assert_eq!(mcp.environments, None, "no list is any environment");
    assert!(bounds(cx, "environments-row-0").is_none());
}

#[gpui::test]
fn workspace_overrides_are_read_only_and_open_config_toml(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let text = format!(
        "[mcp.workspaces.\"{WORKSPACE_ID}\"]\nwrite = \"deny\"\nexecute = \"ask\"\nhosts = [\"a\"]\n\n\
         [mcp.workspaces.\"{OTHER_ID}\"]\nexecute = \"deny\"\n"
    );
    write(&config_path, &text);
    let (shell, cx, opened) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    let automation = automation(cx);
    click(cx, "nav-mcp-policy");
    present(cx, "overrides-table");
    assert_eq!(
        label(&automation, &format!("overrides-row-{WORKSPACE_ID}")).as_deref(),
        Some("Demo (open): read –, write deny, execute ask, destructive –, hosts 1")
    );
    assert_eq!(
        label(&automation, &format!("overrides-row-{OTHER_ID}")),
        Some(format!(
            "{OTHER_ID}: read –, write –, execute deny, destructive –, –"
        ))
    );

    click(cx, "overrides-open-config");
    assert_eq!(*opened.borrow(), vec![config_path.clone()]);
    assert_eq!(read(&config_path), text, "opening never writes");
}

#[gpui::test]
fn a_file_that_does_not_parse_is_read_only_until_reloaded(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let broken = "theme = \"light\"\ntoken = sk-live-SECRET123\n";
    write(&config_path, broken);
    let (shell, cx, opened) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    let automation = automation(cx);

    present(cx, "config-error");
    let shown = label(&automation, "config-error").unwrap_or_default();
    assert!(shown.contains("line 2"), "{shown}");
    assert!(!shown.contains("SECRET123"), "{shown}");
    click(cx, "theme-dark");
    click(cx, "nav-mcp-policy");
    click(cx, "rule-read-deny");
    assert_eq!(read(&config_path), broken, "nothing overwrites the file");

    click(cx, "nav-general");
    click(cx, "config-open");
    assert_eq!(*opened.borrow(), vec![config_path.clone()]);

    write(&config_path, "theme = \"light\"\nfont_size = 15\n");
    click(cx, "config-reload");
    assert!(bounds(cx, "config-error").is_none());
    assert_eq!(
        cx.update(|_, cx| crate::theme::mode(cx)),
        PaletteMode::Light
    );
    assert_eq!(cx.update(|_, cx| bolt::fonts::current(cx).ui_size), 15.0);
    click(cx, "theme-dark");
    assert_eq!(loaded(&config_path).theme, Some(config::ThemeMode::Dark));
}

#[gpui::test]
fn the_workspace_sections_still_work_beside_the_app_section(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx, _) = render_app(cx, &dir, &state_path, &config_path);
    open_app(cx, &shell);
    let automation = automation(cx);

    click(cx, "settings-nav-environments");
    assert_eq!(
        label(&automation, "settings-heading").as_deref(),
        Some("Environments")
    );
    assert_eq!(
        label(&automation, "settings-title").as_deref(),
        Some("Workspace settings")
    );
    present(cx, &format!("variables-scope-environment-{DEV}"));
    present(cx, "variables-new-environment");
    click(cx, "variables-scope-globals");
    present(cx, "variables-row-1");
    assert!(bounds(cx, "theme").is_none());

    click(cx, "nav-general");
    present(cx, "theme");
    click(cx, "settings-close");
    assert!(bounds(cx, "settings-page").is_none());
}

#[gpui::test]
fn a_file_the_os_cannot_open_shows_an_error_toast(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    write(&config_path, "theme = \"dark\"\n");
    let (shell, cx, _opened) = render_app(cx, &dir, &state_path, &config_path);
    cx.update(|_, cx| {
        crate::open_file::set_opener(
            cx,
            Rc::new(|_: &Path| Err(std::io::Error::other("no program opens .toml files"))),
        );
    });
    open_app(cx, &shell);
    click(cx, "nav-mcp-policy");

    click(cx, "overrides-open-config");
    assert_eq!(
        toasts::pushed(cx),
        vec![toasts::error(
            "Cannot open config.toml",
            "no program opens .toml files"
        )]
    );
    assert_eq!(toasts::shown_toasts(cx), vec!["toast-1".to_owned()]);
    assert_eq!(
        toasts::tree_label(cx, "toast-toggle-1").as_deref(),
        Some("Show details")
    );
}
