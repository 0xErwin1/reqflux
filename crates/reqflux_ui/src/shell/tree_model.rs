//! The sidebar collection tree without GPUI: the nodes, which of them are expanded, the
//! selected one, the rows that are visible and the keyboard navigation over them.

use std::collections::HashSet;

use reqflux_core::model::{CollectionNode, Item, Workspace};

/// What a tree node stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeKind {
    /// A collection, at the top level.
    Collection,
    /// A folder inside a collection or folder.
    Folder,
    /// A request, carrying its uppercase method.
    Request {
        /// The HTTP method, as the tree shows it.
        method: String,
    },
}

/// One node of the collection tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    /// The entity id.
    pub(crate) id: String,
    /// The display name.
    pub(crate) name: String,
    /// What the node stands for.
    pub(crate) kind: NodeKind,
    /// The URL of a request as stored, which the sidebar filter matches; empty for a
    /// collection or folder.
    pub(crate) url: String,
    /// Children in store order.
    pub(crate) children: Vec<Node>,
}

impl Node {
    /// Whether the node can be expanded: collections and folders.
    pub(crate) fn is_expandable(&self) -> bool {
        !matches!(self.kind, NodeKind::Request { .. })
    }

    /// The number of direct children of a collection or folder; `None` for a request.
    pub(crate) fn child_count(&self) -> Option<usize> {
        self.is_expandable().then_some(self.children.len())
    }

    /// The number of requests under the node, at any depth, the node included.
    pub(crate) fn request_count(&self) -> usize {
        match self.kind {
            NodeKind::Request { .. } => 1,
            NodeKind::Collection | NodeKind::Folder => {
                self.children.iter().map(Node::request_count).sum()
            }
        }
    }

    /// The ids of the requests under the node, at any depth, the node included.
    pub(crate) fn request_ids(&self) -> Vec<String> {
        match self.kind {
            NodeKind::Request { .. } => vec![self.id.clone()],
            NodeKind::Collection | NodeKind::Folder => {
                self.children.iter().flat_map(Node::request_ids).collect()
            }
        }
    }

    /// Whether the node is a request whose name, method or URL contains `needle`, which is
    /// lowercase.
    fn matches(&self, needle: &str) -> bool {
        match &self.kind {
            NodeKind::Request { method } => [self.name.as_str(), method, self.url.as_str()]
                .iter()
                .any(|text| text.to_lowercase().contains(needle)),
            NodeKind::Collection | NodeKind::Folder => false,
        }
    }

    /// Whether the node matches `needle` or holds a request that does.
    fn holds_match(&self, needle: &str) -> bool {
        self.matches(needle) || self.children.iter().any(|child| child.holds_match(needle))
    }

    /// The number of requests under the node matching `needle`, the node included.
    fn match_count(&self, needle: &str) -> usize {
        match self.kind {
            NodeKind::Request { .. } => usize::from(self.matches(needle)),
            NodeKind::Collection | NodeKind::Folder => self
                .children
                .iter()
                .map(|child| child.match_count(needle))
                .sum(),
        }
    }

    /// The text the UI automation tree reads for the row: the name, prefixed by the method
    /// for a request.
    pub(crate) fn label(&self) -> String {
        match &self.kind {
            NodeKind::Request { method } => format!("{method} {}", self.name),
            NodeKind::Collection | NodeKind::Folder => self.name.clone(),
        }
    }
}

/// The tree nodes of every collection of `workspace`, in store order.
pub(crate) fn nodes(workspace: &Workspace) -> Vec<Node> {
    workspace.collections.iter().map(collection_node).collect()
}

fn collection_node(collection: &CollectionNode) -> Node {
    Node {
        id: collection.collection.id.to_string(),
        name: collection.collection.name.clone(),
        kind: NodeKind::Collection,
        url: String::new(),
        children: collection.items.iter().map(item_node).collect(),
    }
}

fn item_node(item: &Item) -> Node {
    match item {
        Item::Folder(folder) => Node {
            id: folder.folder.id.to_string(),
            name: folder.folder.name.clone(),
            kind: NodeKind::Folder,
            url: String::new(),
            children: folder.items.iter().map(item_node).collect(),
        },
        Item::Request(request) => Node {
            id: request.request.id.to_string(),
            name: request.request.name.clone(),
            kind: NodeKind::Request {
                method: request.request.method.as_str().to_owned(),
            },
            url: request.request.url.clone(),
            children: Vec::new(),
        },
    }
}

/// The node `id` among `nodes` and their descendants, with its ancestors from the root
/// down, the node last.
pub(crate) fn path_to<'a>(nodes: &'a [Node], id: &str) -> Option<Vec<&'a Node>> {
    nodes.iter().find_map(|node| {
        if node.id == id {
            return Some(vec![node]);
        }
        let mut path = path_to(&node.children, id)?;
        path.insert(0, node);
        Some(path)
    })
}

/// The node `id` among `nodes` and their descendants.
pub(crate) fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    path_to(nodes, id).and_then(|path| path.last().copied())
}

/// A row of the tree as it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleRow<'a> {
    /// The node of the row.
    pub(crate) node: &'a Node,
    /// The nesting level, 0 for a collection.
    pub(crate) depth: usize,
    /// Whether the node is expanded; always false for a request.
    pub(crate) expanded: bool,
    /// The index of the parent row among the visible rows.
    pub(crate) parent: Option<usize>,
}

/// A change a navigation key makes.
enum Step {
    Collapse(String),
    Expand(String),
    Select(Option<String>),
}

/// The collection tree with its expansion, selection and filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeState {
    roots: Vec<Node>,
    expanded: HashSet<String>,
    selected: Option<String>,
    /// The trimmed, lowercase filter text; empty shows the whole tree.
    filter: String,
}

impl TreeState {
    /// A tree over `roots` with every collection expanded, every folder collapsed and
    /// nothing selected.
    pub(crate) fn new(roots: Vec<Node>) -> Self {
        let expanded = roots.iter().map(|root| root.id.clone()).collect();
        Self {
            roots,
            expanded,
            selected: None,
            filter: String::new(),
        }
    }

    /// Replaces the tree's roots, keeping the expansion and the selection; ids that no
    /// longer exist simply never match again.
    pub(crate) fn set_roots(&mut self, roots: Vec<Node>) {
        self.roots = roots;
    }

    /// The collections, with everything under them.
    pub(crate) fn roots(&self) -> &[Node] {
        &self.roots
    }

    /// The number of requests in every collection.
    pub(crate) fn request_count(&self) -> usize {
        self.roots.iter().map(Node::request_count).sum()
    }

    /// The number of collections.
    pub(crate) fn collection_count(&self) -> usize {
        self.roots.len()
    }

    /// Sets the filter text. While it is set, the tree shows only the requests whose name,
    /// method or URL contains it, case-insensitive, under their folders and collections,
    /// expanded. Empty or blank shows the whole tree with the expansion it had.
    pub(crate) fn set_filter(&mut self, query: &str) {
        self.filter = query.trim().to_lowercase();
    }

    /// Whether a filter is set.
    pub(crate) fn is_filtered(&self) -> bool {
        !self.filter.is_empty()
    }

    /// The requests and collections the tree shows.
    pub(crate) fn shown_counts(&self) -> (usize, usize) {
        if !self.is_filtered() {
            return (self.request_count(), self.collection_count());
        }
        let requests = self
            .roots
            .iter()
            .map(|root| root.match_count(&self.filter))
            .sum();
        let collections = self
            .roots
            .iter()
            .filter(|root| root.holds_match(&self.filter))
            .count();
        (requests, collections)
    }

    /// The id of the selected node.
    pub(crate) fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Whether `id` is a request anywhere in the tree, shown or not.
    pub(crate) fn is_request(&self, id: &str) -> bool {
        fn find(nodes: &[Node], id: &str) -> bool {
            nodes.iter().any(|node| match node.kind {
                NodeKind::Request { .. } => node.id == id,
                NodeKind::Collection | NodeKind::Folder => find(&node.children, id),
            })
        }
        find(&self.roots, id)
    }

    /// Selects the node `id`.
    pub(crate) fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
    }

    /// The node `id`, shown or not.
    pub(crate) fn node(&self, id: &str) -> Option<&Node> {
        find(&self.roots, id)
    }

    /// The collection or folder holding the node `id`; `None` for a collection.
    pub(crate) fn parent_of(&self, id: &str) -> Option<&Node> {
        let path = path_to(&self.roots, id)?;
        path.len()
            .checked_sub(2)
            .and_then(|index| path.get(index).copied())
    }

    /// Selects the node `id` and expands every collection and folder above it, so its row
    /// shows.
    pub(crate) fn reveal(&mut self, id: &str) {
        let ancestors: Vec<String> = match path_to(&self.roots, id) {
            Some(path) => path
                .iter()
                .filter(|node| node.id != id)
                .map(|node| node.id.clone())
                .collect(),
            None => return,
        };
        self.expanded.extend(ancestors);
        self.selected = Some(id.to_owned());
    }

    /// The rows that are shown: every root, and the children of each expanded node, depth
    /// first in store order. While a filter is set, only the nodes that match or hold a
    /// match, every one of them expanded.
    pub(crate) fn visible_rows(&self) -> Vec<VisibleRow<'_>> {
        let mut rows = Vec::new();
        for root in &self.roots {
            self.push_rows(root, 0, None, &mut rows);
        }
        rows
    }

    fn push_rows<'a>(
        &'a self,
        node: &'a Node,
        depth: usize,
        parent: Option<usize>,
        rows: &mut Vec<VisibleRow<'a>>,
    ) {
        if self.is_filtered() && !node.holds_match(&self.filter) {
            return;
        }
        let expanded =
            node.is_expandable() && (self.is_filtered() || self.expanded.contains(&node.id));
        let index = rows.len();
        rows.push(VisibleRow {
            node,
            depth,
            expanded,
            parent,
        });
        if expanded {
            for child in &node.children {
                self.push_rows(child, depth + 1, Some(index), rows);
            }
        }
    }

    fn selected_index(rows: &[VisibleRow<'_>], selected: Option<&str>) -> Option<usize> {
        let selected = selected?;
        rows.iter().position(|row| row.node.id == selected)
    }

    /// Up: selects the previous visible row; the first row when nothing is selected.
    pub(crate) fn select_previous(&mut self) {
        let target = {
            let rows = self.visible_rows();
            let index = match Self::selected_index(&rows, self.selected()) {
                Some(index) => index.saturating_sub(1),
                None => 0,
            };
            rows.get(index).map(|row| row.node.id.clone())
        };
        if let Some(id) = target {
            self.selected = Some(id);
        }
    }

    /// Down: selects the next visible row; the first row when nothing is selected.
    pub(crate) fn select_next(&mut self) {
        let target = {
            let rows = self.visible_rows();
            let index = match Self::selected_index(&rows, self.selected()) {
                Some(index) => (index + 1).min(rows.len().saturating_sub(1)),
                None => 0,
            };
            rows.get(index).map(|row| row.node.id.clone())
        };
        if let Some(id) = target {
            self.selected = Some(id);
        }
    }

    /// Left: collapses the selected node when it is expanded, otherwise selects its parent.
    pub(crate) fn collapse_or_select_parent(&mut self) {
        let step = {
            let rows = self.visible_rows();
            let row =
                Self::selected_index(&rows, self.selected()).and_then(|index| rows.get(index));
            row.map(|row| {
                if row.expanded {
                    Step::Collapse(row.node.id.clone())
                } else {
                    let parent = row.parent.and_then(|parent| rows.get(parent));
                    Step::Select(parent.map(|parent| parent.node.id.clone()))
                }
            })
        };
        self.apply(step);
    }

    /// Right: expands the selected node when it is collapsed, otherwise selects its first
    /// child. Does nothing on a request.
    pub(crate) fn expand_or_select_child(&mut self) {
        let step = {
            let rows = self.visible_rows();
            let index = Self::selected_index(&rows, self.selected());
            let row = index.and_then(|index| rows.get(index).map(|row| (index, row)));
            row.filter(|(_, row)| row.node.is_expandable())
                .map(|(index, row)| {
                    if row.expanded {
                        let child = rows
                            .get(index + 1)
                            .filter(|next| next.parent == Some(index));
                        Step::Select(child.map(|child| child.node.id.clone()))
                    } else {
                        Step::Expand(row.node.id.clone())
                    }
                })
        };
        self.apply(step);
    }

    fn apply(&mut self, step: Option<Step>) {
        match step {
            Some(Step::Collapse(id)) => {
                self.expanded.remove(&id);
            }
            Some(Step::Expand(id)) => {
                self.expanded.insert(id);
            }
            Some(Step::Select(Some(id))) => self.selected = Some(id),
            Some(Step::Select(None)) | None => {}
        }
    }

    /// Enter: expands or collapses the selected collection or folder.
    pub(crate) fn toggle_selected(&mut self) {
        if let Some(id) = self.selected.clone() {
            self.toggle(&id);
        }
    }

    /// Expands or collapses the node `id` when it is a collection or folder.
    pub(crate) fn toggle(&mut self, id: &str) {
        let expandable = self
            .visible_rows()
            .iter()
            .any(|row| row.node.id == id && row.node.is_expandable());
        if expandable && !self.expanded.remove(id) {
            self.expanded.insert(id.to_owned());
        }
    }
}

#[cfg(test)]
#[path = "tree_model_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sidebar_filter_tests.rs"]
mod filter_tests;
