//! The read-only code viewers (S4, S5 of feedback-nov16): the response body and the
//! generated code of a request. A viewer is gpui-component's code editor with
//! `Editor::readonly`: the text can be focused, selected with the mouse or the keyboard
//! and copied (Ctrl+C, Ctrl+A), and nothing the user types changes it. The editor is
//! rope-based and lays out only the rows in view, so a multi-megabyte body costs its
//! parse (skipped above [`HIGHLIGHT_LIMIT`]) and not a row per line.
//!
//! The viewer keeps its own styling, the same tokens as the request editors in
//! `code_edit`: the code face, [`EditorMetrics::CODE_FONT`] on [`EditorMetrics::CODE_LINE_HEIGHT`]
//! rows, a line-number gutter and no chrome of its own: the well around it draws that.
//! Search matches and references are text decorations over the syntax colors, so a
//! selection and a mark can share a range.
//!
//! The owner seeds the text with [`CodeView::show`] only when it changes, which resets the
//! scroll and the selection, and sets the marks with [`CodeView::mark`] after it.

use bolt::tokens::EditorMetrics;
use gpui::{App, AppContext as _, Div, Entity, ParentElement, Stateful, Styled, Window};
use gpui_component::input::{Editor, EditorState, TextDecoration, TextDecorationCollection};

use super::code_edit::PLAIN;
use super::rows::part;

/// The most text a viewer highlights: a larger text is drawn plain, so one response never
/// parses megabytes on the UI thread.
pub(crate) const HIGHLIGHT_LIMIT: usize = 1024 * 1024;

/// A read-only code editor and the marks drawn over its text.
pub(crate) struct CodeView {
    editor: Entity<EditorState>,
    marks: TextDecorationCollection,
    /// The marks last set, so an unchanged set is not applied again.
    applied: Vec<TextDecoration>,
}

impl CodeView {
    /// An empty viewer.
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(PLAIN)
                .line_number(true)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
                // Ctrl F goes to the host: the response has its own search.
                .searchable(false)
        });
        let marks = editor.update(cx, |state, cx| {
            state.create_decorations_collection(Vec::new(), cx)
        });
        Self {
            editor,
            marks,
            applied: Vec::new(),
        }
    }

    /// Replaces the text with `text`, colored by the grammar `language` (plain without one,
    /// and above [`HIGHLIGHT_LIMIT`]); the marks go and the scroll and selection start over.
    pub(crate) fn show(
        &mut self,
        text: &str,
        language: Option<&'static str>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let language = match text.len() > HIGHLIGHT_LIMIT {
            true => PLAIN,
            false => language.unwrap_or(PLAIN),
        };
        self.editor.update(cx, |state, cx| {
            if state.language_name() != language {
                state.set_highlighter(language, cx);
            }
            state.set_value(text.to_owned(), window, cx);
        });
        self.applied.clear();
    }

    /// Draws `marks` over the text, in place of the last ones; the same set changes nothing.
    pub(crate) fn mark(&mut self, marks: Vec<TextDecoration>, cx: &mut App) {
        if marks == self.applied {
            return;
        }
        self.marks.set(marks.clone(), cx);
        self.applied = marks;
    }

    /// Scrolls the text to the byte `offset`, without taking the keyboard focus.
    pub(crate) fn reveal(&self, offset: usize, cx: &mut App) {
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(offset..offset, cx);
        });
    }

    /// The viewer `id` filling the room its parent leaves: the parent must be a flex column
    /// with a definite height or a flex share of one. The editor scrolls its own text.
    pub(crate) fn fill(&self, id: String, cx: &App) -> Stateful<Div> {
        part(id)
            .flex_1()
            .min_h_0()
            .w_full()
            .child(self.editor(cx).size_full())
    }

    /// The viewer `id`, `rows` lines tall.
    pub(crate) fn rows(&self, id: String, rows: usize, cx: &App) -> Stateful<Div> {
        part(id).w_full().child(
            self.editor(cx)
                .h(EditorMetrics::CODE_LINE_HEIGHT * rows as f32),
        )
    }

    /// The editor in the code face without its own chrome.
    fn editor(&self, cx: &App) -> Editor {
        Editor::new(&self.editor)
            .readonly(true)
            .appearance(false)
            .bordered(false)
            .text_size(EditorMetrics::CODE_FONT)
            .font_family(bolt::fonts::editor_family(cx))
            .text_color(bolt::colors(cx).foreground)
            .line_height(EditorMetrics::CODE_LINE_HEIGHT)
    }

    /// The text shown, for tests.
    #[cfg(test)]
    pub(crate) fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    /// The editor, for tests.
    #[cfg(test)]
    pub(crate) fn state(&self) -> &Entity<EditorState> {
        &self.editor
    }

    /// The marks last set, for tests.
    #[cfg(test)]
    pub(crate) fn marks(&self) -> &[TextDecoration] {
        &self.applied
    }
}

#[cfg(test)]
#[path = "code_view_tests.rs"]
pub(super) mod tests;
