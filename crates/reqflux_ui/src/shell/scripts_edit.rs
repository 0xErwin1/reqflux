//! The editable Scripts pane: clicking a script, or the empty state's button, swaps it for
//! the multi-line editor the raw body uses, whose text lands in the draft's scripts as it
//! is typed. Enter inserts a newline, Ctrl+Enter sends and Escape gives the focus back to
//! the shell, keeping the text, which is already in the draft. The delete button removes
//! the script; saving then deletes its sidecar.

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, TextareaState};
use reqflux_core::model::Scripts;

use super::Shell;

/// The rows the script editor shows.
const EDITOR_ROWS: usize = 12;

/// One of a request's two scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptSlot {
    /// Runs before the request is prepared (`<slug>.pre.js`).
    PreRequest,
    /// Runs after the response arrives (`<slug>.test.js`).
    Test,
}

impl ScriptSlot {
    /// Both slots, in pane order.
    pub(crate) const ALL: [Self; 2] = [Self::PreRequest, Self::Test];

    /// The slot's part of element ids: `request-script-<id>-...`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::PreRequest => "pre",
            Self::Test => "test",
        }
    }

    /// The sidecar suffix after the request slug.
    pub(crate) fn suffix(self) -> &'static str {
        match self {
            Self::PreRequest => ".pre.js",
            Self::Test => ".test.js",
        }
    }

    /// The text of this slot in `scripts`; an empty script counts as none.
    pub(crate) fn text(self, scripts: &Scripts) -> Option<&str> {
        let text = match self {
            Self::PreRequest => scripts.pre_request.as_deref(),
            Self::Test => scripts.test.as_deref(),
        };
        text.filter(|text| !text.is_empty())
    }

    fn slot(self, scripts: &mut Scripts) -> &mut Option<String> {
        match self {
            Self::PreRequest => &mut scripts.pre_request,
            Self::Test => &mut scripts.test,
        }
    }
}

/// The open script editor: the request and script it edits and its state.
pub(crate) struct ScriptEdit {
    pub(crate) request_id: String,
    pub(crate) slot: ScriptSlot,
    pub(crate) input: Entity<TextareaState>,
    /// Puts the text into the draft on every change.
    _change: Subscription,
}

impl Shell {
    /// The scripts the Scripts pane of the request `id` shows: the draft's, else the
    /// sidecars'.
    fn shown_scripts(&self, id: &str) -> Option<Scripts> {
        let state = self.requests.get(id)?;
        Some(
            self.displayed_scripts(id, &state.inspection.request.scripts)
                .clone(),
        )
    }

    /// Swaps the script `slot` of the request `id` for a focused editor seeded with it,
    /// empty when there is none. An open cell field commits first.
    pub(super) fn start_script_edit(
        &mut self,
        id: &str,
        slot: ScriptSlot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self.script_edit_of(id, slot) {
            return;
        }
        let Some(scripts) = self.shown_scripts(id) else {
            return;
        };
        let text = slot.text(&scripts).unwrap_or_default().to_owned();
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(EDITOR_ROWS));
        input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.focus(window, cx);
        });
        let change = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some((id, slot)) = this
                    .script_edit
                    .as_ref()
                    .map(|edit| (edit.request_id.clone(), edit.slot))
            {
                let value = input.read(cx).value().to_string();
                this.set_draft_script(&id, slot, value, cx);
            }
        });
        self.script_edit = Some(ScriptEdit {
            request_id: id.to_owned(),
            slot,
            input,
            _change: change,
        });
        cx.notify();
    }

    /// Whether the editor is open over the script `slot` of the request `id`.
    pub(super) fn script_edit_of(&self, id: &str, slot: ScriptSlot) -> bool {
        self.script_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id && edit.slot == slot)
    }

    /// Puts `value` into the script `slot` of the request `id`'s draft. The text the
    /// script already has, without edited scripts in a draft, creates none.
    fn set_draft_script(
        &mut self,
        id: &str,
        slot: ScriptSlot,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let edited = self
            .drafts
            .get(id)
            .is_some_and(|draft| draft.scripts.is_some());
        if !edited
            && self
                .shown_scripts(id)
                .is_some_and(|scripts| slot.text(&scripts).unwrap_or_default() == value)
        {
            return;
        }
        self.edit_draft_scripts(id, |scripts| *slot.slot(scripts) = Some(value), cx);
    }

    /// Removes the script `slot` of the request `id` in its draft and closes its editor;
    /// saving deletes the sidecar. An open cell field commits first.
    pub(super) fn delete_script(
        &mut self,
        id: &str,
        slot: ScriptSlot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self.script_edit_of(id, slot) {
            self.close_script_edit(window, cx);
        }
        self.edit_draft_scripts(id, |scripts| *slot.slot(scripts) = None, cx);
    }

    /// Escape in the editor: closes it and gives the focus back to the shell. The text is
    /// already in the draft.
    pub(super) fn close_script_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.script_edit.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Drops the editor of the request `id`; the draft keeps what it synced.
    pub(super) fn drop_script_edit_of(&mut self, id: &str) {
        if self
            .script_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id == id)
        {
            self.script_edit = None;
        }
    }

    /// Drops the editor when it no longer belongs to the active request `id`.
    pub(super) fn drop_stale_script_edit(&mut self, id: &str) {
        if self
            .script_edit
            .as_ref()
            .is_some_and(|edit| edit.request_id != id)
        {
            self.script_edit = None;
        }
    }
}
