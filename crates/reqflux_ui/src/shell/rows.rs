//! Sidebar tree rows on `bolt`'s navigation-tree row, as DBFlux draws its tree: the selection
//! bar, a guide line per indent level, then the chevron and the collection or folder icon
//! with the child count on the right, or the method label of a request; then the name. A
//! request with saved examples has a chevron and its example count; an example is a leaf with
//! the bookmark icon, its name and its status code in the row's trailing slot.

use bolt::components::tree_nav::{TreeNavChevron, TreeNavRow};
use bolt::primitives::BadgeTone;
use bolt::tokens::TreeMetrics;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, ParentElement,
    Role, Stateful, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::request::{StatusClass, ui};
use super::sidebar_history::class_color;
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

/// The tree row of `row`, with element id `tree-row-<entity id>` and `bolt`'s parts under it
/// (`tree-row-<entity id>-chevron`, `-icon`, `-count`, `-trailing`). The name is
/// `tree-name-<entity id>`, a request's method `tree-method-<entity id>` and an example's
/// status `tree-status-<entity id>`, in the trailing slot before the row's actions. A click on
/// the chevron of a collection or folder calls `on_toggle` instead of reaching the row.
pub(super) fn tree_row(
    row: VisibleRow<'_>,
    selected: bool,
    slots: RowSlots,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let node = row.node;

    let emphasized = match &node.kind {
        NodeKind::Collection => true,
        NodeKind::Folder => row.expanded,
        NodeKind::Request { .. } | NodeKind::Example { .. } => false,
    };
    let name_color = if selected || emphasized {
        colors.strong
    } else {
        colors.foreground
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

    let mut element = TreeNavRow::new(format!("tree-row-{}", node.id), node.label(), row.depth)
        .selected(selected)
        .emphasized(emphasized)
        .name(name);
    if node.is_expandable() {
        let (icon, verb) = if row.expanded {
            (IconName::ChevronDown, "Collapse")
        } else {
            (IconName::ChevronRight, "Expand")
        };
        element = element.chevron(TreeNavChevron::new(
            row.expanded,
            ui::icon(icon),
            format!("{verb} {}", node.name),
            on_toggle,
        ));
    }
    if (node.is_expandable() || node.is_container())
        && let Some(count) = node.child_count()
    {
        element = element.count(count.to_string());
    }

    let mut trailing = slots.trailing;
    let element = match &node.kind {
        NodeKind::Example { status, .. } => {
            let tone = if selected {
                BadgeTone::Accent
            } else {
                BadgeTone::Neutral
            };
            trailing.insert(
                0,
                part(format!("tree-status-{}", node.id))
                    .flex_none()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_size(TreeMetrics::META_FONT)
                    .font_weight(FontWeight::BOLD)
                    .text_color(class_color(StatusClass::of(*status), &colors))
                    .child(status.to_string())
                    .into_any_element(),
            );
            element.icon(ui::icon(IconName::Bookmark), tone)
        }
        NodeKind::Request { method } => element.chevron_spacer(true).leading(
            part(format!("tree-method-{}", node.id))
                .w(tokens::TREE_METHOD_COLUMN)
                .flex_none()
                .overflow_hidden()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(method, cx))
                .child(method.clone()),
        ),
        NodeKind::Collection => element.icon(ui::icon(IconName::Box), BadgeTone::Accent),
        NodeKind::Folder => element.icon(ui::icon(IconName::Folder), BadgeTone::Neutral),
    };
    let element = if trailing.is_empty() {
        element
    } else {
        element.trailing(
            div()
                .flex()
                .items_center()
                .gap(TreeMetrics::GAP)
                .children(trailing),
        )
    };
    element
        .build(cx)
        .font_family(bolt::fonts::ui_family(cx))
        .group(super::sidebar_edit::ROW_GROUP)
}
