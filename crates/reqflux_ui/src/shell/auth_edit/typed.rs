//! Editing digest, AWS SigV4 and OAuth 2.0 through their typed views: a field writes one
//! on-disk key, and the auth is written back through
//! [`Auth::with_typed`](reqflux_core::model::Auth::with_typed), so only the edited
//! key changes: every other key, known or not, and a Postman spelling stay as they were. The digest algorithm and the OAuth 2.0 grant are
//! picked from a menu.

use gpui::{Context, Window};
use reqflux_core::model::{DigestAlgorithm, OAuth2Grant, TypedAuth};

use crate::shell::Shell;

/// The prefix of the table a typed auth field records in its cell edit, before its on-disk
/// key: `auth.client_id`.
pub(crate) const TYPED_PREFIX: &str = "auth.";

/// The text field `key` of `typed`, when it has one.
fn typed_text<'a>(typed: &'a mut TypedAuth, key: &str) -> Option<&'a mut String> {
    match typed {
        TypedAuth::Digest(digest) => match key {
            "username" => Some(&mut digest.username),
            "password" => Some(&mut digest.password),
            _ => None,
        },
        TypedAuth::AwsV4(aws) => match key {
            "access_key" => Some(&mut aws.access_key),
            "secret_key" => Some(&mut aws.secret_key),
            "session_token" => Some(&mut aws.session_token),
            "region" => Some(&mut aws.region),
            "service" => Some(&mut aws.service),
            _ => None,
        },
        TypedAuth::OAuth2(oauth) => match key {
            "token_url" => Some(&mut oauth.token_url),
            "auth_url" => Some(&mut oauth.auth_url),
            "client_id" => Some(&mut oauth.client_id),
            "client_secret" => Some(&mut oauth.client_secret),
            "scope" => Some(&mut oauth.scope),
            "username" => Some(&mut oauth.username),
            "password" => Some(&mut oauth.password),
            "refresh_token" => Some(&mut oauth.refresh_token),
            _ => None,
        },
    }
}

impl Shell {
    /// The typed view of the auth the Auth pane of the request `id` shows.
    fn displayed_typed(&self, id: &str) -> Option<TypedAuth> {
        self.displayed_auth(id)?.typed()?.ok()
    }

    /// Whether the open field is the typed auth field `key` of the request `id`.
    pub(in crate::shell) fn typed_edit_of(&self, id: &str, key: &str) -> bool {
        self.cell_edit.as_ref().is_some_and(|edit| {
            edit.request_id == id && edit.table.strip_prefix(TYPED_PREFIX) == Some(key)
        })
    }

    /// Opens a focused field over the typed auth field `key` of the request `id`: seeded
    /// with its text, or empty with a placeholder when it is `secret`. An open field
    /// elsewhere commits first.
    pub(in crate::shell) fn start_typed_auth_edit(
        &mut self,
        id: &str,
        key: &str,
        secret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.typed_edit_of(id, key) {
            return;
        }
        self.commit_cell_edit(window, cx);
        let Some(mut typed) = self.displayed_typed(id) else {
            return;
        };
        let Some(text) = typed_text(&mut typed, key) else {
            return;
        };
        let seed = if secret { String::new() } else { text.clone() };
        self.open_auth_field(
            id,
            &format!("{TYPED_PREFIX}{key}"),
            seed,
            secret,
            window,
            cx,
        );
    }

    /// Puts `value` into the typed auth field `key` of the request `id` in its draft. An
    /// empty secret keeps the stored one; committing the text the field already has,
    /// without a draft, creates none.
    pub(in crate::shell) fn set_draft_typed_text(
        &mut self,
        id: &str,
        key: &str,
        secret: bool,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if secret && value.is_empty() {
            return;
        }
        let Some(mut typed) = self.displayed_typed(id) else {
            return;
        };
        let Some(text) = typed_text(&mut typed, key) else {
            return;
        };
        if *text == value && !self.drafts.contains_key(id) {
            return;
        }
        *text = value;
        self.edit_draft(
            id,
            |request| request.auth = request.auth.with_typed(typed),
            cx,
        );
    }

    /// Writes `edit` of the typed auth of the request `id` to its draft when it changes it.
    fn edit_typed(
        &mut self,
        id: &str,
        edit: impl FnOnce(&mut TypedAuth),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_auth_menu(window, cx);
        self.commit_cell_edit(window, cx);
        let Some(typed) = self.displayed_typed(id) else {
            return;
        };
        let mut edited = typed.clone();
        edit(&mut edited);
        if edited != typed {
            self.edit_draft(
                id,
                |request| request.auth = request.auth.with_typed(edited),
                cx,
            );
        }
    }

    /// Sets the digest algorithm of the request `id`.
    pub(in crate::shell) fn set_digest_algorithm(
        &mut self,
        id: &str,
        algorithm: DigestAlgorithm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_typed(
            id,
            |typed| {
                if let TypedAuth::Digest(digest) = typed {
                    digest.algorithm = algorithm;
                }
            },
            window,
            cx,
        );
    }

    /// Sets the OAuth 2.0 grant type of the request `id`; the fields of the other grants
    /// are kept.
    pub(in crate::shell) fn set_oauth_grant(
        &mut self,
        id: &str,
        grant: OAuth2Grant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_typed(
            id,
            |typed| {
                if let TypedAuth::OAuth2(oauth) = typed {
                    oauth.grant_type = grant;
                }
            },
            window,
            cx,
        );
    }
}
