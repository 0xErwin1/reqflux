//! A curl command pasted into the URL field (v1-features S2, design/codegen/01-dialog-and-toast):
//! a paste whose first word is `curl` replaces the method, URL, params, headers, body, auth
//! and settings of the draft at once, through [`reqflux_core::curl::parse`] and
//! [`Parsed::apply`], closes the field and reports what was imported in a toast; with
//! options the request has no place for, a warning that lists them. The toast's Undo button
//! (`toast-action-<n>-0`) puts the draft back as it was before the paste, or drops it when
//! there was none. A paste that is not a curl command is plain text in the field.
//!
//! The toast names what was imported, never a value: a basic auth password or a bearer
//! token stays in the draft, masked by the Auth pane.

use bolt::toast::ToastAction;
use gpui::{Context, Window};
use reqflux_core::curl::{self, CurlError, Parsed};
use reqflux_core::model::{Auth, Body, RawLanguage};

use super::drafts::RequestDraft;
use super::{Shell, toasts};

/// What a curl paste replaced: the request and its draft before the paste, `None` when it
/// had none. Kept until Undo or the next paste.
pub(crate) struct CurlUndo {
    request_id: String,
    previous: Option<RequestDraft>,
}

/// The toast title after a paste.
pub(crate) const REPLACED: &str = "Request replaced from curl";

/// What `parsed` puts into the request, as the toast lists it: `Method, URL, 2 headers,
/// JSON body, basic auth`.
pub(crate) fn imported(parsed: &Parsed) -> String {
    let mut parts = vec!["Method".to_owned(), "URL".to_owned()];
    if !parsed.params.is_empty() {
        parts.push(toasts::counted(parsed.params.len(), "param"));
    }
    if !parsed.headers.is_empty() {
        parts.push(toasts::counted(parsed.headers.len(), "header"));
    }
    let body = match &parsed.body {
        Body::None => None,
        Body::Raw {
            language: RawLanguage::Json,
            ..
        } => Some("JSON body"),
        Body::Raw { .. } | Body::Graphql { .. } | Body::Binary { .. } => Some("raw body"),
        Body::Urlencoded { .. } => Some("form body"),
        Body::FormData { .. } => Some("multipart body"),
    };
    parts.extend(body.map(str::to_owned));
    let auth = match &parsed.auth {
        Auth::Basic { .. } => Some("basic auth"),
        Auth::Bearer { .. } => Some("bearer token"),
        _ => None,
    };
    parts.extend(auth.map(str::to_owned));
    parts.join(", ")
}

impl Shell {
    /// The paste into the open URL field: when the clipboard holds a curl command, it replaces
    /// the draft and the paste stops here; returns whether it did. Anything else is left to
    /// the field, which inserts it as text.
    pub(super) fn paste_into_url(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.url_edit.as_ref().map(|edit| edit.request_id.clone()) else {
            return false;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let parsed = match curl::parse(&text) {
            Ok(parsed) => parsed,
            Err(CurlError::Empty | CurlError::NotCurl(_)) => return false,
            Err(error) => {
                toasts::failure("Cannot read the curl command", error.to_string(), cx);
                return true;
            }
        };
        self.apply_curl(&id, parsed, window, cx);
        true
    }

    /// Replaces the draft of the request `id` with `parsed`, closing the URL field without
    /// committing its text, and reports it in a toast with Undo.
    fn apply_curl(
        &mut self,
        id: &str,
        parsed: Parsed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.drafts.get(id).cloned();
        let subtitle = imported(&parsed);
        self.close_url_edit(window, cx);
        let mut warnings = Vec::new();
        self.edit_draft(id, |request| warnings = parsed.apply(request), cx);
        self.curl_undo = Some(CurlUndo {
            request_id: id.to_owned(),
            previous,
        });
        let (subtitle, details) = if warnings.is_empty() {
            (subtitle, None)
        } else {
            let ignored = toasts::counted(warnings.len(), "option");
            (
                format!("{subtitle} · {ignored} ignored"),
                Some(warnings.join("\n")),
            )
        };
        let shell = cx.entity().downgrade();
        let undo = ToastAction::new("curl-paste-undo", "Undo")
            .dismisses()
            .on_click(move |cx| {
                if let Err(error) = shell.update(cx, |this, cx| this.undo_curl_paste(cx)) {
                    log::warn!("undo curl paste: {error}");
                }
            });
        toasts::undoable(REPLACED, subtitle, details, undo, cx);
    }

    /// Undo: puts the draft the last paste replaced back, or drops the draft when the
    /// request had none, and confirms it.
    pub(super) fn undo_curl_paste(&mut self, cx: &mut Context<Self>) {
        let Some(CurlUndo {
            request_id,
            previous,
        }) = self.curl_undo.take()
        else {
            return;
        };
        match previous {
            Some(previous) => {
                self.edit_draft(&request_id, |request| *request = previous.request, cx);
            }
            None => {
                if self.drafts.remove(&request_id).is_some() {
                    self.delete_stored_draft(&request_id);
                }
                cx.notify();
            }
        }
        let request = self
            .requests
            .get(&request_id)
            .map(|state| self.displayed_request(&request_id, &state.inspection.request.request));
        let shown = request.map_or_else(String::new, |request| {
            format!("{} {}", request.method, request.name)
        });
        toasts::success("Paste undone", format!("{shown} is back as it was"), cx);
    }
}

#[cfg(test)]
#[path = "curl_paste_tests.rs"]
mod tests;
