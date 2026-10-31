//! The row menu: on a collection or folder New request, New folder, Rename, Duplicate,
//! Move to… and Delete…; on a request Open, Rename, Duplicate, Move to… and Delete; groups
//! apart by separators, with the key of each item that has one. Move to… turns the menu
//! into the list of collections and folders the row can move into.

use bolt::tokens::{border, icon, space, text};
use bolt::{Icon, IconName};
use gpui::prelude::*;
use gpui::{AnyElement, Context, InteractiveElement, ParentElement, Role, Styled, Window, div};

use crate::shell::request::option_menu;
use crate::shell::rows::{labeled_row, part};
use crate::shell::sidebar::Sidebar;
use crate::shell::tree_drag::{self, DropPlace};
use crate::shell::tree_model::{Node, NodeKind};
use crate::shell::{text as text_role, text_region};
use crate::tokens;

/// What the open menu shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuPage {
    Actions,
    /// The destinations of Move to….
    MoveTo,
}

/// An item of the row menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuItem {
    Open,
    NewRequest,
    NewFolder,
    Rename,
    Duplicate,
    MoveTo,
    Delete,
}

impl MenuItem {
    const ALL: [Self; 7] = [
        Self::Open,
        Self::NewRequest,
        Self::NewFolder,
        Self::Rename,
        Self::Duplicate,
        Self::MoveTo,
        Self::Delete,
    ];

    fn slug(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::NewRequest => "new-request",
            Self::NewFolder => "new-folder",
            Self::Rename => "rename",
            Self::Duplicate => "duplicate",
            Self::MoveTo => "move-to",
            Self::Delete => "delete",
        }
    }

    /// The label on a row of `kind`: Delete asks a confirmation that lists the contents of
    /// a collection or folder, so there it ends with an ellipsis.
    fn label(self, kind: &NodeKind) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::NewRequest => "New request",
            Self::NewFolder => "New folder",
            Self::Rename => "Rename",
            Self::Duplicate => "Duplicate",
            Self::MoveTo => "Move to…",
            Self::Delete if matches!(kind, NodeKind::Request { .. }) => "Delete",
            Self::Delete => "Delete…",
        }
    }

    /// The key that runs the item, as the menu prints it. New request is the window's
    /// Ctrl+N, which creates in the selected row's container; the rest are tree keys.
    fn key(self) -> Option<&'static str> {
        match self {
            Self::Open => Some("Enter"),
            Self::NewRequest => Some("Ctrl N"),
            Self::Rename => Some("F2"),
            Self::Duplicate => Some("Ctrl D"),
            Self::Delete => Some("Del"),
            Self::NewFolder | Self::MoveTo => None,
        }
    }

    /// The icon before the label. Delete has none: the icon set has no trash can, and the
    /// item keeps the icon column empty.
    fn icon(self) -> Option<IconName> {
        match self {
            Self::Open => Some(IconName::ExternalLink),
            Self::NewRequest => Some(IconName::File),
            Self::NewFolder => Some(IconName::FolderPlus),
            Self::Rename => Some(IconName::Pencil),
            Self::Duplicate => Some(IconName::Copy),
            Self::MoveTo => Some(IconName::ArrowRight),
            Self::Delete => None,
        }
    }

    /// The group the item sits in; a separator divides groups.
    fn group(self) -> u8 {
        match self {
            Self::Open | Self::NewRequest | Self::NewFolder => 0,
            Self::Rename | Self::Duplicate | Self::MoveTo => 1,
            Self::Delete => 2,
        }
    }

    /// Whether the item is offered on a row of `kind`: Open on requests, New request and New
    /// folder on collections and folders, Duplicate and Move to… on folders and requests.
    pub(super) fn applies_to(self, kind: &NodeKind) -> bool {
        let request = matches!(kind, NodeKind::Request { .. });
        match self {
            Self::Open => request,
            Self::NewRequest | Self::NewFolder => !request,
            Self::Duplicate | Self::MoveTo => !matches!(kind, NodeKind::Collection),
            Self::Rename | Self::Delete => true,
        }
    }
}

/// A collection or folder the row can move into, with its path from the collection.
struct Destination {
    id: String,
    path: String,
}

/// The collections and folders `moving` can move into: every one but its parent, itself and
/// what is under it.
fn destinations(roots: &[Node], moving: &str, parent: Option<&str>) -> Vec<Destination> {
    fn walk(
        nodes: &[Node],
        prefix: &str,
        moving: &str,
        parent: Option<&str>,
        out: &mut Vec<Destination>,
    ) {
        for node in nodes
            .iter()
            .filter(|node| node.is_expandable() && node.id != moving)
        {
            let path = if prefix.is_empty() {
                node.name.clone()
            } else {
                format!("{prefix} › {}", node.name)
            };
            if parent != Some(node.id.as_str()) {
                out.push(Destination {
                    id: node.id.clone(),
                    path: path.clone(),
                });
            }
            walk(&node.children, &path, moving, parent, out);
        }
    }
    let mut out = Vec::new();
    walk(roots, "", moving, parent, &mut out);
    out
}

impl Sidebar {
    /// The open menu of the row `node`, on its actions or its Move to… destinations.
    pub(super) fn menu(&self, node: &Node, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let items = match self.edit.page {
            MenuPage::Actions => self.action_items(node, cx),
            MenuPage::MoveTo => self.move_items(node, cx),
        };
        option_menu::render_at(
            ("tree-menu", "tree-menu-backdrop"),
            self.edit.menu_at,
            &self.edit.menu_focus,
            items,
            Sidebar::close_menu,
            window,
            cx,
        )
    }

    fn action_items(&self, node: &Node, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut items = Vec::new();
        let mut group = None;
        let mut separators = 0;
        for item in MenuItem::ALL
            .into_iter()
            .filter(|item| item.applies_to(&node.kind))
        {
            if group.is_some_and(|group| group != item.group()) {
                items.push(separator(separators, cx));
                separators += 1;
            }
            group = Some(item.group());
            let id = node.id.clone();
            items.push(menu_row(
                format!("tree-menu-{}", item.slug()),
                item.label(&node.kind).to_owned(),
                Some(item.icon()),
                item.key()
                    .map(|key| (format!("tree-menu-key-{}", item.slug()), key)),
                item == MenuItem::Delete,
                move |this: &mut Sidebar, window, cx| this.pick(&id, item, window, cx),
                cx,
            ));
        }
        items
    }

    fn move_items(&self, node: &Node, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let parent = self.parent(&node.id);
        let targets = destinations(self.roots(), &node.id, parent.as_deref());
        let header = text_role::role(text_region("tree-move-header"), text::LABEL, cx)
            .flex()
            .items_center()
            .h(tokens::TREE_MENU_ITEM)
            .pl(tokens::TREE_MENU_INSET)
            .pr(tokens::TREE_MENU_INSET)
            .child("Move to")
            .into_any_element();
        let mut items = vec![header];
        if targets.is_empty() {
            items.push(
                text_role::role(text_region("tree-move-empty"), text::CAPTION, cx)
                    .flex()
                    .items_center()
                    .h(tokens::TREE_MENU_ITEM)
                    .pl(tokens::TREE_MENU_INSET)
                    .pr(tokens::TREE_MENU_INSET)
                    .child("No other collection or folder")
                    .into_any_element(),
            );
        }
        for target in targets {
            let moving = node.id.clone();
            let to = target.id.clone();
            items.push(menu_row(
                format!("tree-move-{}", target.id),
                target.path,
                None,
                None,
                false,
                move |this: &mut Sidebar, window, cx| this.move_to(&moving, &to, window, cx),
                cx,
            ));
        }
        items
    }

    /// Moves `id` to the end of the collection or folder `target`, as a drop on its name
    /// does.
    fn move_to(&mut self, id: &str, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        match tree_drag::drop_command(self.roots(), id, DropPlace::Inside(target)) {
            Some(command) => self.command(command, window, cx),
            None => self.close_menu(window, cx),
        }
    }
}

/// The separator `tree-menu-separator-<index>` between two groups of the menu.
fn separator(index: usize, cx: &Context<Sidebar>) -> AnyElement {
    let colors = bolt::colors(cx);
    part(format!("tree-menu-separator-{index}"))
        .flex_none()
        .my(space::HALF)
        .h(border::THIN)
        .bg(colors.line)
        .into_any_element()
}

/// The menu row `id` labeled `label`, after its icon column when `leading` is set, with its key
/// on the right and in the danger tone when `danger`; a click runs `pick`.
fn menu_row(
    id: String,
    label: String,
    leading: Option<Option<IconName>>,
    key: Option<(String, &'static str)>,
    danger: bool,
    pick: impl Fn(&mut Sidebar, &mut Window, &mut Context<Sidebar>) + 'static,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    text_role::role(labeled_row(id, label.clone()), text::BODY, cx)
        .role(Role::MenuItem)
        .flex()
        .items_center()
        .gap(space::MD)
        .h(tokens::TREE_MENU_ITEM)
        .pl(tokens::TREE_MENU_INSET)
        .pr(tokens::TREE_MENU_INSET)
        .cursor_pointer()
        .hover(|style| style.bg(colors.hover))
        .when(danger, |row| row.text_color(colors.danger))
        .children(leading.map(|name| {
            div()
                .flex_none()
                .w(icon::MD)
                .children(name.map(|name| Icon::new(name).size(icon::MD).color(colors.muted)))
        }))
        .child(div().flex_1().min_w_0().overflow_hidden().child(label))
        .children(key.map(|(id, key)| {
            text_role::role(
                part(id).role(Role::Label).aria_label(key),
                text::KEY_HINT,
                cx,
            )
            .flex_none()
            .child(key)
        }))
        .on_click(cx.listener(move |this, _event, window, cx| pick(this, window, cx)))
        .into_any_element()
}
