//! Reordering and moving tree items (tree-crud S10): Alt+Up and Alt+Down step the selected
//! row among its siblings, and a row dragged onto another lands before it, or inside a
//! collection or folder when dropped on its name. Both map to a [`TreeCommand`] the shell
//! runs; the index is the target's place among the parent's children as shown, without the
//! moved item, which is what the engine expects.

use bolt::primitives::TextVariant;
use bolt::tokens::TreeMetrics;
use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div};

use super::request::ui;
use super::sidebar_edit::TreeCommand;
use super::tree_model::{Node, NodeKind, path_to};

/// The payload of a dragged tree row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraggedRow {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// Where a dragged row is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropPlace<'a> {
    /// Before the row with this id, in that row's parent.
    Before(&'a str),
    /// At the end of the collection or folder with this id.
    Inside(&'a str),
}

/// Which way a keyboard step moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Up,
    Down,
}

/// The node `id` with its parent; `None` for a collection or a missing id.
fn with_parent<'a>(roots: &'a [Node], id: &str) -> Option<(&'a Node, &'a Node)> {
    let path = path_to(roots, id)?;
    let parent = path
        .len()
        .checked_sub(2)
        .and_then(|index| path.get(index))?;
    Some((path.last().copied()?, parent))
}

/// The ids of `parent`'s children, without `moved`.
fn siblings_without<'a>(parent: &'a Node, moved: &str) -> Vec<&'a str> {
    parent
        .children
        .iter()
        .map(|child| child.id.as_str())
        .filter(|id| *id != moved)
        .collect()
}

/// The reorder that moves `id` one place `step`; `None` at either end and for a
/// collection, which has no siblings to reorder among.
pub(crate) fn step_command(roots: &[Node], id: &str, step: Step) -> Option<TreeCommand> {
    let (_, parent) = with_parent(roots, id)?;
    let index = parent.children.iter().position(|child| child.id == id)?;
    let index = match step {
        Step::Up => index.checked_sub(1)?,
        Step::Down => Some(index + 1).filter(|next| *next < parent.children.len())?,
    };
    Some(TreeCommand::Reorder {
        id: id.to_owned(),
        index,
    })
}

/// The command a drop of `dragged` at `place` runs: a reorder within the same parent, a
/// move into another. `None` when nothing would change: a collection is dragged, or a row
/// is dropped before itself. A folder dropped into itself or below reaches the engine,
/// which refuses it with the reason.
pub(crate) fn drop_command(
    roots: &[Node],
    dragged: &str,
    place: DropPlace<'_>,
) -> Option<TreeCommand> {
    let (_, from) = with_parent(roots, dragged)?;
    let (parent, index) = match place {
        DropPlace::Before(target) if target == dragged => return None,
        DropPlace::Before(target) => match with_parent(roots, target) {
            Some((_, parent)) => {
                let index = siblings_without(parent, dragged)
                    .iter()
                    .position(|id| *id == target)?;
                (parent, index)
            }
            // A collection row takes the drop inside.
            None => return drop_command(roots, dragged, DropPlace::Inside(target)),
        },
        DropPlace::Inside(container) => {
            let parent = path_to(roots, container)?.last().copied()?;
            if matches!(parent.kind, NodeKind::Request { .. }) {
                return None;
            }
            (parent, siblings_without(parent, dragged).len())
        }
    };
    Some(if parent.id == from.id {
        TreeCommand::Reorder {
            id: dragged.to_owned(),
            index,
        }
    } else {
        TreeCommand::Move {
            id: dragged.to_owned(),
            parent: parent.id.clone(),
            index,
        }
    })
}

/// What follows the cursor while a row is dragged: its name.
pub(crate) struct DragPreview(pub(crate) String);

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = bolt::colors(cx);
        ui::role(div(), TextVariant::Body, cx)
            .flex()
            .items_center()
            .h(TreeMetrics::ROW_HEIGHT)
            .pl(TreeMetrics::PADDING_X)
            .pr(TreeMetrics::PADDING_X)
            .bg(bolt::brand::colors(cx).selected_item)
            .text_color(colors.strong)
            .child(self.0.clone())
    }
}
