//! Saved examples (v1-features S4, design/examples/01-tree-children): a request and the
//! response it got, kept beside the request through `Engine::{examples, save_example,
//! rename_example, delete_example, save_response_body}`.
//!
//! - In the tree an example is a leaf under its request with its status code (see
//!   [`super::tree_model`]); its row menu offers Open, Rename, Save body to file… and Delete
//!   (see [`sidebar_menu`]). Rename happens in the row; Delete asks first.
//! - Opening one gives the read-only example tab, `example-tab`, beside the request tabs: the
//!   request as written and the response as saved, a masked value shown with the SECRET
//!   badge (see [`view`]). One example tab is open at a time; opening another replaces it.
//! - Save as example, from the response pane, opens a dialog with the name, the reason phrase
//!   at first (see [`dialog`]); the saved toast follows and the new leaf is selected, not
//!   opened.
//! - Save response body to file…, from the response pane's Export menu (see [`export`]),
//!   and Save body to file… on an example, ask for a path in the system's save dialog and
//!   write the body as received; a toast says where, or why not.
//!
//! Nothing here shows a secret: an example keeps the request as written, `{{variables}}`
//! included, and the response as the send's masker left it.

mod dialog;
mod export;
mod model;
mod sidebar_menu;
mod view;

use std::path::{Path, PathBuf};

use bolt::modals::ModalFocus;
use gpui::{AnyElement, App, ClipboardItem, Context, FocusHandle, Window};
use reqflux_core::model::{Id, Request};
use reqflux_engine::{Engine, Response, SavedExample};

use super::documents::{ExampleLocation, locate_example};
use super::request::Completed;
use super::{Shell, toasts, tree_model};
use crate::workspace::OpenedWorkspace;

pub(crate) use export::response_buttons;
pub(crate) use view::{render, tab_chip};

/// What the examples draw over the window: the Export menu and the Save as example dialog,
/// while open.
pub(crate) fn render_overlays(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let menu = export::render(shell, window, cx);
    menu.into_iter()
        .chain(dialog::render(shell, window, cx))
        .collect()
}

/// Emitted by the sidebar when the example with this tree node id is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenExample(pub(crate) String);

/// An example edit the sidebar asks the shell to run; `id` is the example's tree node id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExampleCommand {
    /// Renames the example to `name`.
    Rename { id: String, name: String },
    /// Deletes the example.
    Delete(String),
    /// Asks for a file and writes the example's response body to it.
    SaveBody(String),
}

/// Which part of the saved response the example tab shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ExampleView {
    #[default]
    Body,
    Headers,
}

/// The open example tab.
pub(crate) struct ExampleTab {
    /// The example's tree node id.
    pub(crate) node: String,
    /// Where the example and its request sit.
    pub(crate) place: ExampleLocation,
    /// The example as read from disk.
    pub(crate) saved: SavedExample,
    pub(crate) view: ExampleView,
    /// Whether the tab is the active document.
    pub(crate) active: bool,
}

/// The examples of a window: the open tab and the Save as example dialog.
pub(crate) struct Examples {
    pub(crate) tab: Option<ExampleTab>,
    dialog: Option<dialog::SaveDialog>,
    focus: ModalFocus,
    /// The response's Export menu, while open.
    export: Option<export::ExportMenu>,
    /// Focused while the Export menu is open.
    export_focus: FocusHandle,
}

impl Examples {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            tab: None,
            dialog: None,
            focus: ModalFocus::new(cx),
            export: None,
            export_focus: cx.focus_handle(),
        }
    }
}

/// The example `example` of the request `request`, read from disk.
fn read_example(engine: &Engine, request: &str, example: &str) -> Option<SavedExample> {
    match engine.examples(request) {
        Ok(examples) => examples
            .into_iter()
            .find(|saved| saved.example.id.to_string() == example),
        Err(error) => {
            log::warn!("cannot read the examples of request {request}: {error}");
            None
        }
    }
}

impl Shell {
    /// Whether the example tab is the active document.
    pub(crate) fn example_tab_active(&self) -> bool {
        self.examples.tab.as_ref().is_some_and(|tab| tab.active)
    }

    /// Leaves the example tab in the tab row, inactive.
    pub(crate) fn leave_example_tab(&mut self) {
        if let Some(tab) = self.examples.tab.as_mut() {
            tab.active = false;
        }
    }

    /// Activates the example tab, leaving the views and the other documents it replaces.
    pub(crate) fn activate_example_tab(&mut self, cx: &mut Context<Self>) {
        self.leave_approvals(cx);
        self.leave_audit(cx);
        self.leave_history_tab();
        self.leave_run_tab();
        if let Some(tab) = self.examples.tab.as_mut() {
            tab.active = true;
        }
        cx.notify();
    }

    /// Opens the example with the tree node id `node` in the example tab, read only, and
    /// activates it.
    pub(crate) fn open_example(&mut self, node: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(place) = locate_example(self.sidebar.read(cx).roots(), node) else {
            return;
        };
        let Some(saved) = read_example(engine, &place.request, &place.example) else {
            toasts::failure(
                "Cannot open the example",
                format!("{} is no longer on disk.", place.file),
                cx,
            );
            return;
        };
        self.examples.tab = Some(ExampleTab {
            node: node.to_owned(),
            place,
            saved,
            view: ExampleView::Body,
            active: false,
        });
        self.activate_example_tab(cx);
    }

    /// Closes the example tab.
    pub(crate) fn close_example_tab(&mut self, cx: &mut Context<Self>) {
        self.examples.tab = None;
        cx.notify();
    }

    /// Shows `view` of the saved response in the example tab.
    pub(crate) fn set_example_view(&mut self, view: ExampleView, cx: &mut Context<Self>) {
        if let Some(tab) = self.examples.tab.as_mut() {
            tab.view = view;
            cx.notify();
        }
    }

    /// Runs an example edit the sidebar asked for.
    pub(crate) fn run_example_command(&mut self, command: &ExampleCommand, cx: &mut Context<Self>) {
        match command {
            ExampleCommand::Rename { id, name } => self.rename_example(id, name, cx),
            ExampleCommand::Delete(id) => self.delete_example(id, cx),
            ExampleCommand::SaveBody(id) => self.save_example_body(id, cx),
        }
    }

    /// The request id and the example id of the example node `node`.
    fn example_ids(&self, node: &str, cx: &App) -> Option<(String, Id)> {
        let place = locate_example(self.sidebar.read(cx).roots(), node)?;
        let example = place.example.parse::<Id>().ok()?;
        Some((place.request, example))
    }

    /// Renames the example `node` to `name`; its file follows. A failure shows above the
    /// tree as a tree edit's does.
    fn rename_example(&mut self, node: &str, name: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some((request, example)) = self.example_ids(node, cx) else {
            return;
        };
        match engine.rename_example(&request, example, name) {
            Ok(()) => self.refresh_examples(None, cx),
            Err(error) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.show_error(error.to_string(), cx)),
        }
    }

    /// Deletes the example `node`; its request is not changed. An open tab on it closes.
    fn delete_example(&mut self, node: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some((request, example)) = self.example_ids(node, cx) else {
            return;
        };
        match engine.delete_example(&request, example) {
            Ok(()) => {
                if self
                    .examples
                    .tab
                    .as_ref()
                    .is_some_and(|tab| tab.node == node)
                {
                    self.examples.tab = None;
                }
                self.refresh_examples(None, cx);
            }
            Err(error) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.show_error(error.to_string(), cx)),
        }
    }

    /// Lists the examples again: the tree, the shell's roots and the open tab, which reads
    /// its example again or closes when it is gone. `created`, a node id, is revealed and
    /// selected.
    fn refresh_examples(&mut self, created: Option<String>, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let roots = tree_model::engine_nodes(engine);
        let reopened = self.examples.tab.as_ref().map(|tab| {
            let place = locate_example(&roots, &tab.node);
            let saved = place
                .as_ref()
                .and_then(|place| read_example(engine, &place.request, &place.example));
            place.zip(saved)
        });
        match reopened {
            Some(Some((place, saved))) => {
                if let Some(tab) = self.examples.tab.as_mut() {
                    tab.place = place;
                    tab.saved = saved;
                }
            }
            Some(None) => self.examples.tab = None,
            None => {}
        }
        self.roots.clone_from(&roots);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.tree_changed(roots, created, false, cx)
        });
        cx.notify();
    }

    /// The open request `id` as written: its draft when it has one, else the saved document.
    fn written_request(&self, id: &str) -> Option<Request> {
        let saved = &self.requests.get(id)?.inspection.request.request;
        Some(self.displayed_request(id, saved).clone())
    }

    /// The response the request `id` last received, with the masker of that send.
    fn received(&self, id: &str) -> Option<(Response, reqflux_engine::Masker)> {
        let send = self.sends.get(id)?;
        match send.state.last()? {
            Completed::Response(received) => {
                Some((model::received_response(received), send.masker.clone()))
            }
            Completed::Failure(_) => None,
        }
    }

    /// Save response body to file…: asks for a path in the system's save dialog and writes
    /// the active request's last response body to it.
    pub(crate) fn save_response_body(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.active_request().map(str::to_owned) else {
            return;
        };
        let Some((response, _)) = self.received(&id) else {
            return;
        };
        let stem = tree_model::find(&self.roots, &id)
            .map(|node| node.name.clone())
            .unwrap_or_default();
        self.ask_and_write_body(response, &stem, cx);
    }

    /// Save body to file… on the example `node`.
    fn save_example_body(&mut self, node: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(place) = locate_example(self.sidebar.read(cx).roots(), node) else {
            return;
        };
        let Some(saved) = read_example(engine, &place.request, &place.example) else {
            return;
        };
        self.ask_and_write_body(model::saved_response(&saved), &place.name, cx);
    }

    /// Save body to file… in the example tab.
    pub(crate) fn save_open_example_body(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.examples.tab.as_ref() else {
            return;
        };
        let response = model::saved_response(&tab.saved);
        let stem = tab.place.name.clone();
        self.ask_and_write_body(response, &stem, cx);
    }

    /// Copy in the example tab: the saved body text, as masked.
    pub(crate) fn copy_open_example_body(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.examples.tab.as_ref() else {
            return;
        };
        if let reqflux_core::model::ExampleBody::Text(text) = &tab.saved.example.response.body {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            toasts::success("Copied the body", tab.place.name.clone(), cx);
        }
    }

    /// Opens the system's save dialog with a name from `stem` and the content type, then
    /// writes `response`'s body to the path chosen. A dismissed dialog writes nothing.
    fn ask_and_write_body(&mut self, response: Response, stem: &str, cx: &mut Context<Self>) {
        let content_type = model::content_type(
            response
                .headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );
        let name = model::suggested_file_name(stem, &content_type);
        let start = dirs_start();
        let answer = cx.prompt_for_new_path(&start, Some(&name));
        cx.spawn(async move |this, cx| {
            let path = match answer.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(error)) => {
                    let shown = this.update(cx, |_, cx| {
                        toasts::failure("Cannot open the save dialog", error.to_string(), cx);
                    });
                    if let Err(closed) = shown {
                        log::debug!("the save dialog failed after the shell closed: {closed}");
                    }
                    return;
                }
            };
            if let Err(error) = this.update(cx, |shell, cx| shell.write_body(&response, &path, cx))
            {
                log::debug!("the body path was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Writes `response`'s body to `path` through the engine and says how it went.
    pub(crate) fn write_body(&mut self, response: &Response, path: &Path, cx: &mut App) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        match engine.save_response_body(response, path) {
            Ok(()) => toasts::success(
                "Saved the response body",
                model::body_saved_subtitle(path, response.body.len()),
                cx,
            ),
            Err(error) => {
                toasts::failure("Could not save the response body", error.to_string(), cx)
            }
        }
    }
}

/// Where the save dialog starts: the working directory, else the current one.
fn dirs_start() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
#[path = "examples_tests.rs"]
pub(crate) mod tests;
