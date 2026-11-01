//! Sidebar tree rows, drawn as DBFlux draws its tree: `bolt`'s list row with the selection
//! bar, a guide line per indent level, then the chevron and the collection or folder icon
//! with the child count on the right, or the method label of a request; then the name.

use bolt::composites::ListRow;
use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Borders, TreeMetrics};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, ParentElement,
    Role, Stateful, Styled, Window, div, prelude::*,
};

use super::request::ui;
use super::tree_model::{NodeKind, VisibleRow};
use crate::tokens::{self, method_hue};

/// A row element the UI automation tree reports by `id`, with `label` as its text.
pub(super) fn labeled_row(id: String, label: String) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector)
        .role(Role::Label)
        .aria_label(label)
}

/// A part of a row that tests find by `id`.
pub(super) fn part(id: String) -> Stateful<Div> {
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector)
}

/// What the sidebar puts into a tree row beyond the node itself.
pub(super) struct RowSlots {
    /// Shown in place of the name: the inline rename field.
    pub(super) name: Option<AnyElement>,
    /// Shown after everything else: the row's action button and its menu.
    pub(super) trailing: Vec<AnyElement>,
    /// Applied to the name, `tree-name-<entity id>`, of a collection or folder: its drop
    /// target.
    pub(super) name_drop: Option<NameDecorator>,
}

/// Adds behavior to the name element of a row.
pub(super) type NameDecorator = Box<dyn FnOnce(Stateful<Div>) -> Stateful<Div>>;

/// The tree row of `row`, with element id `tree-row-<entity id>`. A click on the chevron
/// of a collection or folder calls `on_toggle` instead of reaching the row.
pub(super) fn tree_row(
    row: VisibleRow<'_>,
    selected: bool,
    slots: RowSlots,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let node = row.node;
    let indent = bolt::fonts::ui_px(cx, TreeMetrics::INDENT);
    let guide = bolt::fonts::ui_px(cx, TreeMetrics::GUIDE_OFFSET);

    let element = ListRow::new(ElementId::Name(format!("tree-row-{}", node.id).into()))
        .selected(selected)
        .selection_bar(true)
        .build(cx)
        .role(Role::Label)
        .aria_label(node.label())
        .aria_selected(selected)
        .group(super::sidebar_edit::ROW_GROUP);
    let mut element = ui::role(element, TextVariant::Body, cx)
        .h(TreeMetrics::ROW_HEIGHT)
        .flex_none()
        .flex()
        .items_center()
        .gap(TreeMetrics::GAP)
        .pl(TreeMetrics::PADDING_X + indent * row.depth as f32)
        .pr(TreeMetrics::PADDING_X)
        .whitespace_nowrap()
        .children((0..row.depth).map(|level| {
            part(format!("tree-guide-{}-{level}", node.id))
                .absolute()
                .top_0()
                .bottom_0()
                .left(guide + indent * level as f32)
                .w(Borders::THIN)
                .bg(colors.line)
        }));
    if node.is_expandable() {
        element = element.aria_expanded(row.expanded);
    }

    let name_color = match (&node.kind, selected) {
        (_, true) => colors.strong,
        (NodeKind::Collection, false) => colors.strong,
        (NodeKind::Folder, false) if row.expanded => colors.strong,
        (NodeKind::Folder | NodeKind::Request { .. }, false) => colors.foreground,
    };
    let name = match slots.name {
        Some(field) => field,
        None => part(format!("tree-name-{}", node.id))
            .map(|name| match slots.name_drop {
                Some(decorate) => decorate(name),
                None => name,
            })
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .text_color(name_color)
            .child(node.name.clone())
            .into_any_element(),
    };

    let element = match &node.kind {
        NodeKind::Request { method } => element
            .child(div().w(TreeMetrics::CHEVRON).flex_none())
            .child(
                part(format!("tree-method-{}", node.id))
                    .w(tokens::TREE_METHOD_COLUMN)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_size(tokens::METHOD_LABEL_SIZE)
                    .font_weight(FontWeight::BOLD)
                    .text_color(method_hue(method, cx))
                    .child(method.clone()),
            )
            .child(name),
        NodeKind::Collection | NodeKind::Folder => {
            let (icon_name, icon_color) = match node.kind {
                NodeKind::Collection => (IconName::Box, tint),
                NodeKind::Folder | NodeKind::Request { .. } => (IconName::Folder, colors.muted),
            };
            let chevron = if row.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            };
            element
                .child(
                    part(format!("tree-chevron-{}", node.id))
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            on_toggle(event, window, cx);
                        })
                        .flex_none()
                        .cursor_pointer()
                        .child(
                            Icon::new(ui::icon(chevron))
                                .size(TreeMetrics::CHEVRON)
                                .color(colors.muted),
                        ),
                )
                .child(
                    part(format!("tree-icon-{}", node.id)).flex_none().child(
                        Icon::new(ui::icon(icon_name))
                            .size(TreeMetrics::ICON)
                            .color(icon_color),
                    ),
                )
                .child(name)
                .child(
                    part(format!("tree-count-{}", node.id))
                        .flex_none()
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_size(TreeMetrics::META_FONT)
                        .text_color(colors.muted)
                        .children(node.child_count().map(|count| count.to_string())),
                )
        }
    };
    element.children(slots.trailing)
}
