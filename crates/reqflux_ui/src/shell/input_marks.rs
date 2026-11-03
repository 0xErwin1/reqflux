//! The `{{name}}` marks of a single-line field while it is being edited, drawn as the
//! read-only text draws them ([`super::request::spans`]): bolt's input decorations style the
//! reference ranges the engine's inspection finds in the field's text. The ranges follow
//! edits but are never detected, so every change of the text, and every `set_value`, sets
//! them again from the text as it is.

use gpui::{App, Context, Entity, Subscription};
use gpui_component::input::{InputEvent, InputState};
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

    /// Sets the marks of the open URL and cell fields again, after the requests were
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
    }

    /// The decorations of the text of `input` under the inspection of the request
    /// `request_id`; none when the request is not loaded.
    fn input_decorations(
        &self,
        request_id: &str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> Vec<TextDecoration> {
        let Some(state) = self.requests.get(request_id) else {
            return Vec::new();
        };
        let text = input.read(cx).value().to_string();
        decorations(&state.inspection, &text, cx)
    }
}

#[cfg(test)]
#[path = "input_marks_tests.rs"]
mod tests;
