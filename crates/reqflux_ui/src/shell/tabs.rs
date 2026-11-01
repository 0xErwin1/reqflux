//! The document tab row: one tab per open request with its method label, name and close
//! button, drawn with `bolt`'s document tab bar and tabs. The active tab is the raised cut
//! chip with strong text; the others are muted. While the MCP approvals view is open it adds
//! the active `MCP approvals` tab, `doc-tab-approvals`, and no request tab is active;
//! activating one leaves the view. An open history entry has the history tab,
//! `doc-tab-history`, first in the row; while it is active no request tab is.

use bolt::composites::{document_tab, document_tab_bar, document_tab_title};
use bolt::primitives::Icon;
use bolt::tokens::{FontSizes, TabMetrics};
use bolt_legacy::IconName;
use gpui::{
    ClickEvent, Context, Div, ElementId, FontWeight, InteractiveElement, ParentElement, Role,
    Stateful, Styled, Window, div, prelude::*,
};

use super::approvals_view::TITLE;
use super::documents::{Location, locate};
use super::drafts::RequestDraft;
use super::request::ui;
use super::rows::part;
use super::{Shell, history};
use crate::tokens::{self, DIRTY_DOT, method_hue};

/// The tab row, element id `document-tabs`.
pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> Stateful<Div> {
    let tabs: Vec<_> = shell
        .documents
        .open_ids()
        .iter()
        .filter_map(|id| locate(&shell.roots, id).map(|location| (id.clone(), location)))
        .collect();
    let approvals = shell.approvals_open(cx);
    let active = shell
        .active_request()
        .filter(|_| !approvals)
        .map(str::to_owned);
    let history_tab = shell.orphan.as_ref().map(|tab| history::tab_chip(tab, cx));

    let bar = document_tab_bar(cx)
        .id("document-tabs")
        .debug_selector(|| "document-tabs".to_owned())
        .overflow_hidden()
        .children(history_tab)
        .children(tabs.into_iter().map(|(id, location)| {
            let is_active = active.as_deref() == Some(id.as_str());
            let drafted = shell.drafts.get(&id);
            let save_error = shell.save_errors.get(&id).map(String::as_str);
            tab(id, &location, is_active, drafted, save_error, cx)
        }));
    bar.when(approvals, |row| row.child(approvals_tab(cx)))
}

/// The close button `id` of a tab, named `label`: DBFlux's tab close icon; a click runs
/// `close` and does not reach the tab.
fn close_button(
    id: String,
    label: String,
    close: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    cx: &Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id)
        .role(Role::Button)
        .aria_label(label)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(
            Icon::new(ui::icon(IconName::CircleX))
                .size(TabMetrics::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            close(event, window, cx);
        })
}

/// The active `MCP approvals` tab: the Bot icon in the tint, the title and a close button,
/// `doc-tab-approvals-close`, that goes back to the collection tree.
fn approvals_tab(cx: &mut Context<Shell>) -> Stateful<Div> {
    let close = close_button(
        "doc-tab-approvals-close".to_owned(),
        "Close MCP approvals".to_owned(),
        cx.listener(|this, _event, window, cx| {
            this.leave_approvals(cx);
            this.restore_focus(window, cx);
        }),
        cx,
    );
    document_tab("doc-tab-approvals", true, cx)
        .role(Role::Tab)
        .aria_label(TITLE)
        .aria_selected(true)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            Icon::new(ui::icon(IconName::Bot))
                .size(TabMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(document_tab_title(TITLE, true, cx))
        .child(close)
}

/// The tab of the request `id`: element id `doc-tab-<id>`, label `<METHOD> <name>`, selected
/// when active; its close button is `doc-tab-close-<id>`. A draft marks the tab unsaved with
/// a dot at its top-right, `doc-tab-dirty-<id>`: the brand tint, or the warning color when
/// the saved file moved underneath the draft, with the label `doc-tab-conflict-<id>`. A save
/// that failed for another reason shows the danger label `doc-tab-save-error-<id>`.
fn tab(
    id: String,
    location: &Location,
    active: bool,
    draft: Option<&RequestDraft>,
    save_error: Option<&str>,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let close_id = id.clone();
    let close = close_button(
        format!("doc-tab-close-{id}"),
        format!("Close {}", location.name),
        cx.listener(move |this, _event, _window, cx| this.close_document(&close_id, cx)),
        cx,
    );

    let conflicted = draft.is_some_and(|draft| draft.conflicted);
    let drafted = draft.is_some();
    let dot_color = if conflicted {
        colors.warning
    } else {
        bolt::brand::brand(cx).accent_tint
    };
    let title = document_tab_title(location.name.clone(), active, cx);
    let title = if active {
        title
    } else {
        title.font_weight(FontWeight::NORMAL)
    };

    let activate_id = id.clone();
    let label = format!("{} {}", location.method, location.name);
    document_tab(ElementId::Name(format!("doc-tab-{id}").into()), active, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(active)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            div()
                .flex_none()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(&location.method, cx))
                .child(location.method.clone()),
        )
        .child(title)
        .when(conflicted, |tab| {
            tab.child(
                part(format!("doc-tab-conflict-{id}"))
                    .role(Role::Label)
                    .aria_label("File changed on disk")
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(FontSizes::XS)
                    .text_color(colors.danger)
                    .child("File changed on disk"),
            )
        })
        .when_some(save_error, |tab, error| {
            tab.child(
                part(format!("doc-tab-save-error-{id}"))
                    .role(Role::Label)
                    .aria_label(format!("Save failed: {error}"))
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(FontSizes::XS)
                    .text_color(colors.danger)
                    .child("Save failed"),
            )
        })
        .child(close)
        .when(drafted, |tab| {
            tab.child(
                part(format!("doc-tab-dirty-{id}"))
                    .absolute()
                    .top(DIRTY_DOT)
                    .right(DIRTY_DOT)
                    .size(DIRTY_DOT)
                    .rounded_full()
                    .bg(dot_color),
            )
        })
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.activate_document(&activate_id, window, cx);
        }))
}
