//! The editable Auth pane: the Source control switches between the parent's auth and the
//! request's own, the type menu switches the auth type, and the Username, Key,
//! Password, Token and API key Value rows open a field in the shell's cell field slot, so it
//! commits, discards and gives the focus back exactly as a grid cell does. A secret field
//! opens empty with a `Secret value` placeholder and an empty commit keeps the stored value.
//! Add to picks header or query. Other auth types stay read-only.

use gpui::{AppContext as _, Context, Window};
use gpui_component::input::InputState;
use reqflux_core::model::{ApiKeyLocation, Auth};

use super::Shell;
use super::cell_edit::{CellEdit, CellField};

/// The table name an Auth field records in its [`CellEdit`], which no grid uses.
pub(crate) const AUTH_TABLE: &str = "auth";

/// The auth types the type menu offers, by their on-disk `type`, in menu order. Inheriting
/// is the Source control's, not a type.
pub(crate) const AUTH_KINDS: [&str; 4] = ["none", "basic", "bearer", "apikey"];

/// A new auth of the on-disk type `kind`, with empty fields and the key in a header.
pub(crate) fn empty_auth(kind: &str) -> Option<Auth> {
    Some(match kind {
        "inherit" => Auth::Inherit,
        "none" => Auth::None,
        "basic" => Auth::Basic {
            username: String::new(),
            password: String::new(),
        },
        "bearer" => Auth::Bearer {
            token: String::new(),
        },
        "apikey" => Auth::ApiKey {
            key: String::new(),
            value: String::new(),
            location: ApiKeyLocation::Header,
        },
        _ => return None,
    })
}

/// The plain text (Username, Key) or the secret (Password, Token, Value) of `auth`.
fn auth_text(auth: &mut Auth, secret: bool) -> Option<&mut String> {
    match (auth, secret) {
        (Auth::Basic { username, .. }, false) => Some(username),
        (Auth::Basic { password, .. }, true) => Some(password),
        (Auth::Bearer { token }, true) => Some(token),
        (Auth::ApiKey { key, .. }, false) => Some(key),
        (Auth::ApiKey { value, .. }, true) => Some(value),
        _ => None,
    }
}

impl Shell {
    /// The auth the Auth pane of the request `id` shows: the draft's, else the file's.
    fn displayed_auth(&self, id: &str) -> Option<Auth> {
        let state = self.requests.get(id)?;
        Some(
            self.displayed_request(id, &state.inspection.request.request)
                .auth
                .clone(),
        )
    }

    /// Whether the open field is the Auth field of the request `id`, the secret one when
    /// `secret`.
    pub(super) fn auth_edit_of(&self, id: &str, secret: bool) -> bool {
        self.cell_edit.as_ref().is_some_and(|edit| {
            edit.request_id == id && edit.table == AUTH_TABLE && edit.secret == secret
        })
    }

    /// Opens a focused field over the plain text, or the secret when `secret`, of the auth
    /// of the request `id`. The plain field is seeded with its text; the secret field opens
    /// empty with a placeholder. An open field elsewhere commits first.
    pub(super) fn start_auth_edit(
        &mut self,
        id: &str,
        secret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.auth_edit_of(id, secret) {
            return;
        }
        self.commit_cell_edit(window, cx);
        let Some(mut auth) = self.displayed_auth(id) else {
            return;
        };
        let Some(text) = auth_text(&mut auth, secret) else {
            return;
        };
        let seed = if secret { String::new() } else { text.clone() };
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if secret {
                state.set_placeholder("Secret value", window, cx);
            }
            state
        });
        input.update(cx, |state, cx| {
            state.set_value(seed, window, cx);
            state.focus(window, cx);
        });
        self.cell_edit = Some(CellEdit {
            request_id: id.to_owned(),
            table: AUTH_TABLE.to_owned(),
            n: 0,
            field: if secret {
                CellField::Value
            } else {
                CellField::Key
            },
            input,
            secret,
        });
        cx.notify();
    }

    /// Puts `value` into the plain text, or the secret when `secret`, of the auth of the
    /// request `id` in its draft. An empty secret keeps the stored one; committing the text
    /// the request already has, without a draft, creates none.
    pub(super) fn set_draft_auth_text(
        &mut self,
        id: &str,
        secret: bool,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if secret && value.is_empty() {
            return;
        }
        if !self.drafts.contains_key(id) {
            let mut current = self.displayed_auth(id);
            if current
                .as_mut()
                .and_then(|auth| auth_text(auth, secret))
                .is_some_and(|text| *text == value)
            {
                return;
            }
        }
        self.edit_draft(
            id,
            |request| {
                if let Some(text) = auth_text(&mut request.auth, secret) {
                    *text = value;
                }
            },
            cx,
        );
    }

    /// Opens the auth type menu and focuses it, or closes it when open.
    pub(super) fn toggle_auth_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.auth_menu_open {
            self.close_auth_menu(window, cx);
        } else {
            self.auth_menu_open = true;
            self.auth_menu_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Closes the auth type menu and gives the focus back.
    pub(super) fn close_auth_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auth_menu_open = false;
        self.restore_focus(window, cx);
    }

    /// Switches the auth of the request `id` to the on-disk type `kind`, with empty fields.
    /// Picking the type it already has changes nothing. An open field commits first.
    pub(super) fn set_auth_type(
        &mut self,
        id: &str,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_auth_menu(window, cx);
        self.commit_cell_edit(window, cx);
        if self
            .displayed_auth(id)
            .is_some_and(|auth| auth.kind() == kind)
        {
            return;
        }
        let Some(auth) = empty_auth(kind) else {
            return;
        };
        self.edit_draft(id, |request| request.auth = auth, cx);
    }

    /// Makes the request `id` inherit its parent's auth, or, when `inherit` is false and it
    /// inherits, gives it its own auth, with no auth. Picking the source it already has
    /// changes nothing.
    pub(super) fn set_auth_source(
        &mut self,
        id: &str,
        inherit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inherits = self
            .displayed_auth(id)
            .is_some_and(|auth| matches!(auth, Auth::Inherit));
        if inherit != inherits {
            self.set_auth_type(id, if inherit { "inherit" } else { "none" }, window, cx);
        }
    }

    /// Sends the API key of the request `id` in `location`, a header or the query. Picking
    /// the location it already has changes nothing.
    pub(super) fn set_api_key_location(
        &mut self,
        id: &str,
        location: ApiKeyLocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if !matches!(self.displayed_auth(id), Some(Auth::ApiKey { location: current, .. }) if current != location)
        {
            return;
        }
        self.edit_draft(
            id,
            |request| {
                if let Auth::ApiKey {
                    location: current, ..
                } = &mut request.auth
                {
                    *current = location;
                }
            },
            cx,
        );
    }
}
