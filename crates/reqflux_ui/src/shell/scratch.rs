//! The unsaved copy of a history entry (history-copy-and-scratch S2): a scratch tab, key
//! `scratch-<n>`, that holds the recorded request as a draft that is never stored. It is
//! editable and sendable through the place of its source request, so the engine resolves
//! scopes, auth and scripts as it does for the source, and its sends record in history under
//! the source. Save as makes it a request beside the source; closing it drops it. The
//! source request, its file and its draft are never changed.

use std::collections::HashMap;

use gpui::{Context, Window};
use reqflux_core::model::{Id, Item, Request, Workspace};

use super::Shell;
use super::documents::{Location, locate};
use super::drafts::{ModeStash, RequestDraft};
use super::request::RequestState;
use crate::workspace::OpenedWorkspace;

/// The prefix of a scratch tab's key, apart from every request id and history key.
const KEY_PREFIX: &str = "scratch-";

/// The scratch tabs of the window: each key and the request whose place it sends through.
#[derive(Debug, Default)]
pub(crate) struct Scratches {
    sources: HashMap<String, String>,
    issued: u32,
}

impl Scratches {
    /// Registers a scratch tab of the request `source` and returns its key.
    fn open(&mut self, source: &str) -> String {
        self.issued += 1;
        let key = format!("{KEY_PREFIX}{}", self.issued);
        self.sources.insert(key.clone(), source.to_owned());
        key
    }

    /// Whether `id` is the key of a scratch tab.
    pub(crate) fn is_scratch(&self, id: &str) -> bool {
        self.sources.contains_key(id)
    }

    /// The id the engine knows `id` by: the source request of a scratch key, else `id`.
    pub(crate) fn engine_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.sources.get(id).map_or(id, String::as_str)
    }

    fn forget(&mut self, key: &str) {
        self.sources.remove(key);
    }

    pub(crate) fn clear(&mut self) {
        self.sources.clear();
    }
}

/// The container of the request `id` and the items beside it.
fn place_of<'a>(workspace: &'a Workspace, id: &str) -> Option<(Id, &'a [Item])> {
    fn holds(items: &[Item], id: &str) -> bool {
        items
            .iter()
            .any(|item| matches!(item, Item::Request(node) if node.request.id.to_string() == id))
    }
    fn walk<'a>(items: &'a [Item], id: &str) -> Option<(Id, &'a [Item])> {
        items.iter().find_map(|item| match item {
            Item::Folder(node) if holds(&node.items, id) => Some((node.folder.id, &node.items[..])),
            Item::Folder(node) => walk(&node.items, id),
            Item::Request(_) => None,
        })
    }
    workspace.collections.iter().find_map(|node| {
        if holds(&node.items, id) {
            Some((node.collection.id, &node.items[..]))
        } else {
            walk(&node.items, id)
        }
    })
}

/// `<name> copy`, else `<name> copy 2`, ... the first that no item of `siblings` has as a
/// name or a slug.
fn free_name(name: &str, siblings: &[Item]) -> String {
    let taken = |candidate: &str| {
        let lowered = candidate.to_lowercase();
        let slug = lowered.replace(' ', "-");
        siblings.iter().any(|item| {
            let (other, slug_of) = match item {
                Item::Request(node) => (&node.request.name, &node.slug),
                Item::Folder(node) => (&node.folder.name, &node.slug),
            };
            other.to_lowercase() == lowered || slug_of.to_lowercase() == slug
        })
    };
    let first = format!("{name} copy");
    if !taken(&first) {
        return first;
    }
    (2_u32..)
        .map(|n| format!("{name} copy {n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or(first)
}

impl Shell {
    /// Open as unsaved copy: opens the recorded document of the history tab in a scratch
    /// tab and activates it. The source request, its tab, its draft and its file are
    /// untouched. Does nothing when the source no longer exists; a recorded document that
    /// does not parse shows a fixed error toast, since the parse error would quote it.
    pub(super) fn open_history_copy(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.orphan.as_ref().filter(|tab| tab.source_exists) else {
            return;
        };
        let source = tab.request_id.clone();
        let Some(reopened) = self
            .sends
            .get(&tab.key())
            .and_then(|send| send.history.clone())
            .filter(|reopened| reopened.resendable)
        else {
            return;
        };
        let request = match toml::from_str::<Request>(&reopened.document) {
            Ok(request) => request,
            Err(_) => {
                log::warn!("the recorded document of {source} did not parse");
                super::toasts::failure(
                    "Cannot open this entry",
                    "The recorded request could not be read.",
                    cx,
                );
                return;
            }
        };
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let inspection = match engine.inspect_draft_current(
            &source,
            Some(&request),
            self.active_environment.as_deref(),
            &self.current,
        ) {
            Ok(inspection) => inspection,
            Err(error) => {
                super::toasts::failure("Cannot open this entry", error.to_string(), cx);
                return;
            }
        };
        let key = self.scratch.open(&source);
        self.requests
            .insert(key.clone(), RequestState::new(inspection));
        self.drafts.insert(
            key.clone(),
            RequestDraft {
                request,
                base_hash: None,
                conflicted: false,
                scripts: None,
                stash: ModeStash::default(),
            },
        );
        self.open_document(&key, cx);
    }

    /// The tab-row location of `id`: the tree location of a request; for a scratch tab, the
    /// place of its source with the copy's own name and method.
    pub(super) fn tab_location(&self, id: &str) -> Option<Location> {
        if !self.scratch.is_scratch(id) {
            return locate(&self.roots, id);
        }
        let draft = self.drafts.get(id)?;
        let place = locate(&self.roots, self.scratch.engine_id(id));
        Some(Location {
            collection: place
                .as_ref()
                .map_or_else(String::new, |place| place.collection.clone()),
            folders: place.map_or_else(Vec::new, |place| place.folders),
            name: draft.request.name.clone(),
            method: draft.request.method.as_str().to_owned(),
        })
    }

    /// Drops the scratch tab `id`: its draft, edit history and error. Does nothing for a
    /// request id.
    pub(super) fn drop_scratch(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.scratch.is_scratch(id) {
            return;
        }
        self.scratch.forget(id);
        self.drafts.remove(id);
        self.reinspections.remove(id);
        cx.default_global::<super::drafts::draft_history::DraftHistories>()
            .forget(id);
    }

    /// Save as (Ctrl+S in a scratch tab): creates a request from the copy beside its source
    /// under a free name, `<name> copy`, reloads the workspace, closes the scratch tab and
    /// opens the new request. The source is never written. A failure keeps the scratch tab
    /// and says why in an error toast.
    pub(super) fn save_scratch(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_url_edit_of(key);
        self.drop_body_edit_of(key);
        self.drop_script_edit_of(key);
        self.drop_docs_edit_of(key);
        self.commit_cell_edit(window, cx);
        let Some(mut request) = self.drafts.get(key).map(|draft| draft.request.clone()) else {
            return;
        };
        let source = self.scratch.engine_id(key).to_owned();
        let created = match &mut self.workspace {
            OpenedWorkspace::Loaded(engine) => {
                let Some(engine) = std::sync::Arc::get_mut(engine) else {
                    super::toasts::failure(
                        "Cannot save the copy",
                        "a send is being prepared; save again once it finished",
                        cx,
                    );
                    return;
                };
                let Some((parent, siblings)) = place_of(engine.workspace(), &source) else {
                    super::toasts::failure(
                        "Cannot save the copy",
                        "its source request no longer exists, so there is no place to save it",
                        cx,
                    );
                    return;
                };
                request.name = free_name(&request.name, siblings);
                request.id = Id::new();
                engine
                    .create_request_as(parent, request)
                    .map_err(|error| error.to_string())
            }
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => return,
        };
        match created {
            Ok(id) => {
                self.close_document(key, cx);
                if !self.reload_workspace(cx) {
                    super::toasts::reload_failed("Saved", cx);
                }
                self.open_document(&id.to_string(), cx);
            }
            Err(error) => super::toasts::failure("Cannot save the copy", error, cx),
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
