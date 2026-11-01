//! The sidebar island: the `COLLECTIONS` header with the new-collection button, the filter
//! field, the error of the last failed tree operation, the collection tree and a footer with
//! the request and collection counts, or the row being dragged and how to cancel. The filter does
//! nothing yet. While the rail's History view is active, the History panel of
//! [`super::sidebar_history`] takes the tree's place; its Environments view opens the
//! workspace settings page, which the shell draws in place of the sidebar.
//!
//! The tree keeps keyboard focus as a whole; its rows are not focusable. Navigation runs
//! through the actions below, bound in the `SidebarTree` key context, which also rename (F2),
//! duplicate (Ctrl+D) and delete (Delete) the selected row.
//!
//! Clicking a request row, or Enter on a selected one, emits [`OpenRequest`]; the shell
//! subscribes to it and opens the request in a document tab. The sidebar knows nothing of
//! the documents. Tree edits go the same way: see [`super::sidebar_edit`].

use bolt::composites::Island;
use bolt::controls::Button;
use bolt::primitives::{Chamfer, Icon, Kbd, TextVariant};
use bolt::tokens::{Borders, ChamferCut, Fields, HeaderMetrics, ShellMetrics, TreeMetrics};
use bolt_legacy::IconName;
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyBinding, MouseButton, MouseDownEvent, ParentElement, Render, Styled, Window, actions, div,
    prelude::*,
};

use super::request::ui;
use super::rows::{NameDecorator, RowSlots};
use super::sidebar_edit::{DeletePrompted, TreeCommand, TreeEdit};
use super::sidebar_history::{HistoryRow, OpenHistory, Panel};
use super::tree_drag::{self, DragPreview, DraggedRow, DropPlace, Step};
use super::tree_model::{self, Node, NodeKind, TreeState};
use super::{region, rows, text_region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The key context of the collection tree.
pub(crate) const TREE_CONTEXT: &str = "SidebarTree";

actions!(
    sidebar_tree,
    [
        /// Selects the previous visible row.
        SelectPrevious,
        /// Selects the next visible row.
        SelectNext,
        /// Collapses the selected row, or selects its parent.
        CollapseOrSelectParent,
        /// Expands the selected row, or selects its first child.
        ExpandOrSelectChild,
        /// Expands or collapses the selected collection or folder, or opens the selected
        /// request.
        ToggleSelected,
        /// Moves the selected row one place up among its siblings.
        MoveUp,
        /// Moves the selected row one place down among its siblings.
        MoveDown,
        /// Opens the selected row into rename.
        RenameSelected,
        /// Duplicates the selected request or folder.
        DuplicateSelected,
        /// Asks to delete the selected row.
        DeleteSelected,
    ]
);

/// Binds the tree navigation keys in [`TREE_CONTEXT`].
pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(TREE_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("left", CollapseOrSelectParent, context),
        KeyBinding::new("right", ExpandOrSelectChild, context),
        KeyBinding::new("enter", ToggleSelected, context),
        KeyBinding::new("alt-up", MoveUp, context),
        KeyBinding::new("alt-down", MoveDown, context),
        KeyBinding::new("f2", RenameSelected, context),
        KeyBinding::new("ctrl-d", DuplicateSelected, context),
        KeyBinding::new("delete", DeleteSelected, context),
    ]);
}

/// The footer text while the row `name` is dragged.
pub(crate) fn moving_text(name: &str) -> String {
    format!("Moving {name} · Esc cancels")
}

/// The footer text: `<n> requests · <m> collections`, singular for one.
pub(crate) fn footer_text(requests: usize, collections: usize) -> String {
    let plural = |count: usize, noun: &str| match count {
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    };
    format!(
        "{} · {}",
        plural(requests, "request"),
        plural(collections, "collection")
    )
}

/// Emitted when the user opens the request with this id from the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenRequest(pub(crate) String);

/// The sidebar view.
pub(crate) struct Sidebar {
    /// `None` when no workspace is loaded: the island stays empty.
    tree: Option<TreeState>,
    pub(super) focus: FocusHandle,
    /// The row menu, the rename field, the delete confirmation and the last error.
    pub(super) edit: TreeEdit,
    /// Whether the island shows the tree or the History panel.
    pub(super) panel: Panel,
    /// The rows of the History panel, newest first.
    pub(super) history: Vec<HistoryRow>,
}

impl Sidebar {
    pub(crate) fn new(workspace: &OpenedWorkspace, cx: &mut Context<Self>) -> Self {
        let tree = match workspace {
            OpenedWorkspace::Loaded(engine) => {
                Some(TreeState::new(tree_model::nodes(engine.workspace())))
            }
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
        };
        Self {
            tree,
            focus: cx.focus_handle(),
            edit: TreeEdit::new(cx),
            panel: Panel::default(),
            history: Vec::new(),
        }
    }

    /// The id of the selected tree node.
    pub(crate) fn selected(&self) -> Option<&str> {
        self.tree.as_ref().and_then(TreeState::selected)
    }

    /// The node `id`, shown or not.
    pub(super) fn node(&self, id: &str) -> Option<&Node> {
        self.tree.as_ref().and_then(|tree| tree.node(id))
    }

    /// The roots of the tree; empty when no workspace is loaded.
    pub(super) fn roots(&self) -> &[Node] {
        self.tree.as_ref().map_or(&[], TreeState::roots)
    }

    /// The id of the collection or folder holding `id`.
    pub(super) fn parent(&self, id: &str) -> Option<String> {
        self.tree
            .as_ref()
            .and_then(|tree| tree.parent_of(id))
            .map(|parent| parent.id.clone())
    }

    pub(super) fn update_tree(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut TreeState),
    ) {
        if let Some(tree) = &mut self.tree {
            change(tree);
            cx.notify();
        }
    }

    /// Replaces the tree's roots after the workspace was reloaded from disk, keeping the
    /// expansion and selection the tree had.
    pub(crate) fn set_roots(&mut self, roots: Vec<Node>, cx: &mut Context<Self>) {
        self.update_tree(cx, |tree| tree.set_roots(roots));
    }

    /// Selects the row `id` and focuses the tree; `toggle` also expands or collapses it.
    /// A click on a request row, not on a chevron, also opens the request.
    fn click_row(&mut self, id: &str, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_tree(cx, |tree| {
            tree.select(id);
            if toggle {
                tree.toggle(id);
            }
        });
        if !toggle {
            self.open_if_request(id, cx);
        }
        self.focus.focus(window, cx);
    }

    /// Enter: opens the selected request, or expands or collapses the selected collection or
    /// folder.
    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        let selected = self
            .tree
            .as_ref()
            .and_then(TreeState::selected)
            .map(str::to_owned);
        match selected {
            Some(id) if self.open_if_request(&id, cx) => {}
            Some(_) | None => self.update_tree(cx, TreeState::toggle_selected),
        }
    }

    /// Alt+Up or Alt+Down: moves the selected row one place among its siblings. Nothing
    /// happens at either end or on a collection.
    fn step_selected(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.tree.as_ref().and_then(|tree| {
            tree.selected()
                .and_then(|id| tree_drag::step_command(tree.roots(), id, step))
        });
        if let Some(command) = command {
            self.command(command, window, cx);
        }
    }

    /// Drops the dragged row `dragged` at `place`.
    fn drop_row(
        &mut self,
        dragged: &str,
        place: DropPlace<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit.dragging = None;
        let command = self
            .tree
            .as_ref()
            .and_then(|tree| tree_drag::drop_command(tree.roots(), dragged, place));
        if let Some(command) = command {
            self.command(command, window, cx);
        }
    }

    /// The drop target on the name of the collection or folder `id`: a drop there puts the
    /// dragged row inside at the end, and the name shows the drop-target wash and outline
    /// meanwhile.
    fn name_drop(&self, id: &str, cx: &mut Context<Self>) -> NameDecorator {
        let wash = bolt::brand::colors(cx).drop_target;
        let outline = bolt::brand::brand(cx).accent_tint;
        let id = id.to_owned();
        let on_drop = cx.listener(move |this, dragged: &DraggedRow, window, cx| {
            this.drop_row(&dragged.id, DropPlace::Inside(&id), window, cx);
        });
        Box::new(move |name| {
            name.drag_over::<DraggedRow>(move |style, _, _, _| {
                style.bg(wash).border(Borders::THIN).border_color(outline)
            })
            .on_drop(on_drop)
        })
    }

    /// The name of the row being dragged, while a drag runs.
    fn moving(&self, cx: &App) -> Option<&str> {
        self.edit
            .dragging
            .as_ref()
            .filter(|_| cx.has_active_drag())
            .map(|(_, name)| name.as_str())
    }

    /// Emits [`OpenRequest`] when `id` is a request; returns whether it did.
    pub(super) fn open_if_request(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let is_request = self.tree.as_ref().is_some_and(|tree| tree.is_request(id));
        if is_request {
            cx.emit(OpenRequest(id.to_owned()));
        }
        is_request
    }
}

impl EventEmitter<OpenRequest> for Sidebar {}

impl EventEmitter<TreeCommand> for Sidebar {}

impl EventEmitter<DeletePrompted> for Sidebar {}

impl EventEmitter<OpenHistory> for Sidebar {}

impl Focusable for Sidebar {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// The sidebar header, drawn as DBFlux's: the `COLLECTIONS` label and the icon-only
/// new-collection button.
fn header(cx: &mut Context<Sidebar>) -> impl IntoElement {
    region("collections-header")
        .flex_none()
        .h(ShellMetrics::SIDEBAR_HEADER_HEIGHT)
        .flex()
        .items_center()
        .gap(HeaderMetrics::PANEL_GAP)
        .pl(HeaderMetrics::PANEL_PADDING_LEFT)
        .pr(HeaderMetrics::PANEL_PADDING_RIGHT)
        .child(
            ui::role(text_region("collections-label"), TextVariant::Label, cx)
                .text_size(ShellMetrics::SECTION_LABEL_FONT)
                .child("COLLECTIONS"),
        )
        .child(div().flex_1())
        .child(
            Button::new("sidebar-new-collection", "New collection")
                .icon(ui::icon(IconName::Plus))
                .icon_size(ShellMetrics::SIDEBAR_FILTER_ICON)
                .icon_only()
                .tab_stop(false)
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.command(TreeCommand::NewCollection, window, cx);
                })),
        )
}

/// The filter field, drawn as DBFlux's sidebar filter: a 30 px chamfered field with the
/// search icon, the placeholder and the `/` keycap.
fn filter(cx: &App) -> impl IntoElement {
    let colors = bolt::colors(cx);
    div()
        .flex_none()
        .pl(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
        .pr(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
        .pb(ShellMetrics::SIDEBAR_FILTER_PADDING_BOTTOM)
        .child(
            region("tree-filter")
                .relative()
                .flex()
                .items_center()
                .gap(Fields::GAP)
                .h(Fields::HEIGHT)
                .pl(Fields::PADDING_X)
                .pr(Fields::PADDING_X)
                .text_size(Fields::TEXT)
                .text_color(colors.muted)
                .child(
                    Chamfer::new(ChamferCut::CONTROL)
                        .fill(colors.background)
                        .border(colors.line),
                )
                .child(
                    Icon::new(ui::icon(IconName::Search))
                        .size(ShellMetrics::SIDEBAR_FILTER_ICON)
                        .color(colors.muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child("Filter requests"),
                )
                .child(keycap("tree-filter-keycap", "/")),
        )
}

/// The keycap `key` that tests and the automation tree find by `id`.
// bolt gap: Kbd takes no element id; drop the wrapper once it does.
pub(super) fn keycap(id: &'static str, key: &'static str) -> impl IntoElement {
    text_region(id)
        .aria_label(key)
        .flex_none()
        .child(Kbd::new(key))
}

/// The footer, drawn as DBFlux's: 46 px, the counts in muted 12 px text.
fn footer(tree: &TreeState, moving: Option<&str>, cx: &App) -> impl IntoElement {
    let footer = match moving {
        Some(name) => moving_text(name),
        None => footer_text(tree.request_count(), tree.collection_count()),
    };
    text_region("sidebar-footer")
        .aria_label(footer.clone())
        .flex_none()
        .h(ShellMetrics::SIDEBAR_FOOTER_HEIGHT)
        .flex()
        .items_center()
        .pl(ShellMetrics::SIDEBAR_FOOTER_PADDING_X)
        .pr(ShellMetrics::SIDEBAR_FOOTER_PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ShellMetrics::SIDEBAR_FOOTER_FONT)
        .text_color(bolt::colors(cx).muted)
        .whitespace_nowrap()
        .child(footer)
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = region("sidebar-content").size_full().flex().flex_col();

        if self.panel == Panel::History && self.tree.is_some() {
            content = content.children(self.history_panel(cx));
        } else if let Some(tree) = &self.tree {
            let moving = self.moving(cx);
            let dragging = moving.and(self.edit.dragging.as_ref().map(|(id, _)| id.clone()));
            let sidebar = cx.weak_entity();
            let tree_rows: Vec<_> = tree
                .visible_rows()
                .into_iter()
                .flat_map(|row| {
                    let hint = self.rename_hint_row(&row.node.id, row.depth, cx);
                    let dimmed = dragging.as_deref() == Some(row.node.id.as_str());
                    let sidebar = sidebar.clone();
                    let selected = tree.selected() == Some(row.node.id.as_str());
                    let id = row.node.id.clone();
                    let chevron_id = id.clone();
                    let menu_id = id.clone();
                    let on_toggle = cx.listener(move |this, _event, window, cx| {
                        this.click_row(&chevron_id, true, window, cx);
                    });
                    let draggable = !matches!(row.node.kind, NodeKind::Collection);
                    let slots = RowSlots {
                        name: self.rename_field(&id, cx),
                        trailing: self.row_actions(row.node, selected, window, cx),
                        name_drop: row.node.is_expandable().then(|| self.name_drop(&id, cx)),
                    };
                    let dragged = DraggedRow {
                        id: id.clone(),
                        name: row.node.name.clone(),
                    };
                    let target = id.clone();
                    let line = bolt::brand::brand(cx).accent_tint;
                    let element = rows::tree_row(row, selected, slots, on_toggle, cx)
                        .when(dimmed, |element| {
                            element.opacity(tokens::TREE_DRAG_SOURCE_OPACITY)
                        })
                        .when(draggable, |element| {
                            element.on_drag(dragged, move |dragged, _, _, cx| {
                                let started = sidebar.update(cx, |this, cx| {
                                    this.edit.dragging =
                                        Some((dragged.id.clone(), dragged.name.clone()));
                                    cx.notify();
                                });
                                if let Err(error) = started {
                                    log::warn!("tree drag start: {error}");
                                }
                                cx.new(|_| DragPreview(dragged.name.clone()))
                            })
                        })
                        .drag_over::<DraggedRow>(move |style, _, _, _| {
                            style
                                .border_t(TreeMetrics::SELECTION_BAR)
                                .border_color(line)
                        })
                        .on_drop(cx.listener(move |this, dragged: &DraggedRow, window, cx| {
                            this.drop_row(&dragged.id, DropPlace::Before(&target), window, cx);
                        }))
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.click_row(&id, false, window, cx);
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                this.open_menu(&menu_id, event.position, window, cx);
                            }),
                        )
                        .into_any_element();
                    std::iter::once(element).chain(hint)
                })
                .collect();
            content = content
                .child(header(cx))
                .child(filter(cx))
                .children(self.error_label(cx))
                .children(self.delete_prompt(window, cx))
                .child(
                    region("collection-tree")
                        .track_focus(&self.focus)
                        .key_context(TREE_CONTEXT)
                        .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| {
                            this.update_tree(cx, TreeState::select_previous);
                        }))
                        .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                            this.update_tree(cx, TreeState::select_next);
                        }))
                        .on_action(cx.listener(|this, _: &CollapseOrSelectParent, _, cx| {
                            this.update_tree(cx, TreeState::collapse_or_select_parent);
                        }))
                        .on_action(cx.listener(|this, _: &ExpandOrSelectChild, _, cx| {
                            this.update_tree(cx, TreeState::expand_or_select_child);
                        }))
                        .on_action(cx.listener(|this, _: &ToggleSelected, _, cx| {
                            this.activate_selected(cx);
                        }))
                        .on_action(cx.listener(|this, _: &MoveUp, window, cx| {
                            this.step_selected(Step::Up, window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &MoveDown, window, cx| {
                            this.step_selected(Step::Down, window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &RenameSelected, window, cx| {
                            this.rename_selected(window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &DuplicateSelected, window, cx| {
                            this.duplicate_selected(window, cx);
                        }))
                        .on_action(cx.listener(|this, _: &DeleteSelected, window, cx| {
                            this.delete_selected(window, cx);
                        }))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .children(tree_rows),
                )
                .child(footer(tree, moving, cx));
        }

        region("sidebar")
            .flex_none()
            .w(ShellMetrics::SIDEBAR_WIDTH)
            .h_full()
            .child(Island::new().size_full().child(content))
    }
}
