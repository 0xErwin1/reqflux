//! The window's commands (command-palette S4, S5): one registry the palette lists, the
//! keymap binds and the coverage test checks, so the three cannot drift. Each command has
//! an area the palette groups it under and, when it cannot run, a reason the palette shows.
//!
//! Every command has an action bound to its default key in its key context. A key runs the
//! command when it can run now; the palette lists it dimmed and inert when it cannot.
//! Escape always reaches [`Shell::cancel_active`], which closes an open menu first.

use gpui::{App, Context, Div, InteractiveElement, KeyBinding, Window, actions};

use super::Shell;
use super::send::SHELL_CONTEXT;
use super::settings_page::Section;
use super::sidebar_edit::TreeCommand;
use super::sidebar_history::Panel;
use super::tree_model::NodeKind;
use crate::workspace::OpenedWorkspace;

/// One command: what the palette shows and where its key is bound.
pub(crate) struct Command {
    pub(crate) id: CommandId,
    /// The stable id of the command.
    pub(crate) name: &'static str,
    /// The palette label.
    pub(crate) label: &'static str,
    /// The default key, in GPUI keystroke syntax.
    pub(crate) key: &'static str,
    /// The key context the key is bound in.
    pub(crate) context: &'static str,
    /// The area the palette groups the command under.
    pub(crate) area: Area,
    /// Whether the command can run now; else why not, as the palette shows it.
    pub(crate) available: fn(&Shell) -> Result<(), &'static str>,
}

/// The area of a command, which the palette's Commands scope groups by, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Area {
    Request,
    Tabs,
    Environment,
    View,
    Agents,
    Workspace,
}

impl Area {
    /// The group header of the area.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Request => "Request",
            Self::Tabs => "Tabs",
            Self::Environment => "Environments",
            Self::View => "View",
            Self::Agents => "Agents",
            Self::Workspace => "Workspace",
        }
    }

    /// The element id suffix of the area's group header.
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Tabs => "tabs",
            Self::Environment => "environment",
            Self::View => "view",
            Self::Agents => "agents",
            Self::Workspace => "workspace",
        }
    }
}

impl Command {
    /// Whether the command can run now.
    pub(crate) fn enabled(&self, shell: &Shell) -> bool {
        (self.available)(shell).is_ok()
    }
}

/// Generates the command actions, [`CommandId`], [`COMMANDS`], their key bindings and the
/// shell's action handlers from one list.
macro_rules! commands {
    ($($variant:ident: $action:ident, $name:literal, $label:literal, $key:literal, $area:ident, $available:path;)*) => {
        actions!(shell, [$(
            #[doc = concat!("Runs the `", $name, "` command.")]
            $action,
        )*]);

        /// Names a command of [`COMMANDS`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum CommandId {
            $($variant,)*
        }

        /// Every command, in palette order.
        pub(crate) const COMMANDS: &[Command] = &[$(Command {
            id: CommandId::$variant,
            name: $name,
            label: $label,
            key: $key,
            context: SHELL_CONTEXT,
            area: Area::$area,
            available: $available,
        },)*];

        /// Binds the default key of every command in its context, as [`COMMANDS`] has them.
        fn bind_commands(cx: &mut App) {
            let mut bindings = Vec::new();
            $(if let Some(command) = command(CommandId::$variant) {
                bindings.push(KeyBinding::new(command.key, $action, Some(command.context)));
            })*
            cx.bind_keys(bindings);
        }

        /// Adds the handler of every command action to the shell's root `element`.
        pub(super) fn on_actions(element: Div, cx: &mut Context<Shell>) -> Div {
            element$(.on_action(cx.listener(|this, _: &$action, window, cx| {
                this.run_key(CommandId::$variant, window, cx);
            })))*
        }
    };
}

commands! {
    SendRequest: SendRequest, "send-request", "Send request", "ctrl-enter", Request, can_send;
    CancelSend: CancelRequest, "cancel-send", "Cancel send", "escape", Request, has_request;
    SaveRequest: SaveRequest, "save-request", "Save request", "ctrl-s", Request, has_draft;
    RevertRequest: RevertRequest, "revert-request", "Revert request", "ctrl-shift-r", Request, has_draft;
    NewRequest: NewRequest, "new-request", "New request", "ctrl-n", Request, has_collection;
    NewCollection: NewCollection, "new-collection", "New collection", "ctrl-shift-n", Request, loaded;
    NewEnvironment: NewEnvironment, "new-environment", "New environment", "ctrl-shift-e", Environment, loaded;
    SwitchEnvironment: SwitchEnvironment, "switch-environment", "Switch environment", "ctrl-e", Environment, loaded;
    OpenWorkspace: OpenWorkspace, "open-workspace", "Open workspace", "ctrl-o", Workspace, always;
    NewWorkspace: NewWorkspace, "new-workspace", "New workspace", "ctrl-shift-o", Workspace, always;
    CloseWorkspace: CloseWorkspace, "close-workspace", "Close workspace", "ctrl-shift-w", Workspace, loaded;
    WorkspaceSettings: WorkspaceSettings, "workspace-settings", "Workspace settings", "ctrl-,", Workspace, loaded;
    CloseTab: CloseTab, "close-tab", "Close tab", "ctrl-w", Tabs, has_tab;
    NextTab: NextTab, "next-tab", "Next tab", "ctrl-tab", Tabs, has_tab;
    PreviousTab: PreviousTab, "previous-tab", "Previous tab", "ctrl-shift-tab", Tabs, has_tab;
    ShowCollections: ShowCollections, "show-collections", "Show Collections", "ctrl-1", View, loaded;
    ShowHistory: ShowHistory, "show-history", "Show History", "ctrl-2", View, loaded;
    ShowEnvironments: ShowEnvironments, "show-environments", "Show Environments", "ctrl-3", Environment, loaded;
    ToggleTheme: ToggleTheme, "toggle-theme", "Toggle theme", "ctrl-shift-t", View, always;
    OpenApprovals: OpenApprovals, "open-approvals", "Open MCP approvals", "ctrl-shift-m", Agents, always;
}

actions!(
    shell,
    [
        /// Opens the command palette.
        OpenPalette,
    ]
);

/// The keys that open the palette, bound in [`SHELL_CONTEXT`].
pub(crate) const PALETTE_KEYS: [&str; 2] = ["ctrl-k", "ctrl-p"];

/// Binds every command's key and the palette keys.
pub(super) fn bind_keys(cx: &mut App) {
    bind_commands(cx);
    cx.bind_keys(PALETTE_KEYS.map(|key| KeyBinding::new(key, OpenPalette, Some(SHELL_CONTEXT))));
}

/// The command `id`.
pub(crate) fn command(id: CommandId) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.id == id)
}

/// `key` as the palette prints it: `ctrl-shift-tab` is `Ctrl+Shift+Tab`.
pub(crate) fn key_label(key: &str) -> String {
    key.split('-')
        .map(|part| match part {
            "ctrl" => "Ctrl".to_owned(),
            "shift" => "Shift".to_owned(),
            "alt" => "Alt".to_owned(),
            "escape" => "Esc".to_owned(),
            other => {
                let mut characters = other.chars();
                characters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(characters).collect()
                })
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn always(_: &Shell) -> Result<(), &'static str> {
    Ok(())
}

fn loaded(shell: &Shell) -> Result<(), &'static str> {
    match shell.workspace {
        OpenedWorkspace::Loaded(_) => Ok(()),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Err("no workspace open"),
    }
}

fn has_collection(shell: &Shell) -> Result<(), &'static str> {
    loaded(shell)?;
    if shell.roots.is_empty() {
        Err("no collection")
    } else {
        Ok(())
    }
}

/// A request tab is active; the history tab, while active, is not one.
fn has_request(shell: &Shell) -> Result<(), &'static str> {
    if shell.history_tab_active() {
        return Err("a recorded request is open");
    }
    shell
        .documents
        .active()
        .map(|_| ())
        .ok_or("no open request")
}

/// Ctrl+Enter sends the active request, or resends the active history tab's record.
fn can_send(shell: &Shell) -> Result<(), &'static str> {
    if shell.history_tab_active() {
        shell.can_resend()
    } else {
        has_request(shell)
    }
}

/// A request tab or the history tab is open.
fn has_tab(shell: &Shell) -> Result<(), &'static str> {
    if shell.orphan.is_some() {
        Ok(())
    } else {
        shell
            .documents
            .active()
            .map(|_| ())
            .ok_or("no open request")
    }
}

fn has_draft(shell: &Shell) -> Result<(), &'static str> {
    has_request(shell)?;
    if shell
        .documents
        .active()
        .is_some_and(|id| shell.drafts.contains_key(id))
    {
        Ok(())
    } else {
        Err("no unsaved changes")
    }
}

impl Shell {
    /// The key of `id`: runs the command when it can run now. Escape always runs, because
    /// it also closes an open menu.
    fn run_key(&mut self, id: CommandId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = command(id) else {
            return;
        };
        if command.enabled(self) || id == CommandId::CancelSend {
            log::debug!("running {} from its key", command.name);
            self.run_command(id, window, cx);
        }
    }

    /// Runs the command `id`.
    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.active_request().map(str::to_owned);
        match id {
            CommandId::SendRequest if self.history_tab_active() => {
                self.resend_history(window, cx);
            }
            CommandId::SendRequest => self.send_active(window, cx),
            CommandId::CancelSend => self.cancel_active(window, cx),
            CommandId::SaveRequest => self.save_active(window, cx),
            CommandId::RevertRequest => {
                if let Some(active) = active {
                    self.revert(&active, window, cx);
                }
            }
            CommandId::NewRequest => self.new_request(window, cx),
            CommandId::NewCollection => {
                self.show_panel(Panel::Collections, cx);
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.command(TreeCommand::NewCollection, window, cx);
                });
            }
            CommandId::NewEnvironment => self.new_environment(window, cx),
            CommandId::SwitchEnvironment => {
                if !self.environment_menu_open {
                    self.workspace_menu_open = false;
                    self.toggle_environment_menu(window, cx);
                }
            }
            CommandId::OpenWorkspace => self.pick_folder(window, cx),
            CommandId::NewWorkspace => self.start_new_workspace(window, cx),
            CommandId::CloseWorkspace => self.switch_workspace(None, window, cx),
            CommandId::WorkspaceSettings => {
                self.open_settings(Section::Globals, cx);
                self.restore_focus(window, cx);
            }
            CommandId::CloseTab if self.history_tab_active() => self.close_history_tab(cx),
            CommandId::CloseTab => {
                if let Some(active) = active {
                    self.close_document(&active, cx);
                }
            }
            CommandId::NextTab => self.step_tab(true, window, cx),
            CommandId::PreviousTab => self.step_tab(false, window, cx),
            CommandId::ShowCollections => self.switch_panel(Panel::Collections, window, cx),
            CommandId::ShowHistory => self.switch_panel(Panel::History, window, cx),
            CommandId::ShowEnvironments => {
                self.open_settings(Section::Environments(None), cx);
                self.restore_focus(window, cx);
            }
            CommandId::ToggleTheme => self.toggle_theme(window, cx),
            CommandId::OpenApprovals => self.open_approvals(window, cx),
        }
    }

    /// Creates a request in the selected collection or folder, in the parent of the
    /// selected request, else in the first collection, and opens it into rename.
    fn new_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = {
            let sidebar = self.sidebar.read(cx);
            sidebar
                .selected()
                .and_then(|id| match sidebar.node(id).map(|node| &node.kind) {
                    Some(NodeKind::Request { .. }) => sidebar.parent(id),
                    Some(NodeKind::Collection | NodeKind::Folder) => Some(id.to_owned()),
                    None => None,
                })
                .or_else(|| self.roots.first().map(|root| root.id.clone()))
        };
        let Some(target) = target else {
            return;
        };
        self.show_panel(Panel::Collections, cx);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.command(TreeCommand::NewRequest(target), window, cx);
        });
    }

    /// Shows `panel` and gives the focus back: the panel it replaces may have held it, and
    /// without a focus the window's keys reach nothing.
    fn switch_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        self.show_panel(panel, cx);
        self.restore_focus(window, cx);
    }

    /// Activates the tab after the active one, or before it, wrapping around.
    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.documents.open_ids();
        let Some(index) = self
            .documents
            .active()
            .and_then(|active| open.iter().position(|id| id == active))
        else {
            return;
        };
        let count = open.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        if let Some(id) = open.get(next).cloned() {
            self.activate_document(&id, window, cx);
        }
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
