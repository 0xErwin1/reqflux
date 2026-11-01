//! The editable Body pane: the mode control switches the body mode, the language menu the
//! language of a raw body, and clicking the raw text
//! swaps it for a multi-line editor whose text lands in the draft as it is typed, as the
//! URL field's does. Enter inserts a newline, Ctrl+Enter sends and Escape gives the focus
//! back to the shell, keeping the text, which is already in the draft. The editor stays open
//! when the focus moves elsewhere; its focus ring shows only while it has the focus (S11).
//! The urlencoded and
//! form-data grids edit through the grid cells with the table `body`. GraphQL and binary
//! bodies stay read-only.

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, TextareaState};
use reqflux_core::model::{Body, FormPart, RawLanguage};

use super::Shell;

/// The modes the mode control offers, by their id and name, left to right.
pub(crate) const BODY_MODES: [(&str, &str); 6] = [
    ("none", "None"),
    ("raw", "Raw"),
    ("urlencoded", "URL-encoded"),
    ("formdata", "Form data"),
    ("binary", "Binary"),
    ("graphql", "GraphQL"),
];

/// The languages the language menu of a raw body offers, by their id and name, in menu
/// order.
pub(crate) const RAW_LANGUAGES: [(&str, &str, RawLanguage); 3] = [
    ("json", "JSON", RawLanguage::Json),
    ("xml", "XML", RawLanguage::Xml),
    ("text", "Text", RawLanguage::Text),
];

/// The rows the raw text editor shows.
const EDITOR_ROWS: usize = 12;

/// The mode id of `body`.
pub(crate) fn body_kind(body: &Body) -> &'static str {
    match body {
        Body::None => "none",
        Body::Raw { .. } => "raw",
        Body::Urlencoded { .. } => "urlencoded",
        Body::FormData { .. } => "formdata",
        Body::Binary { .. } => "binary",
        Body::Graphql { .. } => "graphql",
    }
}

/// The content type the request sends with `body`, as the mode row hints it.
pub(crate) fn body_hint(body: &Body) -> Option<&'static str> {
    Some(match body {
        Body::Raw { language, .. } => match language {
            RawLanguage::Json => "application/json",
            RawLanguage::Xml => "application/xml",
            RawLanguage::Text => "text/plain",
            RawLanguage::Html => "text/html",
            RawLanguage::Javascript => "application/javascript",
        },
        Body::Urlencoded { .. } => "application/x-www-form-urlencoded",
        Body::FormData { .. } => "multipart/form-data · boundary set on send",
        Body::Graphql { .. } => "application/json",
        Body::None | Body::Binary { .. } => return None,
    })
}

/// An empty body of the mode `kind`; a raw body starts in JSON.
fn empty_body(kind: &str) -> Option<Body> {
    Some(match kind {
        "none" => Body::None,
        "raw" => Body::Raw {
            language: RawLanguage::Json,
            text: String::new(),
        },
        "urlencoded" => Body::Urlencoded { fields: Vec::new() },
        "formdata" => Body::FormData { parts: Vec::new() },
        "binary" => Body::Binary {
            path: String::new(),
        },
        "graphql" => Body::Graphql {
            query: String::new(),
            variables: String::new(),
        },
        _ => return None,
    })
}

/// The key, the value (a file part's path) and the enabled flag of the 1-based row `n` of
/// the urlencoded or form-data `body`.
pub(crate) fn body_cell(
    body: &mut Body,
    n: usize,
) -> Option<(&mut String, &mut String, &mut bool)> {
    let index = n.checked_sub(1)?;
    match body {
        Body::Urlencoded { fields } => fields
            .get_mut(index)
            .map(|field| (&mut field.key, &mut field.value, &mut field.enabled)),
        Body::FormData { parts } => parts.get_mut(index).map(|part| match part {
            FormPart::Text {
                key,
                value,
                enabled,
                ..
            } => (key, value, enabled),
            FormPart::File {
                key, path, enabled, ..
            } => (key, path, enabled),
        }),
        _ => None,
    }
}

/// The raw text editor of a request: the request it edits and its state.
pub(crate) struct BodyEdit {
    pub(crate) request_id: String,
    pub(crate) input: Entity<TextareaState>,
    /// Whether the editor has the focus, which its ring shows.
    pub(crate) focused: bool,
    /// Puts the text into the draft on every change and follows the focus.
    _change: Subscription,
}

impl Shell {
    /// The body the Body pane of the request `id` shows: the draft's, else the file's.
    fn displayed_body(&self, id: &str) -> Option<Body> {
        let state = self.requests.get(id)?;
        Some(
            self.displayed_request(id, &state.inspection.request.request)
                .body
                .clone(),
        )
    }

    /// Opens the language menu and focuses it, or closes it when open.
    pub(super) fn toggle_body_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.body_menu_open {
            self.close_body_menu(window, cx);
        } else {
            self.body_menu_open = true;
            self.body_menu_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Closes the language menu and gives the focus back.
    pub(super) fn close_body_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body_menu_open = false;
        self.restore_focus(window, cx);
    }

    /// Switches the body of the request `id` to an empty body of the mode `kind`. Picking
    /// the mode it already has changes nothing. An open field commits first.
    pub(super) fn set_body_mode(
        &mut self,
        id: &str,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(current) = self.displayed_body(id) else {
            return;
        };
        if body_kind(&current) == kind {
            return;
        }
        self.drop_body_edit_of(id);
        if let Some(body) = empty_body(kind) {
            self.edit_draft(id, |request| request.body = body, cx);
        }
    }

    /// Switches the raw body of the request `id` to `language`, keeping its text. Picking
    /// the language it already has changes nothing. An open field commits first.
    pub(super) fn set_raw_language(
        &mut self,
        id: &str,
        language: RawLanguage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_body_menu(window, cx);
        self.commit_cell_edit(window, cx);
        if !matches!(self.displayed_body(id), Some(Body::Raw { language: current, .. }) if current != language)
        {
            return;
        }
        self.edit_draft(
            id,
            |request| {
                if let Body::Raw {
                    language: current, ..
                } = &mut request.body
                {
                    *current = language;
                }
            },
            cx,
        );
    }

    /// Swaps the raw text of the request `id` for a focused multi-line editor seeded with
    /// it. An open cell field commits first.
    pub(super) fn start_body_edit(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self.body_edit_of(id) {
            return;
        }
        let Some(Body::Raw { text, .. }) = self.displayed_body(id) else {
            return;
        };
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(EDITOR_ROWS));
        input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.focus(window, cx);
        });
        let change = cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                if let Some(id) = this.body_edit.as_ref().map(|edit| edit.request_id.clone()) {
                    let value = input.read(cx).value().to_string();
                    this.set_draft_body_text(&id, value, cx);
                }
            }
            InputEvent::Focus | InputEvent::Blur => {
                if let Some(edit) = this.body_edit.as_mut() {
                    edit.focused = matches!(event, InputEvent::Focus);
                    cx.notify();
                }
            }
            InputEvent::PressEnter { .. } => {}
        });
        self.body_edit = Some(BodyEdit {
            request_id: id.to_owned(),
            input,
            focused: true,
            _change: change,
        });
        cx.notify();
    }

    /// Whether the raw text editor is open over the request `id`.
    pub(super) fn body_edit_of(&self, id: &str) -> bool {
        self.body_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
    }

    /// Puts `text` into the raw body of the request `id`'s draft. The text the request
    /// already has, without a draft, creates none.
    fn set_draft_body_text(&mut self, id: &str, value: String, cx: &mut Context<Self>) {
        if !self.drafts.contains_key(id)
            && matches!(self.displayed_body(id), Some(Body::Raw { text, .. }) if text == value)
        {
            return;
        }
        self.edit_draft(
            id,
            |request| {
                if let Body::Raw { text, .. } = &mut request.body {
                    *text = value;
                }
            },
            cx,
        );
    }

    /// Escape in the editor: closes it and gives the focus back to the shell. The text is
    /// already in the draft.
    pub(super) fn close_body_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.body_edit.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Drops the editor of the request `id`; the draft keeps what it synced.
    pub(super) fn drop_body_edit_of(&mut self, id: &str) {
        if self.body_edit_of(id) {
            self.body_edit = None;
        }
    }

    /// Drops the editor when it no longer belongs to the active request `id`.
    pub(super) fn drop_stale_body_edit(&mut self, id: &str) {
        if self
            .body_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id != id)
        {
            self.body_edit = None;
        }
    }
}
