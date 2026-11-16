//! The read-only code viewers (S4, S5 of feedback-nov16), through the response body and the
//! generated code of a request: the text can be selected with the mouse and the keyboard
//! and copied by its selection, and nothing the user types changes it.

use std::ops::Range;

use gpui::{
    ClipboardItem, Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext,
};

use super::CodeView;
use crate::shell::Shell;
use crate::shell::preview_tests::serve_reply;
use crate::shell::response_tests::sent_shell;
use crate::shell::send_tests::{JSON, PING, click, render, settle};

/// The pretty body of `GET Ping`.
pub(crate) const PRETTY: &str = "{\n  \"id\": 7,\n  \"tags\": [\n    \"a\"\n  ]\n}";

/// The body viewer of the request `PING`, read in `read`; it must exist.
fn with_body<R>(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    read: impl FnOnce(&CodeView, &gpui::App) -> R,
) -> R {
    let result = cx.update(|_, cx| {
        let shell = shell.read(cx);
        let code = shell
            .sends
            .get(PING)
            .and_then(|send| send.views.code.as_ref())?;
        Some(read(code.view(), cx))
    });
    match result {
        Some(result) => result,
        None => panic!("the response has no body editor"),
    }
}

/// The text the read-only body editor of the request `PING` shows.
pub(crate) fn shown_body(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    with_body(cx, shell, |view, cx| view.text(cx))
}

/// The search matches drawn over the body: each one's text and whether it is the current
/// one, in the warning fill.
pub(crate) fn body_marks(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<(String, bool)> {
    let text = shown_body(cx, shell);
    with_body(cx, shell, |view, cx| {
        let warning = bolt::colors(cx).warning;
        view.marks()
            .iter()
            .map(|mark| {
                (
                    text.get(mark.range.clone()).unwrap_or_default().to_owned(),
                    mark.style.background_color == Some(warning),
                )
            })
            .collect()
    })
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

/// Selects `range` of the body text, as a drag would.
fn select(cx: &mut VisualTestContext, shell: &Entity<Shell>, range: Range<usize>) {
    let state = with_body(cx, shell, |view, _| view.state().clone());
    cx.update(|_, cx| state.update(cx, |state, cx| state.set_selected_range(range, cx)));
    cx.run_until_parked();
}

/// The range of `needle` in the body text.
fn range_of(cx: &mut VisualTestContext, shell: &Entity<Shell>, needle: &str) -> Range<usize> {
    let text = shown_body(cx, shell);
    match text.find(needle) {
        Some(start) => start..start + needle.len(),
        None => panic!("{needle} is not in {text}"),
    }
}

#[gpui::test]
fn typing_into_the_response_body_changes_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    click(cx, "response-body-text");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("pasted".to_owned())));

    cx.simulate_input("zz");
    cx.run_until_parked();
    for keys in [
        "backspace",
        "delete",
        "enter",
        "tab",
        "ctrl-v",
        "ctrl-x",
        "ctrl-z",
    ] {
        press(cx, keys);
    }
    press(cx, "ctrl-a");
    cx.simulate_input("replaced");
    cx.run_until_parked();
    press(cx, "backspace");

    assert_eq!(shown_body(cx, &shell), PRETTY);
}

#[gpui::test]
fn select_all_and_copy_put_the_whole_body_on_the_clipboard(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    click(cx, "response-body-text");
    press(cx, "ctrl-a");
    press(cx, "ctrl-c");
    assert_eq!(clipboard(cx).as_deref(), Some(PRETTY));
    assert_eq!(shown_body(cx, &shell), PRETTY);
}

#[gpui::test]
fn copy_puts_exactly_the_selection_on_the_clipboard(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    click(cx, "response-body-text");

    let tags = range_of(cx, &shell, "\"tags\"");
    select(cx, &shell, tags);
    press(cx, "ctrl-c");
    assert_eq!(clipboard(cx).as_deref(), Some("\"tags\""));

    // Keyboard selection: the first line.
    press(cx, "ctrl-home");
    press(cx, "shift-end");
    press(cx, "ctrl-c");
    assert_eq!(clipboard(cx).as_deref(), Some("{"));
    assert_eq!(shown_body(cx, &shell), PRETTY);
}

#[gpui::test]
fn dragging_the_mouse_selects_text_to_copy(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    let tags = range_of(cx, &shell, "\"tags\"");
    let state = with_body(cx, &shell, |view, _| view.state().clone());
    let bounds = cx.update(|_, cx| state.read(cx).range_to_bounds(&tags));
    let Some(bounds) = bounds else {
        panic!("the tags line is not laid out");
    };
    let y: Pixels = bounds.center().y;
    let from = gpui::point(bounds.left() + gpui::px(1.), y);
    let to = gpui::point(bounds.right() - gpui::px(1.), y);

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    press(cx, "ctrl-c");

    assert_eq!(clipboard(cx).as_deref(), Some("\"tags\""));
}

#[gpui::test]
fn the_copy_button_still_copies_the_whole_body_with_a_selection(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    click(cx, "response-body-text");
    let tags = range_of(cx, &shell, "\"tags\"");
    select(cx, &shell, tags);

    click(cx, "response-copy");

    assert_eq!(clipboard(cx).as_deref(), Some(JSON));
}

#[gpui::test]
fn the_syntax_follows_the_body_and_a_huge_body_is_plain(cx: &mut TestAppContext) {
    let (shell, cx) = sent_shell(cx);
    let language = with_body(cx, &shell, |view, cx| {
        view.state().read(cx).language_name().to_string()
    });
    assert_eq!(language, "json");

    // Above the highlight limit the viewer takes no grammar; the text is whole.
    let big = "x".repeat(super::HIGHLIGHT_LIMIT + 1);
    let editor = cx.update(|window, cx| {
        let mut view = CodeView::new(window, cx);
        view.show(&big, Some("json"), window, cx);
        view
    });
    let (language, len) = cx.update(|_, cx| {
        (
            editor.state().read(cx).language_name().to_string(),
            editor.text(cx).len(),
        )
    });
    assert_eq!(language, "text");
    assert_eq!(len, big.len());
}

#[gpui::test]
fn a_multi_megabyte_body_shows_whole_and_searches(cx: &mut TestAppContext) {
    let mut body = String::new();
    for n in 0..60_000 {
        body.push_str(&format!(
            "line {n} of a very long plain text response body\n"
        ));
    }
    body.push_str("the needle\n");
    assert!(body.len() > 2 * 1024 * 1024);
    let port = serve_reply(&[("content-type", "text/plain")], body.clone().into_bytes());
    let (shell, cx) = render(cx, port);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    assert_eq!(shown_body(cx, &shell).len(), body.trim_end().len());
    click(cx, "response-search");
    cx.simulate_input("needle");
    cx.run_until_parked();
    assert_eq!(body_marks(cx, &shell), [("needle".to_owned(), true)]);
    let scrolled = with_body(cx, &shell, |view, cx| {
        view.state().read(cx).scroll_offset().y
    });
    assert!(scrolled < Pixels::ZERO, "the body scrolled to the match");
}
