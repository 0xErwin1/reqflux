//! What the response view keeps per request beyond its tab and body view (app-pending S7):
//! the body search, the JSONPath filter, the Tests and Console filters, the body's scroll
//! and the response's keyboard focus; and the shell's handlers for them, Ctrl F and Copy
//! body.
//!
//! The response takes the keyboard focus when its content is clicked; Ctrl F then opens
//! the search on the body. Enter and Shift+Enter in the search field step to the next and
//! previous match, Escape closes the search. The JSONPath filter replaces the body lines
//! with its matches while its expression is valid; search and Copy work on what shows.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, FocusHandle, KeyBinding, ScrollStrategy,
    Subscription, UniformListScrollHandle, Window, actions,
};

use bolt::controls::{InputEvent, InputState};

use super::filters::{ConsoleFilter, TestsFilter};
use super::jsonpath_model::{Filtered, JSONPATH_PLACEHOLDER};
use super::model::{BodyText, BodyView, ResponseTab};
use super::search::{self, Hit};
use super::sub_tabs::effective_view;
use crate::shell::Shell;
use crate::shell::request::{Completed, Received, SendState};
use crate::shell::send::RequestSend;
use crate::shell::toasts;

/// The key context of the response, where Ctrl F opens the search.
pub(crate) const RESPONSE_CONTEXT: &str = "Response";

/// The placeholder of the search field.
pub(crate) const SEARCH_PLACEHOLDER: &str = "Search in response";

actions!(
    response,
    [
        /// Opens the search in the response body.
        FindInResponse
    ]
);

/// Binds Ctrl F in the response.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "ctrl-f",
        FindInResponse,
        Some(RESPONSE_CONTEXT),
    )]);
}

/// The response view state of one request.
#[derive(Default)]
pub(crate) struct ViewState {
    /// The search field while the search is open.
    pub(crate) search: Option<SearchField>,
    /// The JSONPath field while the filter is open.
    pub(crate) jsonpath: Option<SearchField>,
    /// The last filter applied, so a render does not run it again.
    filtered: RefCell<Option<FilterCache>>,
    /// The current match, from 0.
    pub(crate) current: usize,
    pub(crate) tests: TestsFilter,
    pub(crate) console: ConsoleFilter,
    /// The scroll of the body lines, so stepping brings the current match into view.
    pub(crate) scroll: UniformListScrollHandle,
    /// Made the first time the response's content is clicked.
    pub(crate) focus: Option<FocusHandle>,
}

/// The search field's text and its change subscription.
pub(crate) struct SearchField {
    pub(crate) input: Entity<InputState>,
    _change: Subscription,
}

/// A filter result and what it was computed from: the body, by identity, and the
/// expression.
struct FilterCache {
    body: Arc<BodyText>,
    expression: String,
    filtered: Option<Rc<Filtered>>,
}

impl ViewState {
    /// The search text, empty while the search is closed.
    pub(crate) fn query(&self, cx: &App) -> String {
        field_text(self.search.as_ref(), cx)
    }

    /// What the JSONPath filter makes of the body of `received`; `None` while the filter
    /// is closed or its expression is blank.
    pub(crate) fn filtered(&self, received: &Received, cx: &App) -> Option<Rc<Filtered>> {
        let expression = field_text(self.jsonpath.as_ref(), cx);
        let mut cache = self.filtered.borrow_mut();
        if let Some(cached) = cache.as_ref()
            && Arc::ptr_eq(&cached.body, &received.text)
            && cached.expression == expression
        {
            return cached.filtered.clone();
        }
        let filtered = Filtered::new(&received.body, &expression).map(Rc::new);
        *cache = Some(FilterCache {
            body: Arc::clone(&received.text),
            expression,
            filtered: filtered.clone(),
        });
        filtered
    }
}

/// The text of `field`, empty while it is closed.
fn field_text(field: Option<&SearchField>, cx: &App) -> String {
    field
        .map(|field| field.input.read(cx).value().to_string())
        .unwrap_or_default()
}

/// The body lines `send` shows of `received` and whether they are colored as JSON: the
/// JSONPath matches while the filter has some, else the body as its view shows it; `None`
/// for a binary body.
pub(crate) fn shown_lines(
    send: &RequestSend,
    received: &Received,
    cx: &App,
) -> Option<(Arc<[String]>, bool)> {
    let view = effective_view(send.view, received);
    if view != BodyView::Preview
        && let Some(Filtered::Matches { lines, .. }) = send.views.filtered(received, cx).as_deref()
    {
        return Some((Arc::clone(lines), view == BodyView::Pretty));
    }
    received.text.lines(view)
}

/// The matches of the search of `send` in the body lines it shows.
pub(crate) fn hits(send: &RequestSend, cx: &App) -> Vec<Hit> {
    let query = send.views.query(cx);
    if query.is_empty() {
        return Vec::new();
    }
    match &send.state {
        SendState::Done(Completed::Response(received)) => shown_lines(send, received, cx)
            .map(|(lines, _)| search::find(&lines, &query))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// A text field with `placeholder` whose every change runs `on_change` for the request
/// `id`.
fn new_field(
    id: &str,
    placeholder: &'static str,
    on_change: fn(&mut Shell, &str, &mut Context<Shell>),
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> SearchField {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    let request = id.to_owned();
    let change = cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            on_change(this, &request, cx);
        }
    });
    SearchField {
        input,
        _change: change,
    }
}

impl Shell {
    /// A click on the response content of the request `id`: the response takes the keyboard
    /// focus, so Ctrl F reaches it.
    pub(crate) fn focus_response(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        let focus = send
            .views
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if !focus.contains_focused(window, cx) {
            focus.focus(window, cx);
        }
        // The shell's root tracks its own focus and takes any click no element under it
        // claimed; this one is the response's.
        window.prevent_default();
        cx.notify();
    }

    /// Ctrl F, or a click on the search field: shows the body and opens the search with the
    /// keyboard focus, its text selected.
    pub(crate) fn open_response_search(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        send.tab = ResponseTab::Body;
        let input = match &send.views.search {
            Some(search) => search.input.clone(),
            None => {
                let field = new_field(
                    id,
                    SEARCH_PLACEHOLDER,
                    |this, id, cx| this.step_response_search_to(id, 0, cx),
                    window,
                    cx,
                );
                let input = field.input.clone();
                send.views.search = Some(field);
                input
            }
        };
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        cx.notify();
    }

    /// Escape in the search field: closes the search, its highlights go, and the response
    /// keeps the keyboard focus.
    pub(crate) fn close_response_search(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        send.views.search = None;
        send.views.current = 0;
        match &send.views.focus {
            Some(focus) => focus.focus(window, cx),
            None => self.restore_focus(window, cx),
        }
        cx.notify();
    }

    /// A click on the JSONPath field: opens the filter with the keyboard focus, its text
    /// selected.
    pub(crate) fn open_response_jsonpath(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        let input = match &send.views.jsonpath {
            Some(field) => field.input.clone(),
            None => {
                // A new expression changes the lines, so the search starts over.
                let field = new_field(
                    id,
                    JSONPATH_PLACEHOLDER,
                    |this, id, cx| this.step_response_search_to(id, 0, cx),
                    window,
                    cx,
                );
                let input = field.input.clone();
                send.views.jsonpath = Some(field);
                input
            }
        };
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        cx.notify();
    }

    /// Escape in the JSONPath field: closes the filter, the body shows whole again, and the
    /// response keeps the keyboard focus.
    pub(crate) fn close_response_jsonpath(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        send.views.jsonpath = None;
        send.views.current = 0;
        match &send.views.focus {
            Some(focus) => focus.focus(window, cx),
            None => self.restore_focus(window, cx),
        }
        cx.notify();
    }

    /// Next (`forward`) or previous match of the search of the request `id`, wrapping.
    pub(crate) fn step_response_search(&mut self, id: &str, forward: bool, cx: &mut Context<Self>) {
        let Some(send) = self.sends.get(id) else {
            return;
        };
        let total = hits(send, cx).len();
        let next = search::step(send.views.current, total, forward);
        self.step_response_search_to(id, next, cx);
    }

    /// Makes the match `index` current and scrolls the body to it.
    fn step_response_search_to(&mut self, id: &str, index: usize, cx: &mut Context<Self>) {
        let Some(send) = self.sends.get(id) else {
            return;
        };
        let found = hits(send, cx);
        let current = index.min(found.len().saturating_sub(1));
        let line = found.get(current).map(|hit| hit.line);
        if let Some(send) = self.sends.get_mut(id) {
            send.views.current = current;
            if let Some(line) = line {
                send.views
                    .scroll
                    .scroll_to_item(line, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }

    /// Shows the tests `filter` keeps in the Tests pane of the request `id`.
    pub(crate) fn filter_response_tests(
        &mut self,
        id: &str,
        filter: TestsFilter,
        cx: &mut Context<Self>,
    ) {
        if let Some(send) = self.sends.get_mut(id) {
            send.views.tests = filter;
            cx.notify();
        }
    }

    /// Shows the entries `filter` keeps in the Console pane of the request `id`.
    pub(crate) fn filter_response_console(
        &mut self,
        id: &str,
        filter: ConsoleFilter,
        cx: &mut Context<Self>,
    ) {
        if let Some(send) = self.sends.get_mut(id) {
            send.views.console = filter;
            cx.notify();
        }
    }

    /// Copy: puts the response body of the request `id`, as received, on the clipboard and
    /// says so in a toast; while the JSONPath filter shows matches, the matches instead. A
    /// body that is not UTF-8 offers no Copy; without a text body to copy, an error toast
    /// says why.
    pub(crate) fn copy_response_body(&mut self, id: &str, cx: &mut Context<Self>) {
        let body = self.sends.get(id).and_then(|send| match &send.state {
            SendState::Done(Completed::Response(received)) => {
                let filtered = (effective_view(send.view, received) != BodyView::Preview)
                    .then(|| send.views.filtered(received, cx))
                    .flatten()
                    .and_then(|filtered| filtered.text());
                filtered.or_else(|| std::str::from_utf8(&received.body).ok().map(str::to_owned))
            }
            _ => None,
        });
        match body {
            Some(body) => {
                cx.write_to_clipboard(ClipboardItem::new_string(body));
                toasts::success("Copied", "Response body", cx);
            }
            None => toasts::failure(
                "Cannot copy the response body",
                "The request has no text response to copy.",
                cx,
            ),
        }
    }
}
