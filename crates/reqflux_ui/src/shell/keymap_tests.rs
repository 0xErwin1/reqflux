//! The configurable keymap (v1-features S7, design/layouts/01 D3): the `[keymap]` table
//! rebinds the registry at startup and after every page edit, a capture writes
//! config.toml at once, and a conflict is reported before it is applied. Every test keeps
//! its config file under a temp `XDG_CONFIG_HOME` layout and never touches the user's
//! config; no test launches a program.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{TestAppContext, VisualTestContext};
use reqflux_engine::config::KeymapConfig;
use tempfile::TempDir;

use super::commands::{self, CommandId, effective_key};
use super::keymap::{apply_startup, resolve};
use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::variables_tests::variables_workspace;

/// A config file path as `$XDG_CONFIG_HOME/reqflux/config.toml` under a fresh temp
/// directory.
fn config_home() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    match reqflux_engine::config::config_path_in(Some(dir.path()), None) {
        Some(path) => (dir, path),
        None => panic!("no config path"),
    }
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => panic!("read {}: {error}", path.display()),
    }
}

fn loaded(path: &Path) -> reqflux_engine::config::UserConfig {
    match reqflux_engine::config::load(path) {
        Ok(config) => config,
        Err(error) => panic!("load: {error}"),
    }
}

fn id_of(slug: &str) -> CommandId {
    match commands::COMMANDS
        .iter()
        .find(|command| command.name == slug)
        .map(|command| command.id)
    {
        Some(id) => id,
        None => panic!("no command {slug}"),
    }
}

/// Renders the shell over `config_path`, as the App section tests do: no test touches the
/// user's config and nothing launches. The keystroke interceptor that serves captures is
/// installed, as the app does at startup.
fn render_app<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    config_path: &Path,
) -> (gpui::Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = render(cx, dir, state_path);
    cx.update(|_, cx| apply_startup(None, cx));
    shell.update(cx, |shell, _| {
        shell.set_config(
            Some(config_path.to_owned()),
            crate::theme::ConfigReport::default(),
        );
    });
    (shell, cx)
}

/// Opens the app settings at App > Keymap.
fn open_keymap(cx: &mut VisualTestContext, shell: &gpui::Entity<Shell>) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_app_settings(window, cx)));
    cx.run_until_parked();
    click(cx, "nav-keymap");
}

#[test]
fn the_default_keys_still_work_without_a_keymap_table() {
    let (dir, path) = config_home();
    let (resolved, warnings) = resolve(&path, &KeymapConfig::default());
    assert!(warnings.is_empty());
    for command in commands::COMMANDS {
        let entry = resolved
            .iter()
            .find(|entry| entry.id == command.id)
            .unwrap_or_else(|| panic!("{} is unresolved", command.name));
        assert_eq!(entry.key, Some(command.key.to_owned()), "{}", command.name);
    }
    assert!(read(&path).is_empty(), "resolving never writes");
    drop(dir);
}

#[test]
fn an_unknown_slug_and_an_unbindable_chord_warn_and_change_nothing() {
    let (dir, path) = config_home();
    let text = "[keymap]\n\"nope\" = \"ctrl-alt-9\"\n\"save-request\" = \"ctrl-a-b\"\n";
    write(&path, text);
    let (resolved, warnings) = resolve(&path, &loaded(&path).keymap);
    let said = warnings.join(" ");
    assert!(said.contains("nope"), "{said}");
    assert!(said.contains("unknown command"), "{said}");
    assert!(said.contains("ctrl-a-b"), "{said}");
    let save = resolved
        .iter()
        .find(|entry| entry.id == id_of("save-request"))
        .unwrap_or_else(|| panic!("save-request is unresolved"));
    assert_eq!(save.key, Some("ctrl-s".to_owned()));
    assert_eq!(read(&path), text, "resolving never writes");
    drop(dir);
}

#[test]
fn the_first_command_in_registry_order_wins_a_conflict() {
    let (dir, path) = config_home();
    // Close tab is after save request in the registry, so the file binds both to Ctrl+W
    // and the first one keeps the chord.
    let text = "[keymap]\n\"save-request\" = \"ctrl-w\"\n\"close-tab\" = \"ctrl-w\"\n";
    write(&path, text);
    let (resolved, warnings) = resolve(&path, &loaded(&path).keymap);
    let save = resolved
        .iter()
        .find(|entry| entry.id == id_of("save-request"))
        .unwrap_or_else(|| panic!("save-request is unresolved"));
    assert_eq!(save.key, Some("ctrl-w".to_owned()));
    let close = resolved
        .iter()
        .find(|entry| entry.id == id_of("close-tab"))
        .unwrap_or_else(|| panic!("close-tab is unresolved"));
    assert_eq!(close.key, None, "the default is taken too");
    let said = warnings.join(" ");
    assert!(said.contains("Save request"), "{said}");
    assert!(said.contains("close-tab"), "{said}");
    assert!(said.contains("ctrl-w"), "{said}");
    assert_eq!(read(&path), text, "resolving never writes");
    drop(dir);
}

#[gpui::test]
fn the_keymap_table_rebinds_commands_at_startup(cx: &mut TestAppContext) {
    let (dir, path) = config_home();
    write(
        &path,
        "[keymap]\n\"toggle-theme\" = \"ctrl-alt-t\"\n\"nope\" = \"ctrl-alt-9\"\n\"save-request\" = \"ctrl-a-b\"\n",
    );
    let warnings = cx.update(|cx| apply_startup(Some(&path), cx));
    let said = warnings.join(" ");
    assert!(said.contains("nope"), "{said}");
    assert!(said.contains("ctrl-a-b"), "{said}");
    assert_eq!(
        cx.update(|cx| effective_key(cx, id_of("toggle-theme"))),
        "ctrl-alt-t"
    );
    assert_eq!(
        cx.update(|cx| effective_key(cx, id_of("save-request"))),
        "ctrl-s",
        "an unbindable chord keeps the default"
    );
    // The new chord runs the command and the old one stops firing.
    let bound = |cx: &mut TestAppContext, chord: &str| {
        cx.update(|cx| {
            let keystroke = match gpui::Keystroke::parse(chord) {
                Ok(keystroke) => keystroke,
                Err(_) => panic!("chord {chord}"),
            };
            cx.key_bindings()
                .borrow()
                .all_bindings_for_input(&[keystroke])
                .iter()
                .any(|binding| binding.action().name() == "shell::ToggleTheme")
        })
    };
    assert!(bound(cx, "ctrl-alt-t"));
    assert!(!bound(cx, "ctrl-shift-t"));
    drop(dir);
}

#[gpui::test]
fn the_default_keys_still_work_after_startup_without_a_table(cx: &mut TestAppContext) {
    let (dir, path) = config_home();
    write(&path, "theme = \"dark\"\n");
    let warnings = cx.update(|cx| apply_startup(Some(&path), cx));
    assert!(warnings.is_empty());
    for command in commands::COMMANDS {
        assert_eq!(
            cx.update(|cx| effective_key(cx, command.id)),
            command.key,
            "{}",
            command.name
        );
    }
    drop(dir);
}

#[gpui::test]
fn a_capture_writes_config_toml_at_once(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_app(cx, &dir, &state_path, &config_path);
    open_keymap(cx, &shell);
    present(cx, "keymap-rows");
    present(cx, "keymap-row-save-request");

    click(cx, "keymap-key-save-request");
    present(cx, "keymap-capture-hint");
    cx.simulate_keystrokes("ctrl-alt-s");
    cx.run_until_parked();
    assert_eq!(
        read(&config_path),
        "[keymap]\nsave-request = \"ctrl-alt-s\"\n",
        "the rebind writes the file at once"
    );
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("save-request"))),
        "ctrl-alt-s"
    );
    assert!(bounds(cx, "keymap-conflict").is_none());

    // A second chord of the same command replaces the first.
    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("ctrl-shift-s");
    cx.run_until_parked();
    assert_eq!(
        read(&config_path),
        "[keymap]\nsave-request = \"ctrl-shift-s\"\n"
    );
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("save-request"))),
        "ctrl-shift-s"
    );
}

#[gpui::test]
fn escape_cancels_a_capture_and_delete_clears_the_binding(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let mine = "theme = \"dark\"\n";
    write(&config_path, mine);
    let (shell, cx) = render_app(cx, &dir, &state_path, &config_path);
    open_keymap(cx, &shell);

    click(cx, "keymap-key-save-request");
    present(cx, "keymap-capture-hint");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "keymap-capture-hint").is_none());
    assert_eq!(read(&config_path), mine, "a cancel changes nothing");

    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert_eq!(read(&config_path), mine, "nothing to clear yet");
}

#[gpui::test]
fn a_conflict_is_reported_before_it_is_applied(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_app(cx, &dir, &state_path, &config_path);
    open_keymap(cx, &shell);

    // Ctrl+W is Close tab's default; taking it reports the command it hits first.
    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("ctrl-w");
    cx.run_until_parked();
    present(cx, "keymap-conflict");
    assert!(
        !read(&config_path).contains("[keymap]"),
        "a conflict writes nothing"
    );
    click(cx, "keymap-cancel");
    cx.run_until_parked();
    assert!(bounds(cx, "keymap-conflict").is_none());
    assert!(
        !read(&config_path).contains("[keymap]"),
        "a cancel writes nothing"
    );

    // Keep mine takes the chord away from the other command.
    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("ctrl-w");
    cx.run_until_parked();
    click(cx, "keymap-keep");
    cx.run_until_parked();
    assert_eq!(read(&config_path), "[keymap]\nsave-request = \"ctrl-w\"\n");
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("save-request"))),
        "ctrl-w"
    );
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("close-tab"))),
        "",
        "the other command loses the chord"
    );
}

#[gpui::test]
fn clear_and_reset_return_the_defaults(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_app(cx, &dir, &state_path, &config_path);
    open_keymap(cx, &shell);
    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("ctrl-alt-s");
    cx.run_until_parked();

    click(cx, "keymap-clear-save-request");
    cx.run_until_parked();
    assert_eq!(read(&config_path), "", "clear removes the last binding");
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("save-request"))),
        "ctrl-s"
    );

    click(cx, "keymap-key-save-request");
    cx.simulate_keystrokes("ctrl-alt-s");
    cx.run_until_parked();
    click(cx, "keymap-reset");
    cx.run_until_parked();
    assert_eq!(read(&config_path), "", "reset removes the table");
    assert_eq!(
        cx.update(|_, cx| effective_key(cx, id_of("save-request"))),
        "ctrl-s"
    );
}

/// Whether the key capture of the Keymap page (bolt's `KeyCapture`) is listening.
fn field_capturing(cx: &mut VisualTestContext, shell: &gpui::Entity<Shell>) -> bool {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .settings
            .keymap
            .field
            .as_ref()
            .is_some_and(|field| field.read(cx).is_capturing())
    })
}

#[gpui::test]
fn the_key_cell_captures_through_bolts_key_capture(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let mine = "theme = \"dark\"\n";
    write(&config_path, mine);
    let (shell, cx) = render_app(cx, &dir, &state_path, &config_path);
    open_keymap(cx, &shell);

    click(cx, "keymap-key-save-request");
    present(cx, "keymap-capture-hint");
    assert!(field_capturing(cx, &shell), "the click starts the capture");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!field_capturing(cx, &shell), "escape ends the capture");
    assert!(bounds(cx, "keymap-capture-hint").is_none());
    assert_eq!(read(&config_path), mine, "a cancel changes nothing");

    click(cx, "keymap-key-save-request");
    assert!(field_capturing(cx, &shell));
    // A bare letter is typing, not a chord: the capture keeps listening.
    cx.simulate_keystrokes("a");
    cx.run_until_parked();
    assert!(field_capturing(cx, &shell), "a bare letter keeps listening");
    cx.simulate_keystrokes("ctrl-alt-s");
    cx.run_until_parked();
    assert!(!field_capturing(cx, &shell));
    assert_eq!(
        read(&config_path),
        "theme = \"dark\"\n\n[keymap]\nsave-request = \"ctrl-alt-s\"\n"
    );
    present(cx, "keymap-key-save-request");
    let row = cx.update(|_, cx| effective_key(cx, id_of("save-request")));
    assert_eq!(row, "ctrl-alt-s", "the row shows the new key");
}
