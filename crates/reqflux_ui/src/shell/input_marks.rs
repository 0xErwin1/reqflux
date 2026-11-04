//! The `{{name}}` marks of a field while it is being edited (a single-line field, or the raw
//! body's code editor), drawn as the read-only text draws them ([`super::request::spans`]): bolt's input decorations style the
//! reference ranges the engine's inspection finds in the field's text. The ranges follow
//! edits but are never detected, so every change of the text, and every `set_value`, sets
//! them again from the text as it is.

use gpui::{App, Context, Entity, Subscription};
use gpui_component::input::{EditorState, InputEvent, InputState};
use reqflux_engine::{Inspection, Span};

use bolt::controls::{TextDecoration, TextDecorationCollection, decorate_input};

use super::Shell;
use super::request::spans;

/// The marks of an open field: the decorations installed on its input, the styles they were
/// set with, and the subscription that sets them again on every change of the text.
pub(crate) struct InputMarks {
    collection: TextDecorationCollection,
    decorations: Vec<TextDecoration>,
    _change: Subscription,
}

impl InputMarks {
    /// The marks as the field draws them now: the tracked ranges with their styles.
    #[cfg(test)]
    pub(crate) fn shown(&self, cx: &App) -> Vec<TextDecoration> {
        self.collection
            .get_ranges(cx)
            .into_iter()
            .zip(&self.decorations)
            .map(|(range, decoration)| TextDecoration::new(range, decoration.style))
            .collect()
    }
}

/// The marks of the open raw body editor: the decorations installed on its editor and the
/// styles they were set with. [`Shell::remark_body_edit`] sets them again on every change.
pub(crate) struct EditorMarks {
    collection: TextDecorationCollection,
    decorations: Vec<TextDecoration>,
}

impl EditorMarks {
    /// The marks as the editor draws them now: the tracked ranges with their styles.
    #[cfg(test)]
    pub(crate) fn shown(&self, cx: &App) -> Vec<TextDecoration> {
        self.collection
            .get_ranges(cx)
            .into_iter()
            .zip(&self.decorations)
            .map(|(range, decoration)| TextDecoration::new(range, decoration.style))
            .collect()
    }
}

/// The decorations of the references in `text`, each over its `{{...}}` in UTF-8 bytes,
/// styled as resolving or not by `inspection`.
pub(crate) fn decorations(inspection: &Inspection, text: &str, cx: &App) -> Vec<TextDecoration> {
    let mut offset = 0;
    let mut decorations = Vec::new();
    for span in inspection.spans(text) {
        match span {
            Span::Literal(literal) => offset += literal.len(),
            Span::Reference { raw, resolves, .. } => {
                let end = offset + raw.len();
                decorations.push(TextDecoration::new(
                    offset..end,
                    spans::mark_style(resolves, cx),
                ));
                offset = end;
            }
        }
    }
    decorations
}

impl Shell {
    /// Marks the references of the field `input` of the request `request_id` and keeps
    /// them marked while it is edited.
    pub(super) fn mark_input(
        &mut self,
        request_id: &str,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> InputMarks {
        let decorations = self.input_decorations(request_id, input, cx);
        let collection = decorate_input(input, decorations.clone(), cx);
        let request_id = request_id.to_owned();
        let change = cx.subscribe(input, move |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.remark_input(&request_id, &input, cx);
            }
        });
        InputMarks {
            collection,
            decorations,
            _change: change,
        }
    }

    /// Marks the references of the code editor `input` of the request `request_id`, whose
    /// text is JSON where a reference may stand in place of a value, quoted or not.
    pub(super) fn mark_editor(
        &self,
        request_id: &str,
        input: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) -> EditorMarks {
        let text = input.read(cx).value().to_string();
        let decorations = self.text_decorations(request_id, &text, cx);
        let collection = input.update(cx, |state, cx| {
            state.create_decorations_collection(decorations.clone(), cx)
        });
        EditorMarks {
            collection,
            decorations,
        }
    }

    /// Sets the marks of the open raw body editor again from its text, after a change or
    /// once the requests were inspected anew.
    pub(super) fn remark_body_edit(&mut self, cx: &mut Context<Self>) {
        let Some((request_id, input)) = self
            .body_edit
            .as_ref()
            .map(|edit| (edit.request_id.clone(), edit.input.clone()))
        else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let decorations = self.text_decorations(&request_id, &text, cx);
        if let Some(edit) = self.body_edit.as_mut() {
            edit.marks.collection.set(decorations.clone(), cx);
            edit.marks.decorations = decorations;
        }
    }

    /// Sets the marks of the open field `input` again from its text, after a change or a
    /// `set_value`.
    pub(super) fn remark_input(
        &mut self,
        request_id: &str,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) {
        let decorations = self.input_decorations(request_id, input, cx);
        let marks = self
            .url_edit
            .as_mut()
            .filter(|edit| edit.input == *input)
            .and_then(|edit| edit.marks.as_mut())
            .or_else(|| {
                self.cell_edit
                    .as_mut()
                    .filter(|edit| edit.input == *input)
                    .and_then(|edit| edit.marks.as_mut())
            });
        if let Some(marks) = marks {
            marks.collection.set(decorations.clone(), cx);
            marks.decorations = decorations;
        }
    }

    /// Sets the marks of the open URL, cell and body fields again, after the requests were
    /// inspected anew: a variable added, changed or another environment may resolve
    /// differently.
    pub(super) fn remark_open_inputs(&mut self, cx: &mut Context<Self>) {
        let open = [
            self.url_edit
                .as_ref()
                .map(|edit| (edit.request_id.clone(), edit.input.clone())),
            self.cell_edit
                .as_ref()
                .map(|edit| (edit.request_id.clone(), edit.input.clone())),
        ];
        for (request_id, input) in open.into_iter().flatten() {
            self.remark_input(&request_id, &input, cx);
        }
        self.remark_body_edit(cx);
    }

    /// The decorations of the text of `input` under the inspection of the request
    /// `request_id`; none when the request is not loaded.
    fn input_decorations(
        &self,
        request_id: &str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> Vec<TextDecoration> {
        let text = input.read(cx).value().to_string();
        self.text_decorations(request_id, &text, cx)
    }

    /// The decorations of `text` under the inspection of the request `request_id`; none
    /// when the request is not loaded.
    fn text_decorations(&self, request_id: &str, text: &str, cx: &App) -> Vec<TextDecoration> {
        let Some(state) = self.requests.get(request_id) else {
            return Vec::new();
        };
        decorations(&state.inspection, text, cx)
    }
}

#[cfg(test)]
#[path = "input_marks_tests.rs"]
mod tests;
