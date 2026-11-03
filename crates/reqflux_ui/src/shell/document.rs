//! The document island: the tab row and the path bar over the active document, or the reason
//! there is none.
//!
//! The active request shows the action strip at its left and the request beside it, with
//! its response under it across the draggable split (see `split`). While the layout is
//! Document (see `layout`), the island draws the active request's folder as a read-only
//! `.http` document instead (`document_view`), and Ctrl+Enter sends the section the cursor
//! is in, its response under that section rather than in a split. While the history tab is active, the island shows its read-only
//! document, `history-readonly`, instead (see `history::render_tab`).

use bolt::composites::Island;
use bolt::tokens::Borders;
use gpui::{Context, InteractiveElement as _, IntoElement, ParentElement, Styled, Window};
use reqflux_engine::config::LayoutMode;

use super::documents::locate;
use super::{
    Shell, action_strip, commands, empty_state, history, layout, path_bar, region, request, split,
    tabs,
};

#[path = "document_view.rs"]
pub(crate) mod document_view;

pub(super) fn render(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let colors = bolt::colors(cx);
    let active = shell
        .active_request()
        .and_then(|id| locate(&shell.roots, id));
    let history_tab = shell.orphan.as_ref().filter(|tab| tab.active);
    let active_id = shell.documents.active();
    let drafted = active_id.is_some_and(|id| shell.drafts.contains_key(id));

    let mut body = region("document-body")
        .flex_1()
        .min_h_0()
        .flex()
        .border_t(Borders::THIN)
        .border_color(colors.line);
    body = match (shell.workspace.document_message(), &active) {
        (Some(message), _) => body
            .items_center()
            .justify_center()
            .child(empty_state::render(shell, message, cx)),
        (None, _) if shell.run_tab_active() => body.child(super::runner::render(shell, window, cx)),
        (None, _) if shell.example_tab_active() => {
            body.child(super::examples::render(shell, window, cx))
        }
        (None, _) if history_tab.is_some() => match history_tab {
            Some(tab) => body.child(history::render_tab(shell, tab, cx)),
            None => body,
        },
        (None, _) if shell.split.mode == LayoutMode::Document => body
            .child(layout::with_strip_switcher(
                action_strip::render(active_id, drafted, cx),
                shell,
                cx,
            ))
            .child(
                region("document-content")
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    // Ctrl+Enter sends the section the cursor is in while the focus is in
                    // the document, before the send-request command reaches the shell.
                    .on_action(cx.listener(|this, _: &commands::SendRequest, window, cx| {
                        this.send_document_section(window, cx);
                    }))
                    .child(document_view::render(shell, window, cx)),
            ),
        (None, Some(_)) => {
            let content = region("document-content")
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col();
            let state = active_id.and_then(|id| shell.requests.get(id).map(|state| (id, state)));
            body.child(layout::with_strip_switcher(
                action_strip::render(active_id, drafted, cx),
                shell,
                cx,
            ))
            .child(match state {
                Some((id, state)) => {
                    let send = shell.sends.get(id);
                    let sending = send.is_some_and(|send| send.state.is_sending());
                    let request = request::render(id, state, sending, shell, window, cx);
                    content.child(split::render(id, request, send, shell, cx))
                }
                None => content,
            })
        }
        (None, None) => body.child(region("document-content").flex_1()),
    };

    region("document").flex_1().min_w_0().h_full().child(
        Island::new()
            .size_full()
            .child(tabs::render(shell, cx))
            .child(path_bar::render(active.as_ref(), cx))
            .child(body),
    )
}
