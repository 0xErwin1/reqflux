//! The editable URL: the read-only URL of the request line swaps for a text field seeded
//! with the URL as shown, the request's URL as written with the params that are sent
//! appended. The URL's query and the Params grid are one thing: every change of the field
//! puts the URL without its query into the draft and its pairs into the params the request
//! had when the field opened, keeping the disabled and blank rows in place (see
//! [`reqflux_core::query::merge`]). Merging into those, not into the last change's, keeps the
//! rows' places and descriptions while the text passes through states without a query. Enter commits the text
//! and sends the request; Escape discards what was typed since the field got focus and
//! restores the URL and the params. The field takes Escape while it has focus through the
//! input's own key context, so the shell's send-cancel binding keeps working when no field
//! is focused.

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};
use reqflux_core::model::{KeyValue, Request};
use reqflux_core::query;

use super::Shell;

/// The URL field of a request under edit: its state, the id of the request it belongs to
/// and the URL and params the request had when the field opened.
pub(crate) struct UrlEdit {
    /// The request the field edits.
    pub(crate) request_id: String,
    /// The input state of the field.
    pub(crate) input: Entity<InputState>,
    /// The URL as written when the field opened, restored on Escape.
    seed_url: String,
    /// The params when the field opened, restored on Escape.
    seed_params: Vec<KeyValue>,
    /// Syncs the draft on every change of the field's text.
    _change: Subscription,
}

impl Shell {
    /// Swaps the read-only URL of the request `id` for a focused field seeded with the URL
    /// as shown.
    pub(super) fn start_url_edit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // A cell field that is open commits first, like a spreadsheet, so no edit is lost.
        if self.cell_edit.is_some() {
            self.commit_cell_edit(window, cx);
        }
        if self
            .url_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
        {
            return;
        }
        let Some((seed_url, seed_params)) = self
            .shown_request(id)
            .map(|request| (request.url.clone(), request.params.clone()))
        else {
            return;
        };
        let shown = query::compose(&seed_url, &seed_params);
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(shown, window, cx);
            state.focus(window, cx);
        });
        let request_id = id.to_owned();
        let change = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(edit) = this.url_edit.as_ref()
            {
                let value = edit.input.read(cx).value().to_string();
                let request_id = edit.request_id.clone();
                let seed_params = edit.seed_params.clone();
                this.set_draft_url_and_query(&request_id, &value, &seed_params, cx);
            }
        });
        self.url_edit = Some(UrlEdit {
            request_id,
            input,
            seed_url,
            seed_params,
            _change: change,
        });
        cx.notify();
    }

    /// The request the view of `id` shows: the draft's when it has one, else the saved
    /// request as inspected.
    fn shown_request(&self, id: &str) -> Option<&Request> {
        let saved = &self.requests.get(id)?.inspection.request.request;
        Some(self.displayed_request(id, saved))
    }

    /// Puts the URL `text` into the draft of the request `id`: the URL without its query,
    /// and as params the query's pairs merged into `seed_params`.
    fn set_draft_url_and_query(
        &mut self,
        id: &str,
        text: &str,
        seed_params: &[KeyValue],
        cx: &mut Context<Self>,
    ) {
        let (base, pairs) = query::split(text);
        self.edit_draft(
            id,
            |request| {
                request.params = query::merge(seed_params, pairs);
                request.url = base;
            },
            cx,
        );
    }

    /// Enter in the field: commits its text into the draft, returns to the read-only line
    /// and sends the request.
    pub(super) fn commit_url_edit_and_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request_id) = self.url_edit.as_ref().map(|edit| edit.request_id.clone()) else {
            return;
        };
        self.commit_url_edit(window, cx);
        self.send(&request_id, window, cx);
    }

    /// Commits the field's text into the draft and returns to the read-only line.
    pub(super) fn commit_url_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((request_id, input, seed_params)) = self.url_edit.as_ref().map(|edit| {
            (
                edit.request_id.clone(),
                edit.input.clone(),
                edit.seed_params.clone(),
            )
        }) else {
            return;
        };
        let value = input.read(cx).value().to_string();
        self.set_draft_url_and_query(&request_id, &value, &seed_params, cx);
        self.close_url_edit(window, cx);
    }

    /// Escape in the field: discards what was typed since the field got focus and restores
    /// the URL and the params. A draft that ends up equal to the saved request is dropped.
    pub(super) fn discard_url_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((request_id, seed_url, seed_params)) = self.url_edit.as_ref().map(|edit| {
            (
                edit.request_id.clone(),
                edit.seed_url.clone(),
                edit.seed_params.clone(),
            )
        }) else {
            return;
        };
        if self.drafts.contains_key(&request_id) {
            self.edit_draft(
                &request_id,
                |request| {
                    request.url = seed_url;
                    request.params = seed_params;
                },
                cx,
            );
        }
        let saved = self
            .requests
            .get(&request_id)
            .map(|state| state.inspection.request.request.clone());
        if self
            .drafts
            .get(&request_id)
            .is_some_and(|draft| Some(&draft.request) == saved.as_ref())
        {
            self.drafts.remove(&request_id);
            self.delete_stored_draft(&request_id);
            cx.notify();
        }
        self.close_url_edit(window, cx);
    }

    /// Drops the field and gives the focus back to the shell, so the window's keys
    /// (Ctrl+Enter, Escape) keep reaching the shell's key context.
    pub(super) fn close_url_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.url_edit.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Drops the field when it no longer belongs to the active request `id`.
    pub(super) fn drop_stale_url_edit(&mut self, id: &str) {
        if self
            .url_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id != id)
        {
            self.url_edit = None;
        }
    }

    /// The URL the request line of `id` shows: the URL as written with the params that are
    /// sent appended as written, the draft's when it has one.
    pub(super) fn shown_url(&self, id: &str) -> String {
        self.shown_request(id).map_or_else(String::new, |request| {
            query::compose(&request.url, &request.params)
        })
    }

    /// Drops the field of the request `id`, without committing or discarding its text: the
    /// draft keeps what the field synced so far.
    pub(super) fn drop_url_edit_of(&mut self, id: &str) {
        if self
            .url_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
        {
            self.url_edit = None;
        }
    }
}

#[cfg(test)]
#[path = "url_edit_tests.rs"]
mod tests;
