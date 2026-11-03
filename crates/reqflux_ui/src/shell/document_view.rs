//! The Document layout's view (v1-features S7, design/layouts/02): the active request's
//! folder as one `.http` document, the sections `reqflux_core::http_document` writes from
//! the saved requests (the draft's text when a request has one). The lines draw like the
//! response body's, one `document-section-<n>` per request and a `document-line-<n>` per
//! line; a section's method line takes the method colour the request line uses and its
//! `###` line is muted. Clicking the document swaps it for the multi-line editor the raw
//! body uses (`document-editor`), seeded from `write` of the folder and refreshed
//! underneath while the text is not dirty. Ctrl+Enter or `document-send` sends the
//! section the cursor is in through the engine, its response under that section
//! (`document-section-<n>-response`, see `response::document_block`); Ctrl+S or
//! `document-save` parses the text and saves it through the engine (see
//! `save_document`), `document-discard` drops the edit, and `document-warnings` shows what
//! a broken section blocks. Without an open request the view shows an empty state.
// bolt gap: `bolt` has no read-only code view with line numbers and no plain field well
// outside the request module; the lines are drawn as the Body pane draws its code and the
// wells as its `text_well`.

use std::ops::Range;
use std::str::FromStr;
use std::sync::Arc;

use bolt::controls::{Button, InputEnter};
use bolt::primitives::{BannerBlock, BannerVariant, Chamfer};
use bolt::tokens::{ChamferCut, EditorMetrics, GridMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, Entity, IntoElement, ParentElement, Point, Role, Stateful,
    Styled, Subscription, Window, anchored, deferred, div, prelude::*,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit_assets::IconName;
use reqflux_core::http_document::{self, DocumentRequest};
use reqflux_core::model::{Body, Id, Request};
use reqflux_engine::config::LayoutMode;

use crate::shell::{
    Shell, history, region,
    request::ui,
    response,
    rows::{labeled_row, part},
    send::RequestSend,
    toasts, tree_model,
    tree_model::{Node, NodeKind},
};
use crate::tokens::method_hue;
use crate::workspace::OpenedWorkspace;

/// The empty state while no request is open.
const NO_REQUEST: &str = "No request is open";
/// The empty state while the active request's folder shows no request.
const NO_FOLDER: &str = "This folder has no requests to show";
/// The rows the document editor shows before it grows with its text.
const EDITOR_ROWS: usize = 20;

/// The `.http` document under edit: the folder it belongs to, the editor state, the text
/// it was seeded from, whether the text has changed, the warnings the last parse produced,
/// and the removed requests the pending save still has to confirm.
pub(crate) struct DocumentEdit {
    folder: String,
    input: Entity<TextareaState>,
    seed: String,
    dirty: bool,
    warnings: Vec<String>,
    confirm: Option<Vec<String>>,
    _change: Subscription,
}

pub(crate) fn render(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let view = region("document-view")
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT);
    let Some((folder, sections)) = document_sections(shell) else {
        return view.child(centered(NO_REQUEST));
    };
    if sections.is_empty() {
        return view.child(centered(NO_FOLDER));
    }
    shell.refresh_document_edit(&folder, &sections, window, cx);
    if shell.document_edit.is_some() {
        let mut items = edited_document(shell, cx);
        if let Some(block) = edited_response(shell, cx) {
            items.push(block);
        }
        return view.children(items);
    }
    let text = http_document::write(&plain(&sections));
    let shown = shown_response(shell, &sections, cx);
    view.child(
        document_well("document-text", cx)
            .cursor_text()
            .on_click(cx.listener(|this, _event, window, cx| this.start_document_edit(window, cx)))
            .children(section_groups(&sections, &text, shown, cx)),
    )
}

/// The empty state of the view: `message` centered over the island.
fn centered(message: &str) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .child(bolt::composites::EmptyState::new(
            ui::icon(IconName::File),
            message,
        ))
}

/// A field well `id`, drawn as the Body pane draws its raw text (see
/// `request/body/controls`).
fn document_well(id: &str, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id.to_owned())
        .relative()
        .mx(Spacing::MD)
        .my(Spacing::SM)
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
}

/// The document under edit: the toolbar (dirty marker, Discard, Save), the editor and the
/// warnings banner; the deletion confirmation renders over the window (`render_confirm`).
fn edited_document(shell: &Shell, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let Some(edit) = shell.document_edit.as_ref() else {
        return Vec::new();
    };
    let warnings = edit.warnings.clone();
    let mut items = vec![document_toolbar(edit.dirty, cx).into_any_element()];
    items.push(document_editor(shell, cx).into_any_element());
    if !warnings.is_empty() {
        items.push(
            part("document-warnings".to_owned())
                .flex_none()
                .mx(Spacing::MD)
                .role(Role::Label)
                .aria_label(warnings.join(" "))
                .child(
                    BannerBlock::new(BannerVariant::Danger, "This document has warnings")
                        .default_icon(ui::icon(IconName::TriangleAlert))
                        .with_body(warnings.join("\n")),
                )
                .into_any_element(),
        );
    }
    items
}

/// The toolbar of the document under edit: the dirty marker `document-dirty` while the
/// text changed, then Discard and Save.
fn document_toolbar(dirty: bool, cx: &mut Context<Shell>) -> Stateful<Div> {
    part("document-toolbar".to_owned())
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .gap(Spacing::SM)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pt(Spacing::SM)
        .pb(Spacing::SM)
        .when(dirty, |toolbar| {
            toolbar.child(
                part("document-dirty".to_owned())
                    .size(crate::tokens::DIRTY_DOT)
                    .rounded_full()
                    .bg(bolt::brand::brand(cx).accent_tint),
            )
        })
        .child(
            Button::new("document-send", "Send")
                .icon(ui::icon(IconName::Send))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.send_document_section(window, cx);
                })),
        )
        .child(
            Button::new("document-discard", "Discard")
                .on_click(cx.listener(|this, _event, _window, cx| this.discard_document(cx))),
        )
        .child(
            Button::new("document-save", "Save")
                .primary()
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.save_document(false, window, cx);
                })),
        )
}

/// The multi-line editor of the document under edit, `document-editor`, the raw body's
/// editor pattern over the folder's whole text.
fn document_editor(shell: &Shell, cx: &mut Context<Shell>) -> Stateful<Div> {
    let Some(edit) = shell.document_edit.as_ref() else {
        return part("document-editor".to_owned());
    };
    let input = edit.input.clone();
    document_well("document-editor", cx)
        // Ctrl+Enter sends the section the cursor is in; the editor would otherwise
        // consume the secondary Enter as its own, as the GraphQL editor's does.
        .capture_action(cx.listener(|this, action: &InputEnter, window, cx| {
            if action.secondary {
                cx.stop_propagation();
                this.send_document_section(window, cx);
            }
        }))
        .child(
            Textarea::new(&input)
                .appearance(false)
                .bordered(false)
                .text_size(EditorMetrics::CODE_FONT)
                .font_family(bolt::fonts::editor_family(cx))
                .text_color(bolt::colors(cx).foreground),
        )
}

/// The deletion confirmation, `document-confirm-delete`, over the window: the requests no
/// section covers any more, which saving will delete; Cancel (`document-confirm-cancel`)
/// keeps them and Save (`document-confirm-save`) deletes them with the rest of the save.
pub(crate) fn render_confirm(
    shell: &Shell,
    _window: &mut Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let removed = shell.document_edit.as_ref()?.confirm.as_ref()?.join(", ");
    let modal = bolt::modals::Modal::new("Delete removed requests?")
        .danger()
        .body(
            region("document-confirm-delete")
                .role(Role::Dialog)
                .aria_label(format!("Saving deletes: {removed}"))
                .flex()
                .flex_col()
                .gap(Spacing::SM)
                .child(div().child(format!(
                    "These requests are no longer in the document and saving deletes them: \
                     {removed}."
                ))),
        )
        .footer(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(Spacing::SM)
                .child(Button::new("document-confirm-cancel", "Cancel").on_click(
                    cx.listener(|this, _event, _window, cx| this.cancel_document_confirm(cx)),
                ))
                .child(
                    Button::new("document-confirm-save", "Save")
                        .primary()
                        .on_click(cx.listener(|this, _event, window, cx| {
                            this.save_document(true, window, cx);
                        })),
                ),
        );
    let viewport = _window.viewport_size();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                deferred(
                    anchored().position(Point::default()).child(
                        div()
                            .relative()
                            .w(viewport.width)
                            .h(viewport.height)
                            .child(modal),
                    ),
                )
                .with_priority(3),
            )
            .into_any_element(),
    )
}

impl Shell {
    /// Whether the Document layout's document edit has unsaved text; the commands use it
    /// to make Ctrl+S save the document.
    pub(in crate::shell) fn document_dirty(&self) -> bool {
        self.split.mode == LayoutMode::Document
            && self.document_edit.as_ref().is_some_and(|edit| edit.dirty)
    }

    /// Opens the document editor over the read-only text, seeded with the canonical
    /// document of the active request's folder. The editor stays open until the text is
    /// saved or discarded.
    pub(in crate::shell) fn start_document_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_edit.is_some() {
            return;
        }
        let Some((folder, sections)) = document_sections(self) else {
            return;
        };
        let seed = http_document::write(&plain(&sections));
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(EDITOR_ROWS));
        input.update(cx, |state, cx| state.set_value(seed.clone(), window, cx));
        let change = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let value = input.read(cx).value().to_string();
                if let Some(edit) = this.document_edit.as_mut() {
                    edit.dirty = value != edit.seed;
                    edit.warnings = http_document::parse(&value).1;
                    cx.notify();
                }
            }
        });
        self.document_edit = Some(DocumentEdit {
            folder,
            input,
            seed,
            dirty: false,
            warnings: Vec::new(),
            confirm: None,
            _change: change,
        });
        cx.notify();
    }

    /// Re-seeds the open editor when the folder changed underneath and the text is not
    /// dirty; drops an editor that belongs to another folder.
    fn refresh_document_edit(
        &mut self,
        folder: &str,
        sections: &[(String, DocumentRequest)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .document_edit
            .as_ref()
            .is_some_and(|edit| edit.folder != folder)
        {
            self.document_edit = None;
        }
        let Some(edit) = self.document_edit.as_mut() else {
            return;
        };
        let seed = http_document::write(&plain(sections));
        if !edit.dirty && edit.seed != seed {
            edit.seed = seed.clone();
            edit.warnings = Vec::new();
            edit.input
                .update(cx, |state, cx| state.set_value(seed, window, cx));
            cx.notify();
        }
    }

    /// Puts `text` into the open document editor, marking the edit and its warnings like a
    /// keystroke does. Tests use it instead of typing.
    #[cfg(test)]
    pub(in crate::shell) fn set_document_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.document_edit.as_mut() else {
            return;
        };
        edit.input
            .update(cx, |state, cx| state.set_value(text.to_owned(), window, cx));
        edit.dirty = text != edit.seed;
        edit.warnings = http_document::parse(text).1;
        cx.notify();
    }

    /// Ctrl+Enter (`document-send`) in the Document layout: sends the section the cursor
    /// is in — the last one that starts before it when it sits between sections. The send
    /// goes through the engine like any send, with the unsaved document text in place of
    /// the saved request, so it records history and honours the environment allowlist and
    /// the host policy. Saving is not required.
    pub(in crate::shell) fn send_document_section(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, section)) = self.section_at_cursor(cx) else {
            return;
        };
        self.send_folder_section(&id, section, window, cx);
    }

    /// The response block's Retry: sends the section of the request `id` again, from the
    /// document as it shows now.
    pub(in crate::shell) fn resend_document_section(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = document_text(self, cx) else {
            return;
        };
        let section = http_document::parse(&text)
            .0
            .into_iter()
            .find(|section| request_of_name(self, &section.name).as_deref() == Some(id));
        if let Some(section) = section {
            self.send_folder_section(id, section, window, cx);
        }
    }

    /// The request the section the cursor is in sends as: its id in the folder and the
    /// section itself. `None` while the editor is closed (no cursor) or the section has
    /// no request in the folder.
    fn section_at_cursor(&self, cx: &App) -> Option<(String, DocumentRequest)> {
        let edit = self.document_edit.as_ref()?;
        let text = edit.input.read(cx).value().to_string();
        let index = section_at(&text, edit.input.read(cx).cursor())?;
        let section = http_document::parse(&text).0.into_iter().nth(index)?;
        let id = request_of_name(self, &section.name)?;
        Some((id, section))
    }

    /// Sends `section` as the request `id`: the request the section saves to (its auth,
    /// settings and extensions kept, its non-raw body the document cannot show), through
    /// the engine like any send.
    fn send_folder_section(
        &mut self,
        id: &str,
        section: DocumentRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => {
                history::find_request(engine.workspace(), id).map(|(_, request)| request.clone())
            }
            _ => None,
        };
        let Some(saved) = saved else {
            return;
        };
        let request = updated_request(&saved, &section);
        self.send_document(id, Some(request), window, cx);
    }

    /// Puts the document cursor `offset` bytes into the open editor, the way clicking
    /// there does. Tests use it to select the section a Ctrl+Enter sends.
    #[cfg(test)]
    pub(in crate::shell) fn set_document_cursor(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.document_edit.as_ref() else {
            return;
        };
        let input = edit.input.clone();
        input.update(cx, |state, cx| {
            let offset = offset.min(state.text().len());
            let position = gpui_component::input::RopeExt::offset_to_position(state.text(), offset);
            state.set_cursor_position(position, window, cx);
        });
    }

    /// Drops the document edit: the view goes back to the read-only document of the saved
    /// requests, and nothing is written.
    pub(in crate::shell) fn discard_document(&mut self, cx: &mut Context<Self>) {
        if self.document_edit.take().is_some() {
            cx.notify();
        }
    }

    /// Cancels the deletion confirmation: the modal closes and the edit keeps its text.
    pub(in crate::shell) fn cancel_document_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.document_edit.as_mut()
            && edit.confirm.take().is_some()
        {
            cx.notify();
        }
    }

    /// Saves the document (Ctrl+S, `document-save`): parses the text, updates the requests
    /// the sections cover (method, URL and params, enabled headers, raw body), creates the
    /// requests the new sections name, and — after one confirmation for the whole save —
    /// deletes the requests whose section disappeared. Warnings block the save; a failed
    /// save keeps the edit and says why in a toast.
    pub(in crate::shell) fn save_document(
        &mut self,
        confirmed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match self.document_edit.as_ref() {
            Some(edit) => edit.input.read(cx).value().to_string(),
            None => return,
        };
        let (sections, warnings) = http_document::parse(&text);
        if !warnings.is_empty() {
            if let Some(edit) = self.document_edit.as_mut() {
                edit.warnings = warnings;
            }
            cx.notify();
            return;
        }
        let Some((folder, ids)) = active_folder(self) else {
            return;
        };
        let mut updates: Vec<(String, DocumentRequest)> = Vec::new();
        let mut creates: Vec<DocumentRequest> = Vec::new();
        let mut removes: Vec<String> = Vec::new();
        for id in &ids {
            let Some(node) = tree_model::find(&self.roots, id) else {
                continue;
            };
            match sections.iter().find(|section| section.name == node.name) {
                Some(section) => updates.push((id.clone(), section.clone())),
                None => removes.push(id.clone()),
            }
        }
        for section in &sections {
            let covered = ids.iter().any(|id| {
                tree_model::find(&self.roots, id).is_some_and(|node| node.name == section.name)
            });
            if !covered {
                creates.push(section.clone());
            }
        }
        let removed: Vec<String> = removes
            .iter()
            .filter_map(|id| tree_model::find(&self.roots, id).map(|node| node.name.clone()))
            .collect();
        if !removed.is_empty() && !confirmed {
            if let Some(edit) = self.document_edit.as_mut() {
                edit.confirm = Some(removed);
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.document_edit.as_mut() {
            edit.confirm = None;
        }
        match self.write_document(&folder, &updates, &creates, &removes) {
            Ok(()) => {
                for id in updates.iter().map(|(id, _)| id).chain(removes.iter()) {
                    self.drafts.remove(id);
                    self.delete_stored_draft(id);
                    self.save_errors.remove(id);
                }
                for id in &removes {
                    self.documents.close(id);
                }
                self.document_edit = None;
                if !self.reload_workspace(cx) {
                    toasts::reload_failed("Saved", cx);
                }
            }
            Err(error) => toasts::failure("Cannot save the document", error, cx),
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Writes the parsed sections through the engine: the updates, then the creates, then
    /// the deletes, each file hash-checked. `Ok` only when every file was written.
    fn write_document(
        &mut self,
        folder: &str,
        updates: &[(String, DocumentRequest)],
        creates: &[DocumentRequest],
        removes: &[String],
    ) -> Result<(), String> {
        let OpenedWorkspace::Loaded(engine) = &mut self.workspace else {
            return Ok(());
        };
        let Some(engine) = Arc::get_mut(engine) else {
            return Err("a send is being prepared; save again once it finished".to_owned());
        };
        let parent = Id::from_str(folder).map_err(|error| error.to_string())?;
        for (id, section) in updates {
            let Some((_, saved)) = history::find_request(engine.workspace(), id) else {
                return Err(format!("request {id} is not in the workspace"));
            };
            let request = updated_request(saved, section);
            let expected = engine
                .request_base_hash(id)
                .map_err(|error| error.to_string())?;
            engine
                .save_request(id, &request, expected)
                .map_err(|error| error.to_string())?;
        }
        for section in creates {
            engine
                .create_request_as(parent, section.to_request(Id::new()))
                .map_err(|error| error.to_string())?;
        }
        for id in removes {
            let id = Id::from_str(id).map_err(|error| error.to_string())?;
            engine.delete(id).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// The request `saved` becomes when its section saves: the section's method, URL (its
/// query string as params), headers and raw body; the auth, settings, extensions and a
/// non-raw body the document cannot show are kept.
fn updated_request(saved: &Request, section: &DocumentRequest) -> Request {
    let mut updated = section.to_request(saved.id);
    updated.auth = saved.auth.clone();
    updated.settings = saved.settings;
    updated.extensions = saved.extensions.clone();
    if section.body.is_empty() && !matches!(saved.body, Body::Raw { .. } | Body::None) {
        updated.body = saved.body.clone();
    }
    updated
}

/// The active request's container: its id and its request ids, in tree order; `None`
/// when no request is open.
fn active_folder(shell: &Shell) -> Option<(String, Vec<String>)> {
    let active = shell.documents.active()?;
    let folder = container_of(&shell.roots, active)?;
    Some((folder.id.clone(), folder.request_ids()))
}

/// The `.http` sections of the active request's container's requests, in tree order, the
/// draft's text when a request has one, each with its request id; `None` when no request
/// is open.
fn document_sections(shell: &Shell) -> Option<(String, Vec<(String, DocumentRequest)>)> {
    let (folder, ids) = active_folder(shell)?;
    let mut shown = Vec::new();
    if let OpenedWorkspace::Loaded(engine) = &shell.workspace {
        let workspace = engine.workspace();
        for id in ids {
            if let Some((_, saved)) = history::find_request(workspace, &id) {
                let section = document_section(shell.displayed_request(&id, saved));
                shown.push((id, section));
            }
        }
    }
    Some((folder, shown))
}

/// The sections `write` and `refresh_document_edit` consume, without their request ids.
fn plain(sections: &[(String, DocumentRequest)]) -> Vec<DocumentRequest> {
    sections
        .iter()
        .map(|(_, section)| section.clone())
        .collect()
}

/// The text the document shows: the editor's while it is open, else the canonical
/// document of the saved sections.
fn document_text(shell: &Shell, cx: &App) -> Option<String> {
    match shell.document_edit.as_ref() {
        Some(edit) => Some(edit.input.read(cx).value().to_string()),
        None => {
            document_sections(shell).map(|(_, sections)| http_document::write(&plain(&sections)))
        }
    }
}

/// The id of the folder request a section of `name` belongs to, the match a save makes.
fn request_of_name(shell: &Shell, name: &str) -> Option<String> {
    let (_, ids) = active_folder(shell)?;
    ids.into_iter()
        .find(|id| tree_model::find(&shell.roots, id).is_some_and(|node| node.name == name))
}

/// The response block the read-only document shows, at its section: the request the
/// window sent last, or the active request when that one has no section here; `None`
/// while neither was sent.
fn shown_response(
    shell: &Shell,
    sections: &[(String, DocumentRequest)],
    cx: &mut Context<Shell>,
) -> Option<(usize, AnyElement)> {
    let target = shell
        .last_send
        .as_ref()
        .map(|send| send.id.as_str())
        .filter(|id| sections.iter().any(|(section_id, _)| section_id == id))
        .or_else(|| shell.documents.active())?;
    let (index, (id, section)) = sections
        .iter()
        .enumerate()
        .find(|(_, (section_id, _))| section_id == target)?;
    let send = shell.sends.get(id)?;
    Some((index, section_block(index, id, &section.name, send, cx)))
}

/// The response block under the editor: the section the cursor is in, when its request
/// was sent.
fn edited_response(shell: &Shell, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let text = document_text(shell, cx)?;
    let index = section_at(&text, shell.document_edit.as_ref()?.input.read(cx).cursor())?;
    let section = http_document::parse(&text).0.into_iter().nth(index)?;
    let id = request_of_name(shell, &section.name)?;
    let send = shell.sends.get(&id)?;
    Some(section_block(index, &id, &section.name, send, cx))
}

/// The response block of the section `index`: `document-section-<n>-response` over the
/// response module's `document-response`.
fn section_block(
    index: usize,
    id: &str,
    name: &str,
    send: &RequestSend,
    cx: &mut Context<Shell>,
) -> AnyElement {
    part(format!("document-section-{}-response", index + 1))
        .flex_none()
        .child(response::document_block(id, name, send, cx))
        .into_any_element()
}

/// The section of `text` the cursor `offset` falls in, as its index in `parse`'s order:
/// the last one that starts before the cursor when it sits between sections; `None`
/// before the first one.
fn section_at(text: &str, offset: usize) -> Option<usize> {
    section_ranges(text)
        .iter()
        .rposition(|range| range.start <= offset)
}

/// The byte range each section of `text` spans, `parse`'s sections in order: from the
/// separator, or the first content line of the unnamed first section, to where the next
/// section starts — the end of the text for the last one. A `###` line opens a section
/// under the same rule `parse` applies.
fn section_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut start: Option<usize> = None;
    let mut in_body = false;
    let mut previous_blank = false;
    let mut offset = 0_usize;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let content = line.trim_end();
        let blank = content.trim().is_empty();
        if content.starts_with("###") && (!in_body || previous_blank) {
            if let Some(from) = start.take() {
                ranges.push(from..line_start);
            }
            start = Some(line_start);
            in_body = false;
            previous_blank = false;
            continue;
        }
        match start {
            None => {
                if !blank && !is_head_comment(content) {
                    start = Some(line_start);
                }
            }
            Some(_) if blank && !in_body => in_body = true,
            Some(_) => {}
        }
        previous_blank = blank;
    }
    if let Some(from) = start {
        ranges.push(from..offset);
    }
    ranges
}

/// A head comment line, the ones `parse` ignores: a trimmed `#` or `//` line.
fn is_head_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// The container that directly holds the request `id`: the collection or folder whose
/// subtree lists it. A request's folder is the collection or folder it sits in directly;
/// the document covers every request under that container, its subfolders included.
fn container_of<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| match &node.kind {
        NodeKind::Collection | NodeKind::Folder => {
            if node.children.iter().any(|child| child.id == id) {
                Some(node)
            } else {
                container_of(&node.children, id)
            }
        }
        NodeKind::Request { .. } | NodeKind::Example { .. } => None,
    })
}

/// The section `write` emits for `request`: the name, the method, the URL with the
/// enabled params appended after the separator `?` or `&` the URL asks for, the enabled
/// headers, and the raw body; the other body kinds have no `.http` text yet.
fn document_section(request: &Request) -> DocumentRequest {
    DocumentRequest {
        name: request.name.clone(),
        method: request.method.clone(),
        url: full_url(request),
        headers: request
            .headers
            .iter()
            .filter(|header| header.enabled)
            .cloned()
            .collect(),
        body: match &request.body {
            Body::Raw { text, .. } => text.clone(),
            Body::None
            | Body::Urlencoded { .. }
            | Body::FormData { .. }
            | Body::Binary { .. }
            | Body::Graphql { .. } => String::new(),
        },
    }
}

/// The URL of `request` as written, its enabled params appended after `?`, or `&` when
/// the URL already carries a query.
fn full_url(request: &Request) -> String {
    let params: Vec<String> = request
        .params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| format!("{}={}", param.key, param.value))
        .collect();
    if params.is_empty() {
        return request.url.clone();
    }
    let separator = if request.url.contains('?') { "&" } else { "?" };
    format!("{}{separator}{}", request.url, params.join("&"))
}

/// One kind of document line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    /// A `### <name>` separator line.
    Section,
    /// The `<METHOD> <url>` line that opens a section.
    Method,
    /// A header, a blank line or a body line.
    Text,
}

/// The sections of `text`, `write`'s output for `sections`, as one
/// `document-section-<n>` each, its lines numbered across the whole document, and the
/// response `block` under the section it belongs to.
fn section_groups(
    sections: &[(String, DocumentRequest)],
    text: &str,
    block: Option<(usize, AnyElement)>,
    cx: &App,
) -> Vec<AnyElement> {
    let mut groups: Vec<Vec<(String, LineKind)>> = Vec::new();
    let mut after_section = false;
    for line in text.lines() {
        if line.starts_with("###") {
            groups.push(vec![(line.to_owned(), LineKind::Section)]);
            after_section = true;
        } else {
            let kind = if after_section && !line.is_empty() {
                LineKind::Method
            } else {
                LineKind::Text
            };
            if let Some(group) = groups.last_mut() {
                group.push((line.to_owned(), kind));
            }
            after_section = false;
        }
    }
    let mut rows = Vec::new();
    let mut number = 0usize;
    let mut block = block;
    for (index, ((_, section), group)) in sections.iter().zip(groups).enumerate() {
        let method = section.method.as_str().to_owned();
        let mut section_view = part(format!("document-section-{}", index + 1))
            .flex_none()
            .flex()
            .flex_col()
            .children(group.into_iter().map(|(text, kind)| {
                number += 1;
                line_row(number, &text, kind, &method, cx).into_any_element()
            }));
        if block.as_ref().is_some_and(|(at, _)| *at == index)
            && let Some((_, block)) = block.take()
        {
            section_view = section_view.child(block);
        }
        rows.push(section_view.into_any_element());
    }
    rows
}

/// One numbered line of the document, as the response body draws its lines: the number in
/// the gutter and the text beside it, a section line muted, a method line in the method's
/// hue and everything else in the foreground.
fn line_row(number: usize, text: &str, kind: LineKind, method: &str, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let color = match kind {
        LineKind::Section => colors.muted,
        LineKind::Method => method_hue(method, cx),
        LineKind::Text => colors.foreground,
    };
    labeled_row(format!("document-line-{number}"), text.to_owned())
        .flex()
        .items_center()
        .h(EditorMetrics::CODE_LINE_HEIGHT)
        .child(
            div()
                .w(GridMetrics::ROW_NUMBER_WIDTH)
                .flex_none()
                .pr(Spacing::MD)
                .text_right()
                .text_color(colors.line_2)
                .child(number.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .flex()
                .whitespace_nowrap()
                .text_color(color)
                .child(text.to_owned()),
        )
}

#[cfg(test)]
#[path = "document_view_tests.rs"]
mod tests;
