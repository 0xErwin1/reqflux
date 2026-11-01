//! The delete dialog: a modal over a scrim that names what a delete removes, lists the
//! requests it holds, flags those with an unsaved draft (which the delete discards; it warns
//! and does not block), and asks for the confirmation.

use std::ops::Range;

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::controls::Keycap;
use bolt_legacy::tokens::{border, chrome, cut, family, space, text};
use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Role, Styled, StyledText, Window, anchored, deferred, div, point,
    prelude::*,
};

use crate::shell::diamond::Diamond;
use crate::shell::rows::part;
use crate::shell::sidebar::Sidebar;
use crate::shell::tree_model::{Node, NodeKind};
use crate::shell::{region, text as text_role, text_region};
use crate::tokens::{self, method_color};

/// The noun the dialog uses for a node of `kind`.
fn noun(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Collection => "collection",
        NodeKind::Folder => "folder",
        NodeKind::Request { .. } => "request",
    }
}

fn requests(count: usize) -> String {
    match count {
        1 => "1 request".to_owned(),
        count => format!("{count} requests"),
    }
}

/// The confirmation question for deleting the node of `kind` named `name` that holds
/// `requests` requests: the dialog's accessible label.
pub(crate) fn delete_question(kind: &NodeKind, name: &str, requests: usize) -> String {
    let noun = noun(kind);
    match (kind, requests) {
        (NodeKind::Request { .. }, _) => format!("Delete request \"{name}\"?"),
        (_, 0) => format!("Delete {noun} \"{name}\"?"),
        (_, 1) => format!("Delete {noun} \"{name}\" and its 1 request?"),
        (_, count) => format!("Delete {noun} \"{name}\" and its {count} requests?"),
    }
}

/// The dialog title: `Delete folder invoices?`.
pub(crate) fn delete_title(kind: &NodeKind, name: &str) -> String {
    format!("Delete {} {name}?", noun(kind))
}

/// The dialog's explanation and the byte ranges of it drawn in bold: the name and the
/// request count.
pub(crate) fn delete_body(
    kind: &NodeKind,
    name: &str,
    count: usize,
) -> (String, Vec<Range<usize>>) {
    let mut body = String::new();
    let mut bold = Vec::new();
    let mut push_bold = |body: &mut String, piece: &str| {
        let start = body.len();
        body.push_str(piece);
        bold.push(start..body.len());
    };
    let noun = noun(kind);
    match (kind, count) {
        (NodeKind::Request { .. }, _) => {
            body.push_str("Deleting the request ");
            push_bold(&mut body, name);
            body.push_str(" removes its file from the workspace.");
        }
        (_, 0) => {
            body.push_str(&format!("The {noun} "));
            push_bold(&mut body, name);
            body.push_str(&format!(
                " holds no requests. Deleting it removes the {noun} from the workspace."
            ));
        }
        (_, count) => {
            body.push_str(&format!("The {noun} "));
            push_bold(&mut body, name);
            body.push_str(" holds ");
            push_bold(&mut body, &requests(count));
            body.push_str(&format!(
                ". Deleting it removes the {noun} and its request files from the workspace."
            ));
        }
    }
    (body, bold)
}

/// The note under the list: history keeps the sends, and the unsaved drafts are lost.
/// `None` when the delete removes no request.
pub(crate) fn delete_note(count: usize, drafts: usize) -> Option<String> {
    let history = match count {
        0 => return None,
        1 => "History keeps the sends of this request.",
        _ => "History keeps the sends of these requests.",
    };
    Some(match drafts {
        0 => history.to_owned(),
        1 => format!("{history} The unsaved draft is discarded."),
        drafts => format!("{history} The {drafts} unsaved drafts are discarded."),
    })
}

/// The label of the confirm button: `Delete folder and 4 requests`.
pub(crate) fn delete_button(kind: &NodeKind, count: usize) -> String {
    let noun = noun(kind);
    match (kind, count) {
        (NodeKind::Request { .. }, _) | (_, 0) => format!("Delete {noun}"),
        (_, count) => format!("Delete {noun} and {}", requests(count)),
    }
}

/// The requests at and under `node`, depth first.
fn requests_under(node: &Node) -> Vec<&Node> {
    fn walk<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
        if matches!(node.kind, NodeKind::Request { .. }) {
            out.push(node);
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(node, &mut out);
    out
}

impl Sidebar {
    /// The delete dialog, `tree-delete-prompt`, while a delete waits for it, over the
    /// `tree-delete-backdrop` scrim: `tree-delete-title`, `tree-delete-body`, the list of
    /// `tree-delete-item-<id>` rows with `tree-delete-draft-<id>` on those with an unsaved
    /// draft, `tree-delete-note`, and the `tree-delete-cancel` and `tree-delete-confirm`
    /// buttons.
    pub(in crate::shell) fn delete_prompt(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let node = self.node(self.edit.confirm.as_deref()?)?;
        let listed = requests_under(node);
        let count = node.request_count();
        let drafts = listed
            .iter()
            .filter(|request| self.edit.unsaved.contains(&request.id))
            .count();
        let cancel = self.cancel_button(cx);
        let confirm = self.confirm_button(delete_button(&node.kind, count), cx);
        let colors = bolt_legacy::colors(cx);
        let (body, bold) = delete_body(&node.kind, &node.name, count);
        let highlight = HighlightStyle {
            color: Some(colors.strong),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..HighlightStyle::default()
        };
        let rows: Vec<AnyElement> = listed
            .iter()
            .map(|request| self.delete_row(request, cx))
            .collect();

        let panel = region("tree-delete-prompt")
            .role(Role::Dialog)
            .aria_label(delete_question(&node.kind, &node.name, count))
            .occlude()
            .relative()
            .w(tokens::TREE_DIALOG_WIDTH)
            .flex()
            .flex_col()
            .pl(tokens::TREE_DIALOG_INSET)
            .pr(tokens::TREE_DIALOG_INSET)
            .pt(tokens::TREE_DIALOG_TOP_INSET)
            .pb(tokens::TREE_DIALOG_BOTTOM_INSET)
            .child(
                Chamfer::new(cut::MODAL)
                    .fill(colors.raised)
                    .edge(colors.line_2, border::THIN),
            )
            .child(
                text_role::role(text_region("tree-delete-title"), text::HEADING, cx)
                    .mb(space::SM)
                    .child(delete_title(&node.kind, &node.name)),
            )
            .child(
                text_role::role(text_region("tree-delete-body"), text::BODY, cx)
                    .aria_label(body.clone())
                    .child(
                        StyledText::new(body)
                            .with_highlights(bold.into_iter().map(|range| (range, highlight))),
                    ),
            )
            .when(!rows.is_empty(), |panel| {
                panel.child(
                    region("tree-delete-list")
                        .relative()
                        .mt(space::MD)
                        .py(space::SM)
                        .flex()
                        .flex_col()
                        .child(
                            Chamfer::new(cut::CONTROL)
                                .fill(colors.background)
                                .edge(colors.line, border::THIN),
                        )
                        .children(rows),
                )
            })
            .children(delete_note(count, drafts).map(|note| {
                text_role::role(text_region("tree-delete-note"), text::CAPTION, cx)
                    .aria_label(note.clone())
                    .mt(space::SM)
                    .child(note)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(space::SM)
                    .mt(tokens::TREE_DIALOG_ACTIONS_GAP)
                    .child(cancel)
                    .child(confirm),
            );

        let viewport = window.viewport_size();
        let left = ((viewport.width - tokens::TREE_DIALOG_WIDTH) / 2.).max(Pixels::ZERO);
        let backdrop = region("tree-delete-backdrop")
            .occlude()
            .w(viewport.width)
            .h(viewport.height)
            .bg(colors.scrim)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| this.cancel_delete(window, cx)),
            );
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(
                    deferred(anchored().position(Point::default()).child(backdrop))
                        .with_priority(3),
                )
                .child(
                    deferred(
                        anchored()
                            .position(point(left, tokens::TREE_DIALOG_TOP))
                            .child(panel),
                    )
                    .with_priority(4),
                )
                .into_any_element(),
        )
    }

    /// The row `tree-delete-item-<id>` of a request the delete removes.
    fn delete_row(&self, request: &Node, cx: &Context<Self>) -> AnyElement {
        let method = match &request.kind {
            NodeKind::Request { method } => method.clone(),
            NodeKind::Collection | NodeKind::Folder => String::new(),
        };
        let unsaved = self.edit.unsaved.contains(&request.id);
        text_role::role(
            part(format!("tree-delete-item-{}", request.id)),
            text::BODY,
            cx,
        )
        .role(Role::Label)
        .aria_label(request.label())
        .flex()
        .items_center()
        .gap(space::SM)
        .h(tokens::TREE_DIALOG_ROW)
        .pl(space::LG)
        .pr(space::MD)
        .whitespace_nowrap()
        .child(
            div()
                .w(tokens::TREE_METHOD_COLUMN)
                .flex_none()
                .overflow_hidden()
                .font_family(family::MONO)
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&method, bolt_legacy::palette(cx)))
                .child(method),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(request.name.clone()),
        )
        .when(unsaved, |row| {
            row.child(
                text_role::role(
                    part(format!("tree-delete-draft-{}", request.id)),
                    text::CAPTION,
                    cx,
                )
                .role(Role::Label)
                .aria_label("unsaved draft")
                .flex_none()
                .flex()
                .items_center()
                .gap(space::HALF)
                .child(Diamond::new(bolt_legacy::brand(cx).accent_tint))
                .child("unsaved draft"),
            )
        })
        .into_any_element()
    }

    /// `tree-delete-cancel`: a ghost button with its Esc keycap.
    fn cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = bolt_legacy::colors(cx);
        text_role::role(region("tree-delete-cancel"), text::BODY, cx)
            .role(Role::Button)
            .aria_label("Cancel")
            .relative()
            .h(chrome::CONTROL)
            .flex()
            .items_center()
            .gap(space::SM)
            .pl(space::MD)
            .pr(space::MD)
            .cursor_pointer()
            .text_color(colors.foreground)
            .child(Chamfer::new(cut::CONTROL).edge(colors.line_2, border::THIN))
            .child("Cancel")
            .child(Keycap::new("tree-delete-cancel-key", "Esc"))
            .on_click(cx.listener(|this, _event, window, cx| this.cancel_delete(window, cx)))
            .into_any_element()
    }

    /// `tree-delete-confirm`: the danger button that runs the delete.
    fn confirm_button(&self, label: String, cx: &mut Context<Self>) -> AnyElement {
        let colors = bolt_legacy::colors(cx);
        text_role::role(region("tree-delete-confirm"), text::BODY, cx)
            .role(Role::Button)
            .aria_label(label.clone())
            .relative()
            .h(chrome::CONTROL)
            .flex()
            .items_center()
            .pl(space::MD)
            .pr(space::MD)
            .cursor_pointer()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.on_semantic)
            .child(Chamfer::new(cut::CONTROL).fill(colors.danger))
            .child(label)
            .on_click(cx.listener(|this, _event, window, cx| this.confirm_delete(window, cx)))
            .into_any_element()
    }
}
