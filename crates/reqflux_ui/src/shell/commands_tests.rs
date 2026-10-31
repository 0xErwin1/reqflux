//! The keyboard coverage test (command-palette S6) and the key labels.

use std::collections::HashSet;

use super::{COMMANDS, PALETTE_KEYS, key_label};
use crate::shell::send::SHELL_CONTEXT;

/// The palette commands of command-palette S4, by stable id.
const EXPECTED: [&str; 19] = [
    "send-request",
    "cancel-send",
    "save-request",
    "revert-request",
    "new-request",
    "new-collection",
    "new-environment",
    "switch-environment",
    "open-workspace",
    "new-workspace",
    "close-workspace",
    "close-tab",
    "next-tab",
    "previous-tab",
    "show-collections",
    "show-history",
    "show-variables",
    "toggle-theme",
    "open-approvals",
];

#[test]
fn every_spec_command_is_in_the_registry() {
    let names: Vec<_> = COMMANDS.iter().map(|command| command.name).collect();
    assert_eq!(names, EXPECTED);
}

#[test]
fn every_command_has_a_key_and_no_two_share_one_in_a_context() {
    let mut seen = HashSet::new();
    for command in COMMANDS {
        assert!(!command.key.is_empty(), "{} has no key", command.name);
        assert!(!command.label.is_empty(), "{} has no label", command.name);
        assert!(
            seen.insert((command.context, command.key)),
            "{} shares {} in {}",
            command.name,
            command.key,
            command.context
        );
    }
    for key in PALETTE_KEYS {
        assert!(
            seen.insert((SHELL_CONTEXT, key)),
            "the palette key {key} is taken by a command"
        );
    }
}

#[test]
fn ids_are_unique() {
    let names: HashSet<_> = COMMANDS.iter().map(|command| command.name).collect();
    assert_eq!(names.len(), COMMANDS.len());
}

#[test]
fn the_spec_keys_are_the_defaults() {
    let key = |name: &str| {
        COMMANDS
            .iter()
            .find(|command| command.name == name)
            .map(|command| command.key)
    };
    assert_eq!(key("close-tab"), Some("ctrl-w"));
    assert_eq!(key("next-tab"), Some("ctrl-tab"));
    assert_eq!(key("previous-tab"), Some("ctrl-shift-tab"));
    assert_eq!(key("new-request"), Some("ctrl-n"));
    assert_eq!(key("new-collection"), Some("ctrl-shift-n"));
    assert_eq!(key("show-collections"), Some("ctrl-1"));
    assert_eq!(key("show-history"), Some("ctrl-2"));
    assert_eq!(key("show-variables"), Some("ctrl-3"));
    assert_eq!(key("switch-environment"), Some("ctrl-e"));
    assert_eq!(key("open-workspace"), Some("ctrl-o"));
    assert_eq!(key("send-request"), Some("ctrl-enter"));
    assert_eq!(key("cancel-send"), Some("escape"));
    assert_eq!(key("save-request"), Some("ctrl-s"));
    assert_eq!(key("toggle-theme"), Some("ctrl-shift-t"));
    assert_eq!(key("open-approvals"), Some("ctrl-shift-m"));
}

#[test]
fn keys_print_as_the_palette_shows_them() {
    assert_eq!(key_label("ctrl-shift-tab"), "Ctrl+Shift+Tab");
    assert_eq!(key_label("ctrl-enter"), "Ctrl+Enter");
    assert_eq!(key_label("escape"), "Esc");
    assert_eq!(key_label("ctrl-1"), "Ctrl+1");
    assert_eq!(key_label("ctrl-w"), "Ctrl+W");
}

#[test]
fn every_area_holds_its_commands_in_palette_order() {
    use super::Area;
    let names = |area: Area| -> Vec<&str> {
        COMMANDS
            .iter()
            .filter(|command| command.area == area)
            .map(|command| command.name)
            .collect()
    };
    assert_eq!(
        names(Area::Environment),
        ["new-environment", "switch-environment"]
    );
    assert_eq!(names(Area::Tabs), ["close-tab", "next-tab", "previous-tab"]);
    assert_eq!(names(Area::Agents), ["open-approvals"]);
    assert_eq!(
        names(Area::Workspace),
        ["open-workspace", "new-workspace", "close-workspace"]
    );
}
