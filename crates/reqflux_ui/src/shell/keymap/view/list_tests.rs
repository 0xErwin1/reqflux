//! The App > Keymap page as DBFlux's Keyboard shortcuts page (feedback-nov3 S15): a filter,
//! a context select, one keycap per chord part, a pencil that records, rows grouped by
//! context, and a `[keymap]` block that stays small. Every test keeps its config file under a
//! temp directory.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::{chord_parts, groups};
use crate::shell::Shell;
use crate::shell::commands::COMMANDS;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::keymap::apply_startup;
use crate::shell::send_tests::closed_port;
use crate::shell::variables_tests::variables_workspace;

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

fn open_keymap<'a>(
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
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_app_settings(window, cx)));
    cx.run_until_parked();
    click(cx, "nav-keymap");
    (shell, cx)
}

fn type_filter(cx: &mut VisualTestContext, text: &str) {
    click(cx, "keymap-filter");
    cx.simulate_input(text);
    cx.run_until_parked();
}

#[test]
fn a_chord_is_one_keycap_per_part() {
    assert_eq!(chord_parts("ctrl-s"), ["Ctrl", "S"]);
    assert_eq!(chord_parts("ctrl-shift-tab"), ["Ctrl", "Shift", "Tab"]);
    assert_eq!(chord_parts("escape"), ["Esc"]);
    assert_eq!(chord_parts("ctrl--"), ["Ctrl", "-"]);
    assert!(chord_parts("").is_empty());
}

#[test]
fn the_filter_and_the_context_narrow_the_groups() {
    let key = |command: &crate::shell::commands::Command| command.key.to_owned();
    let all = groups("", None, &key);
    let shown: usize = all.iter().map(|group| group.commands.len()).sum();
    assert_eq!(shown, COMMANDS.len());
    assert!(
        all.iter()
            .all(|group| group.commands.iter().all(|c| c.context == group.context))
    );

    let revert = groups("revert", None, &key);
    assert!(
        revert
            .iter()
            .flat_map(|group| &group.commands)
            .all(|command| command.name.contains("revert")
                || command.label.to_lowercase().contains("revert"))
    );
    assert!(!revert.is_empty());

    let by_key = groups("ctrl+shift+r", None, &key);
    assert!(
        by_key
            .iter()
            .flat_map(|group| &group.commands)
            .any(|command| command.name == "revert-request")
    );
    assert!(groups("zzzz-no-such-command", None, &key).is_empty());
    assert!(groups("", Some("No such context"), &key).is_empty());
}

#[gpui::test]
fn typing_in_the_filter_shows_only_the_matching_rows(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx) = open_keymap(cx, &dir, &state_path, &config_path);
    present(cx, "keymap-filter");
    present(cx, "keymap-context");
    present(cx, "keymap-row-send-request");
    present(cx, "keymap-row-revert-request");

    type_filter(cx, "revert");
    present(cx, "keymap-row-revert-request");
    assert!(bounds(cx, "keymap-row-send-request").is_none());
}

#[gpui::test]
fn rows_are_grouped_by_context_and_a_group_collapses(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx) = open_keymap(cx, &dir, &state_path, &config_path);
    present(cx, "keymap-group-shell");
    present(cx, "keymap-row-send-request");

    click(cx, "keymap-group-shell");
    present(cx, "keymap-group-shell");
    assert!(bounds(cx, "keymap-row-send-request").is_none());

    click(cx, "keymap-group-shell");
    present(cx, "keymap-row-send-request");
}

#[gpui::test]
fn a_row_shows_its_chord_as_keycaps_and_a_pencil_that_records(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx) = open_keymap(cx, &dir, &state_path, &config_path);
    present(cx, "keymap-chord-save-request");

    click(cx, "keymap-record-save-request");
    present(cx, "keymap-capture-hint");
    cx.simulate_keystrokes("ctrl-alt-s");
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(&config_path).ok().as_deref(),
        Some("[keymap]\nsave-request = \"ctrl-alt-s\"\n")
    );
}

#[gpui::test]
fn without_custom_bindings_there_is_no_block_to_open(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx) = open_keymap(cx, &dir, &state_path, &config_path);
    assert!(bounds(cx, "keymap-config").is_none());
    assert!(bounds(cx, "keymap-config-toggle").is_none());
    present(cx, "keymap-config-empty");
}

#[gpui::test]
fn a_long_keymap_table_opens_in_a_bounded_block(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let mut text = String::from("[keymap]\n");
    for n in 0..40 {
        text.push_str(&format!("\"extra-{n}\" = \"ctrl-alt-{}\"\n", n % 10));
    }
    write(&config_path, &text);
    let (_shell, cx) = open_keymap(cx, &dir, &state_path, &config_path);
    assert!(
        bounds(cx, "keymap-config").is_none(),
        "collapsed by default"
    );

    click(cx, "keymap-config-toggle");
    let block = present(cx, "keymap-config");
    assert!(
        block.size.height <= gpui::px(200.0),
        "the block is bounded, got {:?}",
        block.size.height
    );
    click(cx, "keymap-config-toggle");
    assert!(bounds(cx, "keymap-config").is_none());
}
