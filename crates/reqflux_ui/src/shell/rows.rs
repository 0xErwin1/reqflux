//! Sidebar tree rows: a guide line per indent level, then the chevron and the collection or
//! folder icon with the child count on the right, or the method label of a request; then
//! the name.

use bolt_legacy::tokens::{border, chrome, family, icon, space, text};
use bolt_legacy::{Chevron, ChevronDirection, Icon, IconName};
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, ParentElement,
    Role, Stateful, Styled, Window, div, prelude::*,
};

use super::text as text_role;
use super::tree_model::{NodeKind, VisibleRow};
use crate::tokens::{self, method_color};

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
    let colors = bolt_legacy::colors(cx);
    let brand = bolt_legacy::brand(cx);
    let node = row.node;

    let mut element = text_role::role(
        labeled_row(format!("tree-row-{}", node.id), node.label()),
        text::BODY,
        cx,
    )
    .aria_selected(selected)
    .group(super::sidebar_edit::ROW_GROUP)
    .relative()
    .h(chrome::TREE_ROW)
    .flex_none()
    .flex()
    .items_center()
    .gap(tokens::TREE_ROW_GAP)
    .pl(space::MD + chrome::TREE_INDENT * row.depth as f32)
    .pr(space::MD)
    .whitespace_nowrap()
    .hover(|style| style.bg(colors.hover))
    .children((0..row.depth).map(|level| {
        part(format!("tree-guide-{}-{level}", node.id))
            .absolute()
            .top_0()
            .bottom_0()
            .left(tokens::TREE_GUIDE_OFFSET + chrome::TREE_INDENT * level as f32)
            .w(border::THIN)
            .bg(colors.line)
    }));
    if node.is_expandable() {
        element = element.aria_expanded(row.expanded);
    }
    if selected {
        element = element
            .bg(bolt_legacy::accent_washes(cx).selected_item)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(border::THICK)
                    .bg(brand.accent_tint),
            );
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
            .child(div().w(icon::SM).flex_none())
            .child(
                part(format!("tree-method-{}", node.id))
                    .w(tokens::TREE_METHOD_COLUMN)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(family::MONO)
                    .text_size(tokens::METHOD_LABEL_SIZE)
                    .font_weight(FontWeight::BOLD)
                    .text_color(method_color(method, bolt_legacy::palette(cx)))
                    .child(method.clone()),
            )
            .child(name),
        NodeKind::Collection | NodeKind::Folder => {
            let (icon_name, icon_color) = match node.kind {
                NodeKind::Collection => (IconName::Box, brand.accent_tint),
                NodeKind::Folder | NodeKind::Request { .. } => (IconName::Folder, colors.muted),
            };
            element
                .child(
                    part(format!("tree-chevron-{}", node.id))
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            on_toggle(event, window, cx);
                        })
                        .flex_none()
                        .child(Chevron::new(
                            if row.expanded {
                                ChevronDirection::Down
                            } else {
                                ChevronDirection::Right
                            },
                            colors.muted,
                        )),
                )
                .child(
                    part(format!("tree-icon-{}", node.id))
                        .flex_none()
                        .child(Icon::new(icon_name).size(icon::ITEM).color(icon_color)),
                )
                .child(name)
                .child(
                    part(format!("tree-count-{}", node.id))
                        .flex_none()
                        .font_family(family::MONO)
                        .text_size(tokens::TREE_COUNT_SIZE)
                        .text_color(colors.muted)
                        .children(node.child_count().map(|count| count.to_string())),
                )
        }
    };
    element.children(slots.trailing)
}
