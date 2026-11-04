//! The editable method chip: its menu picks a standard verb into the draft, and `Custom…`
//! opens a field in the shell's cell field slot, seeded with the current verb, so it commits,
//! discards and gives the focus back exactly as a grid cell does. A verb the model rejects
//! leaves the draft unchanged.

use gpui::{AppContext as _, Context, Window};
use gpui_component::input::InputState;
use reqflux_core::model::Method;

use super::Shell;
use super::cell_edit::{CellEdit, CellField, seed_focused};

/// The table name the custom verb field records in its [`CellEdit`], which no grid uses.
pub(crate) const METHOD_TABLE: &str = "method";

/// The verbs the method menu offers, in menu order.
pub(crate) const VERBS: [Method; 7] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
    Method::HEAD,
    Method::OPTIONS,
];

impl Shell {
    /// The method the chip of the request `id` shows: the draft's, else the file's.
    pub(super) fn displayed_method(&self, id: &str) -> Option<Method> {
        let state = self.requests.get(id)?;
        Some(
            self.displayed_request(id, &state.inspection.request.request)
                .method
                .clone(),
        )
    }

    /// Whether the open field is the custom verb field of the request `id`.
    pub(super) fn method_edit_of(&self, id: &str) -> bool {
        self.cell_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id && edit.table == METHOD_TABLE)
    }

    /// Opens the method menu and focuses it, or closes it when open.
    pub(super) fn toggle_method_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.method_menu_open {
            self.close_method_menu(window, cx);
        } else {
            self.method_menu_open = true;
            self.method_menu_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Closes the method menu and gives the focus back.
    pub(super) fn close_method_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.method_menu_open = false;
        self.restore_focus(window, cx);
    }

    /// Picks `method` for the request `id` from the menu. An open field commits first.
    pub(super) fn pick_method(
        &mut self,
        id: &str,
        method: Method,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_method_menu(window, cx);
        self.commit_cell_edit(window, cx);
        self.set_draft_method(id, method, cx);
    }

    /// Puts `method` into the draft of the request `id`; the verb it already has changes
    /// nothing.
    fn set_draft_method(&mut self, id: &str, method: Method, cx: &mut Context<Self>) {
        if self.displayed_method(id).as_ref() == Some(&method) {
            return;
        }
        self.edit_draft(id, |request| request.method = method, cx);
    }

    /// Commits the custom verb `text` into the draft of the request `id`; text the model
    /// does not accept as a verb is dropped.
    pub(super) fn commit_custom_method(&mut self, id: &str, text: &str, cx: &mut Context<Self>) {
        match Method::new(text.trim()) {
            Ok(method) => self.set_draft_method(id, method, cx),
            Err(error) => log::debug!("custom method dropped: {error}"),
        }
    }

    /// `Custom…`: closes the menu and opens a focused field seeded with the current verb.
    pub(super) fn start_method_edit(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_method_menu(window, cx);
        self.commit_cell_edit(window, cx);
        let Some(method) = self.displayed_method(id) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            seed_focused(state, method.to_string(), window, cx);
            state.select_all(window, cx);
        });
        self.cell_edit = Some(CellEdit {
            request_id: id.to_owned(),
            table: METHOD_TABLE.to_owned(),
            n: 0,
            field: CellField::Key,
            input,
            secret: false,
            marks: None,
        });
        cx.notify();
    }
}
