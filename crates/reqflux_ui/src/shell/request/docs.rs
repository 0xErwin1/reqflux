//! The Docs pane (feedback-nov3 S11, design/docs/01 frame A): the request's markdown
//! description, rendered, with Edit; or the empty state whose button starts the editor. The
//! editor puts every keystroke into the request's draft; see [`crate::shell::docs`].

use bolt::tokens::Spacing;
use gpui::prelude::*;
use gpui::{AnyElement, Context, ParentElement, Styled};
use reqflux_core::model::Request;

use super::RequestState;
use crate::shell::Shell;
use crate::shell::settings_page::docs::{Docs, act, render as docs};

/// The pane of the request `id`, whose `request` is the draft's when it has one.
pub(super) fn render(
    id: &str,
    state: &RequestState,
    request: &Request,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let text = request.description_text();
    let (start, done) = (id.to_owned(), id.to_owned());
    let view = Docs {
        prefix: "request-docs",
        text: text.as_deref(),
        edit: state.docs_edit.as_ref(),
        start: act(cx, move |this, window, cx| {
            this.start_request_docs_edit(&start, window, cx);
        }),
        done: act(cx, move |this, window, cx| {
            this.close_request_docs_edit(&done, window, cx);
        }),
        done_label: "Done",
        cancel: None,
        hint: "Notes about this request, in markdown: what it does and what it expects.",
        fill: true,
    };
    // An open editor's block fills the pane, so the column around it fills it too.
    let editing = state.docs_edit.is_some();
    vec![
        gpui::div()
            .when(editing, |block| block.flex_1().min_h_0().flex().flex_col())
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .pt(Spacing::SM)
            .child(docs(view, cx))
            .into_any_element(),
    ]
}
