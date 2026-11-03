//! The unsaved text of a folder's document in the app state (v1-close S5b): every edit of a
//! dirty document stores its text under the folder id, beside the content hash of the
//! canonical document it was edited from, so switching to another folder and back, or
//! restarting, opens the editor on that text again. Save and Discard forget it. The text
//! lives in the engine's `document_drafts` table, apart from the request drafts, so
//! `load_drafts` never meets it.
//!
//! When the folder's canonical document no longer hashes to the stored base, the folder
//! changed underneath the text, as a request draft's file can: the text comes back marked
//! stale and saving it is refused until it is discarded, the way a conflicted request draft
//! refuses to save until it is reverted.

use gpui::Context;
use reqflux_engine::ContentHash;
use reqflux_engine::state::Draft;

use crate::shell::Shell;

/// The base of a document seeded with `seed`: its content hash, as text.
pub(super) fn base_of(seed: &str) -> String {
    ContentHash::of(seed.as_bytes()).to_string()
}

/// A stored document text: the text and the base it was edited from.
pub(super) struct StoredDocument {
    pub(super) text: String,
    pub(super) base: Option<String>,
}

impl Shell {
    /// The stored text of the folder `folder`, if any; a failed read is logged.
    pub(super) fn stored_document(&self, folder: &str) -> Option<StoredDocument> {
        let workspace_id = self.workspace_id.as_deref()?;
        let state = self.state.as_ref()?;
        match state.document_draft(workspace_id, folder) {
            Ok(stored) => stored.map(|draft| StoredDocument {
                text: draft.payload,
                base: draft.base_hash,
            }),
            Err(error) => {
                log::warn!("cannot read the document draft of folder {folder}: {error}");
                None
            }
        }
    }

    /// Stores the text of the document under edit while it is dirty, and forgets it once
    /// it is back to the seed; a failure is logged and leaves the text in the editor.
    pub(super) fn store_document_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.document_edit.as_ref() else {
            return;
        };
        let folder = edit.folder.clone();
        if !edit.dirty {
            self.forget_document(&folder);
            return;
        }
        let text = edit.input.read(cx).value().to_string();
        let base = edit.base.clone();
        let (Some(workspace_id), Some(state)) = (self.workspace_id.as_deref(), self.state.as_mut())
        else {
            return;
        };
        if let Err(error) =
            state.put_document_draft(workspace_id, &folder, Draft::new(text, Some(base)))
        {
            log::warn!("cannot store the document draft of folder {folder}: {error}");
        }
    }

    /// Forgets the stored text of the folder `folder`; a failure is logged.
    pub(super) fn forget_document(&mut self, folder: &str) {
        if let (Some(state), Some(workspace_id)) =
            (self.state.as_mut(), self.workspace_id.as_deref())
            && let Err(error) = state.delete_document_draft(workspace_id, folder)
        {
            log::warn!("cannot forget the document draft of folder {folder}: {error}");
        }
    }
}
