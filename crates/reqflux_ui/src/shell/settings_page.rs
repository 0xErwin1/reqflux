//! The workspace settings page (design-catch-up T7, design/variables/01-settings-page.html):
//! what it shows and the environment operations it runs. The rail's Environments entry, the
//! workspace menu's `Workspace settings…` and the palette open it in place of the sidebar and
//! the document islands.
//!
//! Its sections are Globals, Environments (the environment list beside the variables of the
//! selected one) and one per collection. The environment list creates an environment and
//! opens its name into a field; an environment's `…` menu renames or deletes it, and delete
//! asks first. Variables are edited by [`super::variables_ops`].
//!
//! Above them sits the App section (app-pending S3, design/app-settings/02): General and MCP
//! policy, which edit the user's `config.toml` through [`app_settings`]. The rail's Settings
//! opens the page there, and it opens with no workspace too.

use gpui::{App, AppContext as _, Context, Entity, FocusHandle, Pixels, Point, Window};
use gpui_component::input::InputState;
use reqflux_core::model::Id;
use reqflux_engine::SecretScope;

use super::Shell;
use super::sidebar_history::Panel;
use super::variables_model::{collections, environments};
use crate::workspace::OpenedWorkspace;

#[path = "app_settings.rs"]
pub(crate) mod app_settings;

/// A section of the App part of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppSection {
    /// Theme and interface font size.
    General,
    /// The `[mcp]` rules and lists, and the workspace overrides read-only.
    McpPolicy,
}

/// A section of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    /// A section of the user's `config.toml`.
    App(AppSection),
    Globals,
    /// The environments, with the one whose variables show; `None` picks the first.
    Environments(Option<Id>),
    /// The variables of a collection.
    Collection(Id),
}

impl Section {
    /// The scope whose variables the section shows.
    pub(crate) fn scope(self) -> Option<SecretScope> {
        match self {
            Self::App(_) => None,
            Self::Globals => Some(SecretScope::Global),
            Self::Environments(id) => id.map(SecretScope::Environment),
            Self::Collection(id) => Some(SecretScope::Collection(id)),
        }
    }
}

/// The open rename field of an environment.
pub(crate) struct Rename {
    pub(crate) id: Id,
    pub(crate) original: String,
    pub(crate) input: Entity<InputState>,
}

/// The state of the page: its section, the environment menu, rename field and delete
/// confirmation, and the outcome of the last write.
pub(crate) struct SettingsPage {
    section: Section,
    /// The environment whose menu is open.
    pub(crate) menu: Option<Id>,
    /// Where the open menu sits, in window coordinates: the click on the `…` button.
    pub(crate) menu_at: Point<Pixels>,
    pub(crate) menu_focus: FocusHandle,
    pub(crate) rename: Option<Rename>,
    /// The environment whose delete waits for a confirmation.
    pub(crate) confirm: Option<Id>,
    /// Why the last write failed.
    pub(crate) error: Option<String>,
    /// A warning about the last write that succeeded.
    pub(crate) warning: Option<String>,
    /// The App section: what `config.toml` holds and its controls.
    pub(crate) app: app_settings::AppSettings,
}

impl SettingsPage {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            section: Section::Globals,
            menu: None,
            menu_at: Point::default(),
            menu_focus: cx.focus_handle(),
            rename: None,
            confirm: None,
            error: None,
            warning: None,
            app: app_settings::AppSettings::default(),
        }
    }

    /// Drops the menu, the rename field, the confirmation and the messages, as when the
    /// workspace changes.
    pub(crate) fn reset(&mut self) {
        if !matches!(self.section, Section::App(_)) {
            self.section = Section::Globals;
        }
        self.menu = None;
        self.rename = None;
        self.confirm = None;
        self.error = None;
        self.warning = None;
    }
}

impl Shell {
    /// The section the page shows, with an environment or a collection that no longer exists
    /// replaced: by the first environment, or by Globals.
    pub(crate) fn settings_section(&self) -> Section {
        if let Section::App(section) = self.settings.section {
            return Section::App(section);
        }
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return Section::Globals;
        };
        let workspace = engine.workspace();
        match self.settings.section {
            Section::App(section) => Section::App(section),
            Section::Globals => Section::Globals,
            Section::Environments(id) => {
                let all = environments(workspace);
                let kept = id.filter(|id| all.iter().any(|entry| entry.id == *id));
                Section::Environments(kept.or_else(|| all.first().map(|entry| entry.id)))
            }
            Section::Collection(id) => {
                if collections(workspace).iter().any(|entry| entry.id == id) {
                    Section::Collection(id)
                } else {
                    Section::Globals
                }
            }
        }
    }

    /// Whether the page shows instead of the sidebar and the document.
    pub(crate) fn settings_open(&self, cx: &App) -> bool {
        (matches!(self.workspace, OpenedWorkspace::Loaded(_))
            || matches!(self.settings.section, Section::App(_)))
            && self.sidebar.read(cx).panel == Panel::Environments
    }

    /// Opens the page at `section`; `Environments(None)` keeps the environment it showed.
    pub(crate) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        self.leave_approvals(cx);
        self.settings.section = match (section, self.settings.section) {
            (Section::Environments(None), Section::Environments(Some(id))) => {
                Section::Environments(Some(id))
            }
            (section, _) => section,
        };
        self.settings.section = self.settings_section();
        self.show_panel(Panel::Environments, cx);
    }

    /// Shows `section`, committing an open field first.
    pub(crate) fn select_settings_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        self.settings.section = section;
        self.settings.section = self.settings_section();
        self.settings.menu = None;
        self.settings.confirm = None;
        self.settings.rename = None;
        cx.notify();
    }

    /// Closes the page and shows the collection tree again.
    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_panel(Panel::Collections, cx);
        self.restore_focus(window, cx);
    }

    /// Escape on the page: closes the environment menu, the delete confirmation or the
    /// page. Returns whether it did something.
    pub(crate) fn dismiss_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.settings_open(cx) {
            return false;
        }
        if self.settings.menu.is_some() {
            self.close_settings_menu(window, cx);
        } else if self.settings.confirm.take().is_some() {
            cx.notify();
        } else {
            self.close_settings(window, cx);
        }
        true
    }

    /// Opens the page at Environments, creates an environment, shows it and opens its name
    /// into the rename field.
    pub(crate) fn new_environment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(window, cx);
        if !self.settings_open(cx) {
            self.open_settings(Section::Environments(None), cx);
        }
        self.settings.menu = None;
        let outcome = self.with_engine(|engine| {
            engine
                .create_environment()
                .map(Some)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok((Some(id), _)) => {
                self.environments_changed(cx);
                self.open_settings(Section::Environments(Some(id)), cx);
                self.start_environment_rename(id, window, cx);
            }
            Ok((None, _)) => self.environments_changed(cx),
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    fn environment_name_of(&self, id: Id) -> Option<String> {
        match &self.workspace {
            OpenedWorkspace::Loaded(engine) => environments(engine.workspace())
                .into_iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.name),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
        }
    }

    /// Opens the rename field over the name of the environment `id`, all selected.
    pub(crate) fn start_environment_rename(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.environment_name_of(id) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(name.clone(), window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        self.settings.menu = None;
        self.settings.rename = Some(Rename {
            id,
            original: name,
            input,
        });
        cx.notify();
    }

    /// Enter, or a click outside the rename field: renames the environment and its file unless the name is
    /// unchanged; the active environment follows its new slug.
    pub(crate) fn commit_environment_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.settings.rename.take() else {
            return;
        };
        let name = rename.input.read(cx).value().to_string();
        self.focus.focus(window, cx);
        if name == rename.original {
            cx.notify();
            return;
        }
        let id = rename.id;
        let was_active = self.is_active_environment(id);
        let outcome = self.with_engine(|engine| {
            engine
                .rename_environment(id, &name)
                .map(|_| None)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok(_) => {
                if was_active {
                    self.active_environment = self.environment_slug(id);
                }
                self.environments_changed(cx);
            }
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    pub(crate) fn cancel_environment_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.rename.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Opens the menu of the environment `id` at `position`, in window coordinates.
    pub(crate) fn open_settings_menu(
        &mut self,
        id: Id,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.menu = Some(id);
        self.settings.menu_at = position;
        self.settings.menu_focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn close_settings_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.menu = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn ask_environment_delete(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.menu = None;
        self.settings.confirm = Some(id);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn cancel_environment_delete(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.confirm = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Deletes the environment waiting for confirmation; deleting the active one selects No
    /// environment.
    pub(crate) fn confirm_environment_delete(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.settings.confirm.take() else {
            return;
        };
        self.commit_cell_edit(window, cx);
        let was_active = self.is_active_environment(id);
        let outcome = self.with_engine(|engine| {
            engine
                .delete_environment(id)
                .map(|()| None)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok(_) => {
                if was_active {
                    self.active_environment = None;
                    self.save_active_environment();
                }
                self.environments_changed(cx);
            }
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    /// After an environment operation: the error clears, a section that no longer exists is
    /// replaced and the open requests are inspected again.
    fn environments_changed(&mut self, cx: &mut Context<Self>) {
        self.settings.error = None;
        self.settings.section = self.settings_section();
        self.inspect_open_requests(cx);
    }

    fn environment_slug(&self, id: Id) -> Option<String> {
        match &self.workspace {
            OpenedWorkspace::Loaded(engine) => engine
                .workspace()
                .environments
                .iter()
                .find(|node| node.environment.id == id)
                .map(|node| node.slug.clone()),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
        }
    }

    fn is_active_environment(&self, id: Id) -> bool {
        self.active_environment.is_some() && self.active_environment == self.environment_slug(id)
    }

    /// Shows why the last write failed; the warning clears.
    pub(crate) fn show_variables_error(&mut self, error: String, cx: &mut Context<Self>) {
        log::warn!("variables operation failed: {error}");
        self.settings.error = Some(error);
        self.settings.warning = None;
        cx.notify();
    }

    /// Shows `warning` about the write that just succeeded, or clears the error and the
    /// warning with `None`.
    pub(crate) fn show_variables_warning(
        &mut self,
        warning: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.settings.error = None;
        self.settings.warning = warning;
        cx.notify();
    }
}
