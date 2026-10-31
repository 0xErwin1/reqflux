//! The document tab row: one tab per open request with its method label, name and close
//! button. The active tab is the raised cut chip with strong text; the others are muted.
//! While the MCP approvals view is open it adds the active `MCP approvals` tab,
//! `doc-tab-approvals`, and no request tab is active; activating one leaves the view.

use bolt::chamfer::Chamfer;
use bolt::tokens::{chrome, cut, doc_tab, family, icon, text};
use bolt::{Icon, IconName};
use gpui::{Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*};

use super::approvals_view::TITLE;
use super::documents::{Location, locate};
use super::drafts::RequestDraft;
use super::rows::part;
use super::{Shell, region, text as text_role};
use crate::tokens::{self, DIRTY_DOT, method_color};

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
        .documents
        .active()
        .filter(|_| !approvals)
        .map(str::to_owned);

    region("document-tabs")
        .flex_none()
        .h(chrome::DOC_TAB_ROW)
        .flex()
        .items_center()
        .gap(doc_tab::ROW_GAP)
        .pl(doc_tab::ROW_PADDING_X)
        .pr(doc_tab::ROW_PADDING_X)
        .overflow_hidden()
        .children(tabs.into_iter().map(|(id, location)| {
            let is_active = active.as_deref() == Some(id.as_str());
            let drafted = shell.drafts.get(&id);
            let save_error = shell.save_errors.get(&id).map(String::as_str);
            tab(id, &location, is_active, drafted, save_error, cx)
        }))
        .when(approvals, |row| row.child(approvals_tab(cx)))
}

/// The active `MCP approvals` tab: the Bot icon in the tint, the title and a close button,
/// `doc-tab-approvals-close`, that goes back to the collection tree.
fn approvals_tab(cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let close = part("doc-tab-approvals-close".to_owned())
        .role(Role::Button)
        .aria_label("Close MCP approvals")
        .flex_none()
        .cursor_pointer()
        .child(
            Icon::new(IconName::CircleX)
                .size(doc_tab::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(cx.listener(|this, _event, window, cx| {
            cx.stop_propagation();
            this.leave_approvals(cx);
            this.restore_focus(window, cx);
        }));
    text_role::role(part("doc-tab-approvals".to_owned()), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(TITLE)
        .aria_selected(true)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(doc_tab::GAP)
        .h(chrome::DOC_TAB)
        .pl(doc_tab::PADDING_X)
        .pr(doc_tab::PADDING_X)
        .whitespace_nowrap()
        .child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        .child(
            Icon::new(IconName::Bot)
                .size(icon::MD)
                .color(bolt::brand(cx).accent_tint),
        )
        .child(
            div()
                .text_color(colors.strong)
                .font_weight(FontWeight::SEMIBOLD)
                .child(TITLE),
        )
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
    let name_color = if active { colors.strong } else { colors.muted };
    let weight = if active {
        FontWeight::SEMIBOLD
    } else {
        text::BODY.weight
    };

    let close_id = id.clone();
    let close = part(format!("doc-tab-close-{id}"))
        .role(Role::Button)
        .aria_label(format!("Close {}", location.name))
        .flex_none()
        .cursor_pointer()
        .child(
            Icon::new(IconName::CircleX)
                .size(doc_tab::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(cx.listener(move |this, _event, _window, cx| {
            cx.stop_propagation();
            this.close_document(&close_id, cx);
        }));

    let conflicted = draft.is_some_and(|draft| draft.conflicted);
    let drafted = draft.is_some();
    let dot_color = if conflicted {
        colors.warning
    } else {
        bolt::brand(cx).accent_tint
    };

    let activate_id = id.clone();
    let label = format!("{} {}", location.method, location.name);
    text_role::role(part(format!("doc-tab-{id}")), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(active)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(doc_tab::GAP)
        .h(chrome::DOC_TAB)
        .pl(doc_tab::PADDING_X)
        .pr(doc_tab::PADDING_X)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(active, |tab| {
            tab.child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .child(
            div()
                .flex_none()
                .font_family(family::MONO)
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&location.method, bolt::palette(cx)))
                .child(location.method.clone()),
        )
        .child(
            div()
                .text_color(name_color)
                .font_weight(weight)
                .child(location.name.clone()),
        )
        .when(conflicted, |tab| {
            tab.child(
                part(format!("doc-tab-conflict-{id}"))
                    .role(Role::Label)
                    .aria_label("File changed on disk")
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(tokens::NOTICE_TEXT)
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
                    .text_size(tokens::NOTICE_TEXT)
                    .text_color(colors.danger)
                    .child("Save failed"),
            )
        })
        .child(close)
        .when(drafted, |tab| {
            tab.child(
                part(format!("doc-tab-dirty-{id}"))
                    .absolute()
                    .top(tokens::DIRTY_DOT)
                    .right(tokens::DIRTY_DOT)
                    .size(DIRTY_DOT)
                    .rounded_full()
                    .bg(dot_color),
            )
        })
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.activate_document(&activate_id, window, cx);
        }))
}
