//! The state and the pure parts of the Keymap page's list (feedback-nov3 S15): the filter
//! field, the context select, the groups the rows sit in and the keycaps of a chord. The
//! page draws them in `keymap/view`.

use std::collections::BTreeSet;

use bolt::controls::{Dropdown, DropdownItem, DropdownSelectionChanged, InputEvent, InputState};
use gpui::{App, AppContext as _, Context, Entity, Subscription, Window};

use crate::shell::Shell;
use crate::shell::commands::{COMMANDS, Command, key_label};

/// The context select's first item: no context narrows the list.
pub(in crate::shell) const ALL_CONTEXTS: &str = "All contexts";

/// The filter, the context select and what the list has folded or opened.
pub(in crate::shell) struct ListState {
    pub(super) filter: Entity<InputState>,
    pub(super) context: Entity<Dropdown>,
    /// The contexts the select lists after [`ALL_CONTEXTS`].
    contexts: Vec<&'static str>,
    /// The context the select narrows to.
    selected: Option<&'static str>,
    /// The groups folded by their header; a filter or a context opens them all.
    collapsed: BTreeSet<&'static str>,
    /// Whether the `[keymap]` block of config.toml is open.
    pub(super) config_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl ListState {
    /// The filter text, trimmed.
    pub(super) fn filter_text(&self, cx: &App) -> String {
        self.filter.read(cx).value().trim().to_owned()
    }

    /// The context the list is narrowed to.
    pub(super) fn context(&self) -> Option<&'static str> {
        self.selected
    }

    /// Whether the group of `context` shows its rows: always while narrowed by a filter or a
    /// context, else unless its header folded it.
    pub(super) fn is_open(&self, context: &'static str, narrowed: bool) -> bool {
        narrowed || !self.collapsed.contains(context)
    }
}

/// One context and the commands of it that pass the filter, in registry order.
pub(in crate::shell) struct Group {
    pub(in crate::shell) context: &'static str,
    pub(in crate::shell) commands: Vec<&'static Command>,
}

/// The id of a context in the page's element ids: `Shell` is `shell`.
pub(super) fn group_id(context: &str) -> String {
    context
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// The registry's commands that pass `filter` (a case-insensitive part of the label, the
/// slug or the key as the page prints it, `Ctrl+Shift+R`) and `context`, grouped by context
/// in the order the registry first names them. `key` gives a command's effective key; a
/// group with no command left is dropped.
pub(in crate::shell) fn groups(
    filter: &str,
    context: Option<&str>,
    key: &dyn Fn(&Command) -> String,
) -> Vec<Group> {
    let needle = filter.trim().to_lowercase();
    let mut groups: Vec<Group> = Vec::new();
    for command in COMMANDS {
        if context.is_some_and(|context| context != command.context) {
            continue;
        }
        let printed = key_label(&key(command)).to_lowercase();
        let matches = needle.is_empty()
            || command.label.to_lowercase().contains(&needle)
            || command.name.to_lowercase().contains(&needle)
            || printed.contains(&needle);
        if !matches {
            continue;
        }
        match groups
            .iter_mut()
            .find(|group| group.context == command.context)
        {
            Some(group) => group.commands.push(command),
            None => groups.push(Group {
                context: command.context,
                commands: vec![command],
            }),
        }
    }
    groups
}

/// The contexts the registry names, in the order it first names them.
fn contexts() -> Vec<&'static str> {
    let mut contexts: Vec<&'static str> = Vec::new();
    for command in COMMANDS {
        if !contexts.contains(&command.context) {
            contexts.push(command.context);
        }
    }
    contexts
}

/// The keycaps of `key`, one per part: `ctrl-shift-tab` is `Ctrl`, `Shift`, `Tab`. A key
/// that is a dash (`ctrl--`) is the part `-`.
pub(in crate::shell) fn chord_parts(key: &str) -> Vec<String> {
    let mut parts: Vec<&str> = key.split('-').collect();
    let dash = key.ends_with('-');
    while parts.last().is_some_and(|part| part.is_empty()) {
        parts.pop();
    }
    let mut labels: Vec<String> = parts.into_iter().map(key_label).collect();
    if dash {
        labels.push("-".to_owned());
    }
    labels
}

impl Shell {
    /// Makes the filter and the context select the first time the Keymap page shows.
    pub(crate) fn ensure_keymap_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.keymap_list.is_some() {
            return;
        }
        let contexts = contexts();
        let items = std::iter::once(DropdownItem::new(ALL_CONTEXTS))
            .chain(contexts.iter().map(|context| DropdownItem::new(*context)))
            .collect();
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter by command or key"));
        let context = cx.new(|_| {
            Dropdown::new("keymap-context")
                .items(items)
                .selected_index(Some(0))
                .aria_label("Context")
        });
        let typing = cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let choosing = cx.subscribe_in(
            &context,
            window,
            |this, _, event: &DropdownSelectionChanged, _window, cx| {
                if let Some(list) = this.settings.keymap_list.as_mut() {
                    list.selected = event
                        .index
                        .checked_sub(1)
                        .and_then(|index| list.contexts.get(index).copied());
                    cx.notify();
                }
            },
        );
        self.settings.keymap_list = Some(ListState {
            filter,
            context,
            contexts,
            selected: None,
            collapsed: BTreeSet::new(),
            config_open: false,
            _subscriptions: vec![typing, choosing],
        });
    }

    /// A click on a group's header: folds it, or opens it again.
    pub(in crate::shell) fn toggle_keymap_group(
        &mut self,
        context: &'static str,
        cx: &mut Context<Self>,
    ) {
        if let Some(list) = self.settings.keymap_list.as_mut()
            && !list.collapsed.remove(context)
        {
            list.collapsed.insert(context);
        }
        cx.notify();
    }

    /// The `[keymap]` block's header: opens the block, or closes it.
    pub(in crate::shell) fn toggle_keymap_config(&mut self, cx: &mut Context<Self>) {
        if let Some(list) = self.settings.keymap_list.as_mut() {
            list.config_open = !list.config_open;
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
