//! The Document layout's view (v1-features S7, design/layouts/02): the active request's
//! folder as one read-only `.http` document, the sections `reqflux_core::http_document`
//! writes from the saved requests (the draft's text when a request has one). The view
//! edits nothing, syncs nothing and sends nothing: editing and running in this mode are
//! later S7 units. The lines draw like the response body's, one `document-section-<n>`
//! per request and a `document-line-<n>` per line; a section's method line takes the
//! method colour the request line uses and its `###` line is muted. Without an open
//! request the view shows an empty state.
// bolt gap: `bolt` has no read-only code view with line numbers; the lines are drawn as
// the Body pane draws its code.

use bolt::composites::EmptyState;
use bolt::tokens::{EditorMetrics, GridMetrics, Spacing};
use gpui::{AnyElement, App, Context, Div, Stateful, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_core::http_document::{self, DocumentRequest};
use reqflux_core::model::{Body, Request};

use crate::shell::{
    Shell, history, region,
    request::ui,
    rows::{labeled_row, part},
    tree_model::{Node, NodeKind},
};
use crate::tokens::method_hue;
use crate::workspace::OpenedWorkspace;

/// The empty state while no request is open.
const NO_REQUEST: &str = "No request is open";
/// The empty state while the active request's folder shows no request.
const NO_FOLDER: &str = "This folder has no requests to show";

pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> Stateful<Div> {
    let view = region("document-view")
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT);
    let Some(sections) = sections(shell) else {
        return view.child(centered(NO_REQUEST));
    };
    if sections.is_empty() {
        return view.child(centered(NO_FOLDER));
    }
    let text = http_document::write(&sections);
    view.children(section_groups(&sections, &text, cx))
}

/// The empty state of the view: `message` centered over the island.
fn centered(message: &str) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .child(EmptyState::new(ui::icon(IconName::File), message))
}

/// The `.http` sections of the active request's folder: one per request, in tree order,
/// the draft's text when the request has one. `None` when no request is open; empty when
/// the workspace shows none of the folder's requests.
fn sections(shell: &Shell) -> Option<Vec<DocumentRequest>> {
    let active = shell.documents.active()?;
    let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
        return Some(Vec::new());
    };
    let workspace = engine.workspace();
    let mut shown = Vec::new();
    for id in folder_requests(&shell.roots, active)? {
        let Some(saved) = history::find_request(workspace, &id).map(|(_, request)| request) else {
            continue;
        };
        shown.push(document_section(shell.displayed_request(&id, saved)));
    }
    Some(shown)
}

/// The ids of the requests the container that directly holds `id` shows, depth first in
/// tree order; `None` when `nodes` holds no request with that id. A request's folder is
/// the collection or folder it sits in directly; the document lists every request under
/// that container, its subfolders included.
fn folder_requests(nodes: &[Node], id: &str) -> Option<Vec<String>> {
    nodes.iter().find_map(|node| match &node.kind {
        NodeKind::Collection | NodeKind::Folder => {
            if node.children.iter().any(|child| child.id == id) {
                Some(node.request_ids())
            } else {
                folder_requests(&node.children, id)
            }
        }
        NodeKind::Request { .. } | NodeKind::Example { .. } => None,
    })
}

/// The section `write` emits for `request`: the name, the method, the URL with the
/// enabled params appended after the separator `?` or `&` the URL asks for, the enabled
/// headers, and the raw body; the other body kinds have no `.http` text yet.
fn document_section(request: &Request) -> DocumentRequest {
    DocumentRequest {
        name: request.name.clone(),
        method: request.method.clone(),
        url: full_url(request),
        headers: request
            .headers
            .iter()
            .filter(|header| header.enabled)
            .cloned()
            .collect(),
        body: match &request.body {
            Body::Raw { text, .. } => text.clone(),
            Body::None
            | Body::Urlencoded { .. }
            | Body::FormData { .. }
            | Body::Binary { .. }
            | Body::Graphql { .. } => String::new(),
        },
    }
}

/// The URL of `request` as written, its enabled params appended after `?`, or `&` when
/// the URL already carries a query.
fn full_url(request: &Request) -> String {
    let params: Vec<String> = request
        .params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| format!("{}={}", param.key, param.value))
        .collect();
    if params.is_empty() {
        return request.url.clone();
    }
    let separator = if request.url.contains('?') { "&" } else { "?" };
    format!("{}{separator}{}", request.url, params.join("&"))
}

/// One kind of document line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    /// A `### <name>` separator line.
    Section,
    /// The `<METHOD> <url>` line that opens a section.
    Method,
    /// A header, a blank line or a body line.
    Text,
}

/// The sections of `text`, `write`'s output for `sections`, as one
/// `document-section-<n>` each, its lines numbered across the whole document.
fn section_groups(sections: &[DocumentRequest], text: &str, cx: &App) -> Vec<AnyElement> {
    let mut groups: Vec<Vec<(String, LineKind)>> = Vec::new();
    let mut after_section = false;
    for line in text.lines() {
        if line.starts_with("###") {
            groups.push(vec![(line.to_owned(), LineKind::Section)]);
            after_section = true;
        } else {
            let kind = if after_section && !line.is_empty() {
                LineKind::Method
            } else {
                LineKind::Text
            };
            if let Some(group) = groups.last_mut() {
                group.push((line.to_owned(), kind));
            }
            after_section = false;
        }
    }
    let mut rows = Vec::new();
    let mut number = 0usize;
    for (index, (section, group)) in sections.iter().zip(groups).enumerate() {
        let method = section.method.as_str().to_owned();
        let section_view = part(format!("document-section-{}", index + 1))
            .flex_none()
            .flex()
            .flex_col()
            .children(group.into_iter().map(|(text, kind)| {
                number += 1;
                line_row(number, &text, kind, &method, cx).into_any_element()
            }));
        rows.push(section_view.into_any_element());
    }
    rows
}

/// One numbered line of the document, as the response body draws its lines: the number in
/// the gutter and the text beside it, a section line muted, a method line in the method's
/// hue and everything else in the foreground.
fn line_row(number: usize, text: &str, kind: LineKind, method: &str, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let color = match kind {
        LineKind::Section => colors.muted,
        LineKind::Method => method_hue(method, cx),
        LineKind::Text => colors.foreground,
    };
    labeled_row(format!("document-line-{number}"), text.to_owned())
        .flex()
        .items_center()
        .h(EditorMetrics::CODE_LINE_HEIGHT)
        .child(
            div()
                .w(GridMetrics::ROW_NUMBER_WIDTH)
                .flex_none()
                .pr(Spacing::MD)
                .text_right()
                .text_color(colors.line_2)
                .child(number.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .flex()
                .whitespace_nowrap()
                .text_color(color)
                .child(text.to_owned()),
        )
}

#[cfg(test)]
#[path = "document_view_tests.rs"]
mod tests;
