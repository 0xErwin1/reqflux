//! The code editors of the request panes (design/syntax/01): gpui-component's code editor
//! highlighting with one of `bolt`'s bundled grammars, or none for plain text. Text types
//! as it is pressed: no closing bracket or quote is added and Enter keeps no indent, so a
//! keystroke lands in the draft as typed, as the plain editor did.

use bolt::tokens::EditorMetrics;
use gpui::{App, AppContext as _, Context, Entity, Styled, Window};
use gpui_component::input::{Editor, EditorState};
use reqflux_core::model::RawLanguage;

/// The language name of plain text: no grammar is registered under it.
pub(crate) const PLAIN: &str = "text";

/// The grammar that highlights a raw body in `language`.
pub(crate) fn raw_grammar(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Json => "json",
        RawLanguage::Xml => "xml",
        RawLanguage::Html => "html",
        RawLanguage::Javascript => "javascript",
        RawLanguage::Text => PLAIN,
    }
}

/// A focused code editor in `language`, seeded with `text`.
pub(crate) fn open<T: 'static>(
    language: &'static str,
    text: String,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<EditorState> {
    let input = cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .line_number(true)
            .auto_close(false)
            .smart_indent(false)
    });
    input.update(cx, |state, cx| {
        state.set_value(text, window, cx);
        state.focus(window, cx);
    });
    input
}

/// Switches the open editor `input` to `language`. Changing the highlighter clears its
/// cache until the next render, so the same language changes nothing.
pub(crate) fn set_language(input: &Entity<EditorState>, language: &'static str, cx: &mut App) {
    input.update(cx, |state, cx| {
        if state.language_name() != language {
            state.set_highlighter(language, cx);
        }
    });
}

/// The editor of `input`, `rows` lines tall, in the code face without its own chrome: the
/// well around it draws that.
pub(crate) fn element(input: &Entity<EditorState>, rows: usize, cx: &App) -> Editor {
    Editor::new(input)
        .appearance(false)
        .bordered(false)
        .h(EditorMetrics::CODE_LINE_HEIGHT * rows as f32)
        .text_size(EditorMetrics::CODE_FONT)
        .font_family(bolt::fonts::editor_family(cx))
        .text_color(bolt::colors(cx).foreground)
}
