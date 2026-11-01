//! The draft of an edited request: what the shell holds in memory and how it persists in
//! the per-user app state. An edit never touches the workspace file until save; the draft
//! carries the whole request document, so later editable surfaces need no new storage.
//! Edited scripts ride along in the same payload: the scripts live in sidecar files, not in
//! the request document, so the payload adds them as the table [`SCRIPTS_KEY`], with the
//! hashes of the sidecars they were made from. A payload without it, written before scripts
//! were editable, loads as a draft with untouched scripts.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{Context, Window};
use reqflux_core::model::{Request, Scripts};
use reqflux_engine::SaveError;
use reqflux_engine::state::Draft;

use super::Shell;
use super::body_edit::body_cell;
use super::cell_edit::CellField;
use super::tree_model;
use crate::workspace::OpenedWorkspace;

/// The unsaved edit of a request: the document as edited, the content hash of the file the
/// edit started from, and whether the file moved underneath the draft.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RequestDraft {
    /// The whole request document, as edited.
    pub(crate) request: Request,
    /// The content hash of the file the draft was made from, as text; the base a save is
    /// checked against.
    pub(crate) base_hash: Option<String>,
    /// Whether the saved file is known to have changed underneath the draft.
    pub(crate) conflicted: bool,
    /// The edited scripts, `None` while the scripts are as saved.
    pub(crate) scripts: Option<ScriptDraft>,
}

/// The edited sidecar scripts of a draft and the content hashes, as text, of the sidecars
/// they were made from, `None` for a sidecar that did not exist.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ScriptDraft {
    pub(crate) scripts: Scripts,
    pub(crate) pre_request_base: Option<String>,
    pub(crate) test_base: Option<String>,
}

/// The payload table that holds a draft's edited scripts, beside the request document.
const SCRIPTS_KEY: &str = "reqflux_draft_scripts";

/// The stored payload of `request` and its edited `scripts`.
pub(crate) fn encode_payload(
    request: &Request,
    scripts: Option<&ScriptDraft>,
) -> Result<String, toml::ser::Error> {
    let mut table = toml::Table::try_from(request)?;
    if let Some(draft) = scripts {
        let mut stored = toml::Table::new();
        let fields = [
            ("pre_request", draft.scripts.pre_request.as_ref()),
            ("test", draft.scripts.test.as_ref()),
            ("pre_request_base", draft.pre_request_base.as_ref()),
            ("test_base", draft.test_base.as_ref()),
        ];
        for (key, value) in fields {
            if let Some(value) = value {
                stored.insert(key.to_owned(), toml::Value::String(value.clone()));
            }
        }
        table.insert(SCRIPTS_KEY.to_owned(), toml::Value::Table(stored));
    }
    toml::to_string(&table)
}

/// The request and edited scripts of a stored `payload`. A payload without the scripts
/// table, as drafts were stored before scripts were editable, has untouched scripts.
pub(crate) fn decode_payload(
    payload: &str,
) -> Result<(Request, Option<ScriptDraft>), toml::de::Error> {
    let mut table: toml::Table = toml::from_str(payload)?;
    let scripts = table.remove(SCRIPTS_KEY).map(|value| {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
        };
        ScriptDraft {
            scripts: Scripts {
                pre_request: text("pre_request"),
                test: text("test"),
            },
            pre_request_base: text("pre_request_base"),
            test_base: text("test_base"),
        }
    });
    let request = toml::Value::Table(table).try_into::<Request>()?;
    Ok((request, scripts))
}

impl Shell {
    /// Restores the drafts of the loaded workspace from the app state, inspecting their
    /// requests as drafted. A payload that does not parse is dropped with a warning, never
    /// panicked on.
    pub(crate) fn load_drafts(&mut self) {
        let Some(workspace_id) = self.workspace_id.clone() else {
            return;
        };
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let stored = match state.drafts(&workspace_id) {
            Ok(stored) => stored,
            Err(error) => {
                log::warn!("cannot read the drafts of workspace {workspace_id}: {error}");
                return;
            }
        };
        for (entity_id, stored) in stored {
            let (request, scripts) = match decode_payload(&stored.payload) {
                Ok(decoded) => decoded,
                Err(error) => {
                    log::warn!("the draft of request {entity_id} did not parse: {error}");
                    continue;
                }
            };
            let conflicted = self.file_moved(&entity_id, stored.base_hash.as_deref())
                || scripts
                    .as_ref()
                    .is_some_and(|scripts| self.scripts_moved(&entity_id, scripts));
            self.drafts.insert(
                entity_id,
                RequestDraft {
                    request,
                    base_hash: stored.base_hash,
                    conflicted,
                    scripts,
                },
            );
        }
    }

    /// Whether the request `id`'s file no longer has the content hash `base_hash` as the
    /// workspace was loaded, so a draft made from it no longer matches the file.
    fn file_moved(&self, id: &str, base_hash: Option<&str>) -> bool {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return false;
        };
        let current = engine
            .request_base_hash(id)
            .ok()
            .flatten()
            .map(|hash| hash.to_string());
        current.as_deref() != base_hash
    }

    /// Whether the sidecars of the request `id` no longer have the hashes `scripts` was
    /// made from, as the workspace was loaded.
    fn scripts_moved(&self, id: &str, scripts: &ScriptDraft) -> bool {
        self.script_bases(id).is_none_or(|(pre_request, test)| {
            pre_request != scripts.pre_request_base || test != scripts.test_base
        })
    }

    /// The hashes, as text, of the request `id`'s pre-request and test sidecars as the
    /// workspace was loaded; `None` when the request is not in the workspace.
    fn script_bases(&self, id: &str) -> Option<(Option<String>, Option<String>)> {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return None;
        };
        let hashes = engine.script_base_hashes(id).ok()?;
        let text = |hash: Option<reqflux_engine::ContentHash>| hash.map(|hash| hash.to_string());
        Some((text(hashes.pre_request), text(hashes.test)))
    }

    /// Puts `url` into the draft of the request `id`, creating the draft from the saved
    /// request when there is none, and persists it in the app state. Tests use it to make a
    /// draft without going through the URL field's query sync.
    #[cfg(test)]
    pub(super) fn set_draft_url(&mut self, id: &str, url: String, cx: &mut Context<Self>) {
        self.edit_draft(id, |request| request.url = url, cx);
    }

    /// Toggles the `enabled` state of the 1-based row `n` of `table` (`params`, `headers`
    /// or `body`) in the draft of the request `id`, creating the draft from the saved
    /// request when there is none, and persists it. Rows of the inactive sub-tab are never
    /// touched, because only the active one renders.
    pub(super) fn toggle_grid_row(
        &mut self,
        id: &str,
        table: &str,
        n: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit_draft(
            id,
            |request| {
                if let Some((_, _, enabled)) = grid_cell(request, table, n) {
                    *enabled = !*enabled;
                }
            },
            cx,
        );
    }

    /// Puts `value` into the cell (`table`, 1-based row `n`, `field`) of the draft of the
    /// request `id`, creating the draft from the saved request when there is none, and
    /// persists it. An empty commit on a cell whose value references a secret keeps the
    /// stored reference, and committing the text the cell already shows, without a draft,
    /// creates none.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_draft_cell(
        &mut self,
        id: &str,
        table: &str,
        n: usize,
        field: CellField,
        value: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) {
        if secret && value.is_empty() {
            return;
        }
        if !self.drafts.contains_key(id) {
            let current = self.requests.get(id).and_then(|state| {
                let mut request = self
                    .displayed_request(id, &state.inspection.request.request)
                    .clone();
                let (key, value, _) = grid_cell(&mut request, table, n)?;
                Some(match field {
                    CellField::Key => key.clone(),
                    CellField::Value => value.clone(),
                })
            });
            if current.as_deref() == Some(value.as_str()) {
                return;
            }
        }
        self.edit_draft(
            id,
            move |request| {
                if let Some((key, text, _)) = grid_cell(request, table, n) {
                    match field {
                        CellField::Key => *key = value,
                        CellField::Value => *text = value,
                    }
                }
            },
            cx,
        );
    }

    /// Edits the draft of the request `id` with `edit`, creating the draft from the saved
    /// request when there is none, and persists it in the app state.
    pub(super) fn edit_draft(
        &mut self,
        id: &str,
        edit: impl FnOnce(&mut Request),
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.draft_mut(id) else {
            return;
        };
        edit(&mut draft.request);
        self.store_draft(id);
        cx.notify();
    }

    /// Edits the scripts of the draft of the request `id` with `edit`, creating the draft
    /// from the saved request, and its scripts from the saved sidecars, when there are
    /// none, and persists it in the app state.
    pub(super) fn edit_draft_scripts(
        &mut self,
        id: &str,
        edit: impl FnOnce(&mut Scripts),
        cx: &mut Context<Self>,
    ) {
        let saved = self
            .requests
            .get(id)
            .map(|state| state.inspection.request.scripts.clone());
        let bases = self.script_bases(id);
        let (Some(saved), Some((pre_request_base, test_base))) = (saved, bases) else {
            return;
        };
        let Some(draft) = self.draft_mut(id) else {
            return;
        };
        let scripts = draft.scripts.get_or_insert(ScriptDraft {
            scripts: saved,
            pre_request_base,
            test_base,
        });
        edit(&mut scripts.scripts);
        self.store_draft(id);
        cx.notify();
    }

    /// The draft of the request `id`, made from the saved request when there is none;
    /// `None` when no workspace is loaded or the request is not open.
    fn draft_mut(&mut self, id: &str) -> Option<&mut RequestDraft> {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return None;
        };
        if !self.drafts.contains_key(id) {
            let base_hash = engine
                .request_base_hash(id)
                .ok()
                .flatten()
                .map(|hash| hash.to_string());
            let request = self
                .requests
                .get(id)
                .map(|state| state.inspection.request.request.clone())?;
            self.drafts.insert(
                id.to_owned(),
                RequestDraft {
                    request,
                    base_hash,
                    conflicted: false,
                    scripts: None,
                },
            );
        }
        self.drafts.get_mut(id)
    }

    /// Persists the draft of the request `id` in the app state; a failure is logged and
    /// leaves the draft in memory.
    fn store_draft(&mut self, id: &str) {
        let Some(workspace_id) = self.workspace_id.clone() else {
            return;
        };
        let Some(draft) = self.drafts.get(id).cloned() else {
            return;
        };
        let payload = match encode_payload(&draft.request, draft.scripts.as_ref()) {
            Ok(payload) => payload,
            Err(error) => {
                log::warn!("cannot serialize the draft of {id}: {error}");
                return;
            }
        };
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if let Err(error) = state.put_draft(&workspace_id, id, Draft::new(payload, draft.base_hash))
        {
            log::warn!("cannot store the draft of {id}: {error}");
        }
    }

    /// Deletes the stored draft of the request `id` from the app state; a failure is
    /// logged.
    pub(super) fn delete_stored_draft(&mut self, id: &str) {
        if let (Some(state), Some(workspace_id)) =
            (self.state.as_mut(), self.workspace_id.as_deref())
            && let Err(error) = state.delete_draft(workspace_id, id)
        {
            log::warn!("cannot forget the draft of {id}: {error}");
        }
    }

    /// The URL as written in the request of `id`, the draft's when it has one, without its
    /// params; the request line shows `shown_url` instead.
    #[cfg(test)]
    pub(super) fn displayed_url(&self, id: &str) -> String {
        match self.drafts.get(id) {
            Some(draft) => draft.request.url.clone(),
            None => self
                .requests
                .get(id)
                .map_or_else(String::new, |state| state.inspection.url.clone()),
        }
    }

    /// The request the view of `id` shows: the draft's when it has one, else `fallback`,
    /// the saved request as inspected.
    pub(super) fn displayed_request<'a>(&'a self, id: &str, fallback: &'a Request) -> &'a Request {
        self.drafts.get(id).map_or(fallback, |draft| &draft.request)
    }

    /// The scripts the Scripts pane of `id` shows: the draft's edited scripts when it has
    /// them, else `fallback`, the saved sidecars as inspected.
    pub(super) fn displayed_scripts<'a>(&'a self, id: &str, fallback: &'a Scripts) -> &'a Scripts {
        self.drafts
            .get(id)
            .and_then(|draft| draft.scripts.as_ref())
            .map_or(fallback, |draft| &draft.scripts)
    }

    /// Saves the draft of the request `id` into its file, and its edited scripts into their
    /// sidecars, hash-checked against the bases the draft was made from, and discards the
    /// draft. The request file and the sidecars are all checked before anything is written;
    /// then the sidecars are written, only when the scripts were edited, and the request
    /// file, only when its document changed. The workspace is reloaded from disk, so
    /// the tree, the request view and later sends see the saved files.
    pub(super) fn save(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_url_edit_of(id);
        self.drop_body_edit_of(id);
        self.drop_script_edit_of(id);
        // A cell field shows text the draft does not have yet: commit it, so saving never
        // silently drops what was typed.
        self.commit_cell_edit(window, cx);
        let Some(draft) = self.drafts.get(id).cloned() else {
            return;
        };
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let current = match engine.request_base_hash(id) {
            Ok(current) => current,
            Err(error) => {
                log::warn!("cannot save {id}: {error}");
                self.set_save_error(id, error.to_string(), cx);
                return;
            }
        };
        // A live draft was made from the file as this workspace loaded it; when the two no
        // longer agree, the file moved underneath the draft after a restart.
        let loaded = current.as_ref().map(|hash| hash.to_string());
        if loaded.as_deref() != draft.base_hash.as_deref()
            || draft
                .scripts
                .as_ref()
                .is_some_and(|scripts| self.scripts_moved(id, scripts))
        {
            self.mark_conflicted(id, cx);
            return;
        }
        let scripts = match &draft.scripts {
            Some(scripts) => match engine.script_base_hashes(id) {
                Ok(hashes) => Some((&scripts.scripts, hashes)),
                Err(error) => {
                    log::warn!("cannot save the scripts of {id}: {error}");
                    self.set_save_error(id, error.to_string(), cx);
                    return;
                }
            },
            None => None,
        };
        let unchanged = self
            .requests
            .get(id)
            .is_some_and(|state| state.inspection.request.request == draft.request);
        let request = (!unchanged).then_some(&draft.request);
        match engine.save_draft(id, request, current, scripts) {
            Ok(()) => {
                self.drafts.remove(id);
                self.save_errors.remove(id);
                self.delete_stored_draft(id);
                self.reload_workspace(cx);
            }
            Err(SaveError::Conflict { .. }) => self.mark_conflicted(id, cx),
            Err(error) => {
                log::warn!("cannot save {id}: {error}");
                self.set_save_error(id, error.to_string(), cx);
            }
        }
        self.focus.focus(window, cx);
    }

    /// Marks the draft of the request `id` conflicted: the saved file moved underneath it,
    /// so saving would overwrite the other edit.
    fn mark_conflicted(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(draft) = self.drafts.get_mut(id) {
            draft.conflicted = true;
            cx.notify();
        }
    }

    /// Records why the last save of the request `id`'s draft failed, so the tab row shows
    /// it the way it shows a conflict.
    fn set_save_error(&mut self, id: &str, error: String, cx: &mut Context<Self>) {
        self.save_errors.insert(id.to_owned(), error);
        cx.notify();
    }

    /// Reverts the draft of the request `id`: the view goes back to the saved file, which
    /// the workspace is reloaded for.
    pub(super) fn revert(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_url_edit_of(id);
        self.drop_cell_edit_of(id);
        self.drop_body_edit_of(id);
        self.drop_script_edit_of(id);
        if self.drafts.remove(id).is_some() {
            self.save_errors.remove(id);
            self.delete_stored_draft(id);
            self.reload_workspace(cx);
        }
        self.focus.focus(window, cx);
    }

    /// Loads the workspace from disk into the same loaded state, keeping the active
    /// environment, the open requests and their sub-tabs. A failure keeps the loaded
    /// workspace as it was.
    pub(super) fn reload_workspace(&mut self, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &mut self.workspace else {
            return;
        };
        let Some(engine) = Arc::get_mut(engine) else {
            // A send's prepare task still holds the other `Arc` clone while it runs
            // `execute` on the background executor. That task only reads the same files
            // this reload would re-read, so skipping once is safe: the next reload sees
            // them. Forcing it would need to drop the engine, whose runtime owns a send
            // in flight, and that send would end as a cancellation.
            log::warn!("cannot reload the workspace while a send is preparing");
            self.inspect_open_requests(cx);
            return;
        };
        // Re-read into the same engine: replacing it would drop the runtime that owns a send
        // in flight, and that send would end as a cancellation.
        match engine.reload() {
            Ok(()) => {
                self.workspace_id = Some(engine.workspace().manifest.id.to_string());
                let roots = tree_model::nodes(engine.workspace());
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_roots(roots, cx));
            }
            Err(error) => log::warn!("cannot reload the workspace: {error}"),
        }
        self.inspect_open_requests(cx);
    }
}

/// The key, the value and the enabled flag of the 1-based row `n` of `table` of `request`:
/// `params`, `headers`, or `body` for the urlencoded fields or the form-data parts, whose
/// file part's value is its path.
pub(super) fn grid_cell<'a>(
    request: &'a mut Request,
    table: &str,
    n: usize,
) -> Option<(&'a mut String, &'a mut String, &'a mut bool)> {
    let rows = match table {
        "body" => return body_cell(&mut request.body, n),
        "params" => &mut request.params,
        _ => &mut request.headers,
    };
    let row = rows.get_mut(n.checked_sub(1)?)?;
    Some((&mut row.key, &mut row.value, &mut row.enabled))
}

/// The map of drafts the shell keeps, by request id.
pub(crate) type Drafts = HashMap<String, RequestDraft>;
