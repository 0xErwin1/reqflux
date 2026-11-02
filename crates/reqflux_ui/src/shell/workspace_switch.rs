//! Switching the window's workspace (recent-workspaces S4, S6, S7): opening a recent one, a
//! picked folder or a new workspace, closing it, and keeping the recent list in step with
//! the app state.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, Window};
use reqflux_engine::CurrentValues;

use super::documents::Documents;
use super::history::now_millis;
use super::sidebar::Sidebar;
use super::workspace_picker::picked_path;
use super::{Shell, subscribe_sidebar, toasts, tree_model};
use crate::workspace::OpenedWorkspace;

/// The message shown when the workspace at `path` did not open.
fn open_error(path: &Path, error: impl std::fmt::Display) -> String {
    format!("Could not open {}: {error}", path.display())
}

impl Shell {
    /// Replaces the window's workspace with the one at `path`, or closes it when `None`,
    /// which shows the empty state.
    ///
    /// The new workspace loads first: a failure keeps the current one and shows the error in
    /// the workspace menu or the empty state. Then every send in flight is cancelled (dropping
    /// a send cancels it), every tab closes and every field edit and menu drops, the active
    /// environment becomes the one stored for the new workspace (else none), and the sidebar, drafts, current values and history are
    /// read again for the new workspace, which is recorded as recent. Drafts of the previous
    /// workspace stay in the app state.
    ///
    /// The previous engine drops here unless a send's prepare task, which runs `execute` on
    /// the background executor, still holds a clone: that task keeps it alive until it
    /// returns, and its hand-off is cancelled with the send, so nothing it produces reaches
    /// the shell.
    pub(super) fn switch_workspace(
        &mut self,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = match path {
            None => OpenedWorkspace::None,
            Some(path) => {
                let canonical = match fs::canonicalize(&path) {
                    Ok(canonical) => canonical,
                    Err(error) => {
                        self.show_workspace_error(open_error(&path, error), cx);
                        self.failed_recent = Some(path);
                        return;
                    }
                };
                match OpenedWorkspace::open(Some(canonical)) {
                    OpenedWorkspace::Failed {
                        path: failed,
                        error,
                    } => {
                        self.show_workspace_error(open_error(&failed, error), cx);
                        self.failed_recent = Some(path);
                        return;
                    }
                    opened => opened,
                }
            }
        };
        self.leave_workspace();
        self.workspace = opened;
        let (roots, workspace_id) = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => (
                tree_model::nodes(engine.workspace()),
                Some(engine.workspace().manifest.id.to_string()),
            ),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => (Vec::new(), None),
        };
        self.roots = roots;
        self.workspace_id = workspace_id;
        let panel = self.sidebar.read(cx).panel;
        let workspace = &self.workspace;
        self.sidebar = cx.new(|cx| {
            let mut sidebar = Sidebar::new(workspace, cx);
            sidebar.panel = panel;
            sidebar
        });
        self._sidebar_events = subscribe_sidebar(&self.sidebar, cx);
        self.restore_active_environment();
        self.load_drafts();
        self.load_current_values();
        self.refresh_history(cx);
        self.workspace_error = None;
        self.failed_recent = None;
        self.record_recent();
        self.refresh_recents();
        self.workspace_menu_open = false;
        self.new_workspace_name = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Drops everything the shell holds for the current workspace. The sends go first:
    /// dropping one cancels its request.
    fn leave_workspace(&mut self) {
        self.sends.clear();
        self.last_send = None;
        self.documents = Documents::default();
        self.requests.clear();
        self.drafts.clear();
        self.save_errors.clear();
        self.url_edit = None;
        self.cell_edit = None;
        self.settings.reset();
        self.body_edit = None;
        self.orphan = None;
        self.current = CurrentValues::default();
        self.active_environment = None;
        self.environment_menu_open = false;
        self.auth_menu_open = false;
        self.body_menu_open = false;
        self.method_menu_open = false;
    }

    /// Opens the recent workspace at 0-based `index`.
    pub(super) fn open_recent(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(recent) = self.recents.get(index) {
            let path = recent.path.clone();
            self.switch_workspace(Some(path), window, cx);
        }
    }

    /// Drops the recent workspace at 0-based `index` from the list.
    pub(super) fn remove_recent(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(recent), Some(state)) = (self.recents.get(index), self.state.as_mut()) else {
            return;
        };
        if self.failed_recent.as_ref() == Some(&recent.path) {
            self.failed_recent = None;
        }
        if let Err(error) = state.remove_recent_workspace(&recent.path) {
            toasts::failure(
                format!("Cannot remove {} from the recent workspaces", recent.name),
                error.to_string(),
                cx,
            );
        }
        self.refresh_recents();
        cx.notify();
    }

    /// Asks for a folder and opens it as the workspace; a dialog that fails shows an error
    /// toast.
    pub(super) fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.picker.pick_folder(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = picked_path(picked.await, "Cannot open the folder dialog", cx) else {
                return;
            };
            if let Err(error) = this.update_in(cx, |shell, window, cx| {
                shell.switch_workspace(Some(path), window, cx);
            }) {
                log::debug!("the folder was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Asks for a new directory, creates a workspace there named after it and opens it; a
    /// dialog that fails shows an error toast.
    pub(super) fn pick_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.picker.pick_new(cx);
        cx.spawn_in(window, async move |this, cx| {
            let title = "Cannot open the new workspace dialog";
            let Some(path) = picked_path(picked.await, title, cx) else {
                return;
            };
            if let Err(error) = this.update_in(cx, |shell, window, cx| {
                shell.create_workspace(path, window, cx);
            }) {
                log::debug!("the new directory was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Creates a workspace at `path` named after its directory and opens it; a failure
    /// shows as the workspace error.
    fn create_workspace(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match reqflux_engine::create_workspace(&path, &name) {
            Ok(_) => self.switch_workspace(Some(path), window, cx),
            Err(error) => self.show_workspace_error(
                format!(
                    "Could not create a workspace in {}: {error}",
                    path.display()
                ),
                cx,
            ),
        }
    }

    pub(super) fn show_workspace_error(&mut self, error: String, cx: &mut Context<Self>) {
        log::warn!("{error}");
        self.workspace_error = Some(error);
        // Only an open failure belongs to a recent row; the caller marks it after this.
        self.failed_recent = None;
        cx.notify();
    }

    /// Records the loaded workspace as the newest recent one, under its canonical
    /// directory, and remembers that directory as the current one.
    pub(super) fn record_recent(&mut self) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            self.workspace_path = None;
            return;
        };
        let path = fs::canonicalize(engine.root()).unwrap_or_else(|error| {
            log::warn!("cannot canonicalize {}: {error}", engine.root().display());
            engine.root().to_path_buf()
        });
        if let Some(state) = self.state.as_mut() {
            let manifest = &engine.workspace().manifest;
            if let Err(error) = state.record_recent_workspace(
                &path,
                &manifest.id.to_string(),
                &manifest.name,
                now_millis(),
            ) {
                log::warn!("cannot record {} as recent: {error}", path.display());
            }
        }
        self.workspace_path = Some(path);
    }

    /// Reads the recent workspaces from the app state again; a failure keeps the list.
    pub(super) fn refresh_recents(&mut self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        match state.recent_workspaces() {
            Ok(recents) => self.recents = recents,
            Err(error) => log::warn!("cannot read the recent workspaces: {error}"),
        }
    }

    /// Opens the workspace menu and focuses it, or closes it when open.
    pub(super) fn toggle_workspace_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_menu_open {
            self.close_workspace_menu(window, cx);
            return;
        }
        self.environment_menu_open = false;
        self.workspace_menu_open = true;
        self.refresh_recents();
        self.workspace_menu_focus.focus(window, cx);
        cx.notify();
    }

    /// Closes the workspace menu and gives the focus back.
    pub(super) fn close_workspace_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_menu_open = false;
        self.new_workspace_name = None;
        self.restore_focus(window, cx);
    }
}
