//! The editable Auth pane: the Source control switches between the parent's auth and the
//! request's own, the type menu switches the auth type, and the Username, Key,
//! Password, Token and API key Value rows open a field in the shell's cell field slot, so it
//! commits, discards and gives the focus back exactly as a grid cell does. Every field opens
//! seeded with its text; one whose value is masked opens masked, and its eye shows or hides
//! it, closed or open. Committing an empty field clears it.
//! Add to picks header or query. Digest, AWS SigV4 and OAuth 2.0 edit through their typed
//! views ([`typed`]), and an OAuth 2.0 auth shows its token's state ([`token`]). Other auth
//! types stay read-only.

mod token;
mod typed;
#[cfg(test)]
mod typed_tests;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use gpui::{AppContext as _, Context, Focusable as _, Window};
use gpui_component::input::InputState;
use reqflux_core::model::{ApiKeyLocation, Auth, AwsV4Auth, DigestAuth, OAuth2Auth};

use super::Shell;
use super::cell_edit::{CellEdit, CellField, seed_focused};
use super::request::auth_fields;

pub(crate) use token::TokenFetching;
pub(crate) use typed::TYPED_PREFIX;

/// The table name an Auth field records in its [`CellEdit`], which no grid uses.
pub(crate) const AUTH_TABLE: &str = "auth";

/// The auth types the type menu offers, by their on-disk `type`, in menu order. Inheriting
/// is the Source control's, not a type.
pub(crate) const AUTH_KINDS: [&str; 7] = [
    "none", "basic", "bearer", "apikey", "digest", "awsv4", "oauth2",
];

/// A menu of the Auth pane, which `Shell::auth_menu_open` shows under its select.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AuthMenu {
    /// The auth type.
    #[default]
    Type,
    /// The digest algorithm.
    Algorithm,
    /// The OAuth 2.0 grant type.
    Grant,
}

/// The Auth pane state the shell keeps beyond the draft: which menu is open, which masked
/// fields are revealed, and the OAuth 2.0 token of each request.
#[derive(Default)]
pub(crate) struct AuthPane {
    /// Which menu `Shell::auth_menu_open` shows.
    pub(crate) menu: AuthMenu,
    /// The masked fields whose eye shows their value, by request id and field element id,
    /// each with a digest of the value revealed: a reveal holds only for that value.
    revealed: HashMap<(String, String), u64>,
    tokens: token::Tokens,
}

/// A digest of a revealed value, so the pane keeps no second copy of the credential.
/// In-process only: it is never written, logged or sent.
fn digest(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

impl AuthPane {
    /// Whether the eye of the field `field` (its element id) of the request `request_id`
    /// shows its value `value`. A reveal of another value does not count; the shell drops
    /// it on its next render ([`Shell::prune_auth_reveals`]).
    pub(crate) fn is_revealed(&self, request_id: &str, field: &str, value: &str) -> bool {
        self.revealed
            .get(&(request_id.to_owned(), field.to_owned()))
            .is_some_and(|revealed| *revealed == digest(value))
    }
}

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
        "digest" => DigestAuth::default().into(),
        "awsv4" => AwsV4Auth::default().into(),
        "oauth2" => OAuth2Auth::default().into(),
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
    pub(super) fn displayed_auth(&self, id: &str) -> Option<Auth> {
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
    /// of the request `id`, seeded with its text and `masked` when the row hides it. An open
    /// field elsewhere commits first.
    pub(super) fn start_auth_edit(
        &mut self,
        id: &str,
        secret: bool,
        masked: bool,
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
        let seed = text.clone();
        self.open_auth_field(id, AUTH_TABLE, seed, secret, masked, window, cx);
    }

    /// Opens the focused field of the Auth pane of the request `id`, recorded under
    /// `table`, seeded with `seed`; a `secret` one shows a placeholder while empty, and a
    /// `masked` one hides its text and draws no `{{name}}` marks.
    #[allow(
        clippy::too_many_arguments,
        reason = "the field's identity, seed and look, plus the GPUI contexts"
    )]
    fn open_auth_field(
        &mut self,
        id: &str,
        table: &str,
        seed: String,
        secret: bool,
        masked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if secret {
                state.set_placeholder("Secret value", window, cx);
            }
            state.set_masked(masked, window, cx);
            state
        });
        input.update(cx, |state, cx| {
            seed_focused(state, seed, window, cx);
            // Typing into a hidden value replaces it rather than appending to what it
            // cannot see.
            if masked {
                state.select_all(window, cx);
            }
        });
        let marks = (!masked).then(|| self.mark_input(id, &input, cx));
        self.cell_edit = Some(CellEdit {
            request_id: id.to_owned(),
            table: table.to_owned(),
            n: 0,
            field: if secret {
                CellField::Value
            } else {
                CellField::Key
            },
            input,
            secret,
            marks,
        });
        cx.notify();
    }

    /// The value of the auth field `field` (its element id) of the request `request_id` as
    /// the pane shows it.
    fn auth_field_value(&self, request_id: &str, field: &str) -> Option<String> {
        let auth = self.displayed_auth(request_id)?;
        auth_fields(&auth, |_| false)
            .into_iter()
            .find(|shown| shown.element_id() == field)
            .map(|shown| shown.value)
    }

    /// Drops every reveal whose field no longer shows the value revealed: an edit, undo, a
    /// reload or revert, a type or source switch, or a deleted request. Called on every
    /// render, so a value that later comes back to the revealed one shows masked.
    pub(super) fn prune_auth_reveals(&mut self) {
        if self.auth_pane.revealed.is_empty() {
            return;
        }
        let stale: Vec<(String, String)> = self
            .auth_pane
            .revealed
            .iter()
            .filter(|((request, field), revealed)| {
                self.auth_field_value(request, field)
                    .is_none_or(|value| digest(&value) != **revealed)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            self.auth_pane.revealed.remove(&key);
        }
    }

    /// Shows the value of the masked field `field` (its element id) of the request
    /// `request_id`, read when clicked, or hides it again. When the field is the open one
    /// (`editing`), its input follows: revealed it shows its text with its `{{name}}` marks,
    /// hidden it masks it.
    pub(crate) fn toggle_auth_reveal(
        &mut self,
        request_id: &str,
        field: &str,
        editing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(value) = self.auth_field_value(request_id, field) else {
            return;
        };
        let revealed = !self.auth_pane.is_revealed(request_id, field, &value);
        let key = (request_id.to_owned(), field.to_owned());
        if revealed {
            self.auth_pane.revealed.insert(key, digest(&value));
        } else {
            self.auth_pane.revealed.remove(&key);
        }
        if editing && let Some(input) = self.cell_edit.as_ref().map(|edit| edit.input.clone()) {
            input.update(cx, |state, cx| state.set_masked(!revealed, window, cx));
            let marks = revealed.then(|| self.mark_input(request_id, &input, cx));
            if let Some(edit) = self.cell_edit.as_mut() {
                edit.marks = marks;
            }
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Puts `value` into the plain text, or the secret when `secret`, of the auth of the
    /// request `id` in its draft; an empty value clears it. Committing the text the request
    /// already has, without a draft, creates none.
    pub(super) fn set_draft_auth_text(
        &mut self,
        id: &str,
        secret: bool,
        value: String,
        cx: &mut Context<Self>,
    ) {
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
        self.toggle_auth_select(AuthMenu::Type, window, cx);
    }

    /// Opens the Auth pane menu `menu` and focuses it, or closes it when it is open.
    pub(super) fn toggle_auth_select(
        &mut self,
        menu: AuthMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.auth_menu_open && self.auth_pane.menu == menu {
            self.close_auth_menu(window, cx);
        } else {
            self.auth_pane.menu = menu;
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

    /// Switches the auth of the request `id` to the on-disk type `kind`, with the fields the
    /// draft last had for it, else empty ones. The auth switched away from stays in the
    /// draft's stash, so switching back restores it; it reaches the file only if that type
    /// is saved. Picking the type it already has changes nothing. An open field commits
    /// first.
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
        self.edit_whole_draft(
            id,
            |draft| {
                let auth = draft.stash.auths.remove(kind).unwrap_or(auth);
                let left = std::mem::replace(&mut draft.request.auth, auth);
                let left_kind = left.kind().to_owned();
                if empty_auth(&left_kind).as_ref() != Some(&left) {
                    draft.stash.auths.insert(left_kind, left);
                }
            },
            cx,
        );
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
