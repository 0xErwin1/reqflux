//! Editing the tree from the sidebar (design/tree-editing/01-context-menu-modal): the row
//! menu of [`menu`], the inline rename field and its hint, the delete dialog of
//! [`delete_dialog`] and the error of the last failed operation.
//!
//! The sidebar never touches the workspace: it emits a [`TreeCommand`], and the shell, which
//! owns the engine, runs it and reports back through [`Sidebar::tree_changed`] or
//! [`Sidebar::show_error`].

use std::collections::HashSet;

use bolt::controls::{Button, GpuiInput, InputEnter, InputEscape, InputState};
use bolt::primitives::{Chamfer, TextVariant};
use bolt::tokens::{ChamferCut, Fields, HeaderMetrics, Spacing, TreeMetrics};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, Styled, Window, div, prelude::*,
};

use super::request::ui;
use super::sidebar::Sidebar;
use super::tree_model::Node;
use super::{region, text_region};
use crate::tokens;

mod delete_dialog;
mod menu;

#[cfg(test)]
pub(crate) use delete_dialog::{
    delete_body, delete_button, delete_note, delete_question, delete_title,
};
use menu::{MenuItem, MenuPage};

#[cfg(test)]
#[path = "sidebar_edit_tests.rs"]
mod tests;

/// The hover group of a tree row, which shows its action button.
pub(super) const ROW_GROUP: &str = "tree-row";

/// A tree operation the sidebar asks the shell to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TreeCommand {
    /// Creates a collection.
    NewCollection,
    /// Creates a request at the end of the collection or folder with this id.
    NewRequest(String),
    /// Creates a folder at the end of the collection or folder with this id.
    NewFolder(String),
    /// Renames the entity `id` to `name`.
    Rename { id: String, name: String },
    /// Duplicates the request or folder with this id.
    Duplicate(String),
    /// Deletes the entity with this id and everything under it.
    Delete(String),
    /// Moves the request or folder `id` to `index` among its siblings.
    Reorder { id: String, index: usize },
    /// Moves the request or folder `id` into the collection or folder `parent` at `index`.
    Move {
        id: String,
        parent: String,
        index: usize,
    },
}

/// Emitted when the delete dialog opens: the ids of the requests it lists, so the shell can
/// answer with [`Sidebar::set_unsaved`] before the dialog is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeletePrompted {
    pub(crate) ids: Vec<String>,
}

/// The open rename field: the entity it renames, the name it started from, whether the
/// entity was just created, and its input.
pub(crate) struct Rename {
    id: String,
    original: String,
    created: bool,
    input: Entity<InputState>,
}

/// An entity the shell just reported, which opens into rename once the field can be made
/// with a window; `created` when a New command made it, not a duplicate.
struct PendingRename {
    id: String,
    created: bool,
}

/// What the sidebar shows while the tree is edited.
pub(crate) struct TreeEdit {
    /// The id of the row whose menu is open.
    menu: Option<String>,
    /// Where the open menu sits, in window coordinates: the pointer when it opened.
    menu_at: Point<Pixels>,
    /// Whether the open menu shows the actions or the Move to… destinations.
    page: MenuPage,
    /// Focused while the menu is open.
    menu_focus: FocusHandle,
    rename: Option<Rename>,
    pending_rename: Option<PendingRename>,
    /// Set while a duplicate runs: the copy it reports opens into rename.
    rename_copy: bool,
    /// The id of the entity whose delete waits for a confirmation.
    confirm: Option<String>,
    /// The requests with an unsaved draft, as the shell reported them for the delete dialog.
    unsaved: HashSet<String>,
    /// The id and name of the row being dragged.
    pub(super) dragging: Option<(String, String)>,
    /// Why the last operation failed.
    error: Option<String>,
}

impl TreeEdit {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            menu: None,
            menu_at: Point::default(),
            page: MenuPage::Actions,
            menu_focus: cx.focus_handle(),
            rename: None,
            pending_rename: None,
            rename_copy: false,
            confirm: None,
            unsaved: HashSet::new(),
            dragging: None,
            error: None,
        }
    }
}

/// The hint under the rename field.
fn rename_hint(created: bool) -> &'static str {
    if created {
        "Enter creates · Esc cancels"
    } else {
        "Enter saves · Esc cancels"
    }
}

impl Sidebar {
    /// Emits `command` for the shell to run. A command that creates an entity opens it into
    /// rename once the shell has reported it, which happens before the deferred step runs.
    pub(super) fn command(
        &mut self,
        command: TreeCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit.menu = None;
        self.edit.rename_copy = matches!(command, TreeCommand::Duplicate(_));
        cx.emit(command);
        cx.defer_in(window, |this, window, cx| {
            this.edit.rename_copy = false;
            if let Some(pending) = this.edit.pending_rename.take() {
                this.start_rename(&pending.id, pending.created, window, cx);
            }
        });
        cx.notify();
    }

    /// The shell ran an operation: the error clears, and `created`, when the operation made
    /// an entity, is selected and shown; `rename`, or a duplicate, also opens it into rename.
    pub(crate) fn tree_changed(
        &mut self,
        roots: Vec<Node>,
        created: Option<String>,
        rename: bool,
        cx: &mut Context<Self>,
    ) {
        self.edit.error = None;
        self.set_roots(roots, cx);
        if let Some(id) = created {
            self.update_tree(cx, |tree| tree.reveal(&id));
            if rename || self.edit.rename_copy {
                self.edit.pending_rename = Some(PendingRename {
                    id,
                    created: rename,
                });
            }
        }
        cx.notify();
    }

    /// Shows why the last operation failed, above the tree.
    pub(crate) fn show_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.edit.error = Some(error);
        cx.notify();
    }

    /// The requests with an unsaved draft, which the delete dialog flags.
    pub(crate) fn set_unsaved(&mut self, ids: HashSet<String>, cx: &mut Context<Self>) {
        self.edit.unsaved = ids;
        cx.notify();
    }

    /// Why the last operation failed, while the sidebar shows it.
    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&str> {
        self.edit.error.as_deref()
    }

    /// The text of the rename field, while it is open.
    #[cfg(test)]
    pub(crate) fn rename_text(&self, cx: &App) -> Option<String> {
        self.edit
            .rename
            .as_ref()
            .map(|rename| rename.input.read(cx).value().to_string())
    }

    /// The hint under the rename field, while it is open.
    #[cfg(test)]
    pub(crate) fn rename_hint(&self) -> Option<&'static str> {
        self.edit
            .rename
            .as_ref()
            .map(|rename| rename_hint(rename.created))
    }

    /// Escape: cancels a drag, closes the menu or the delete dialog, and clears the error.
    /// Returns whether it closed something, so the shell does not also cancel a send.
    pub(crate) fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.edit.dragging.take().is_some() && cx.stop_active_drag(window) {
            cx.notify();
            return true;
        }
        let had_error = self.edit.error.take().is_some();
        let closed = if self.edit.menu.is_some() {
            self.close_menu(window, cx);
            true
        } else {
            self.edit.confirm.take().is_some()
        };
        if had_error || closed {
            cx.notify();
        }
        closed
    }

    /// Opens the menu of the row `id` on its actions at `position`, the pointer in window
    /// coordinates, selecting the row.
    pub(super) fn open_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_tree(cx, |tree| tree.select(id));
        self.edit.menu = Some(id.to_owned());
        self.edit.menu_at = position;
        self.edit.page = MenuPage::Actions;
        self.edit.menu_focus.focus(window, cx);
        cx.notify();
    }

    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit.menu = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Opens the rename field over the name of `id`, seeded with the name, all selected.
    fn start_rename(
        &mut self,
        id: &str,
        created: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.node(id).map(|node| node.name.clone()) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(name.clone(), window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        self.edit.menu = None;
        self.edit.rename = Some(Rename {
            id: id.to_owned(),
            original: name,
            created,
            input,
        });
        cx.notify();
    }

    /// Enter, or a click outside the rename field: renames through the shell unless the name
    /// is unchanged.
    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.edit.rename.take() else {
            return;
        };
        let name = rename.input.read(cx).value().to_string();
        self.focus.focus(window, cx);
        if name == rename.original {
            cx.notify();
            return;
        }
        self.command(
            TreeCommand::Rename {
                id: rename.id,
                name,
            },
            window,
            cx,
        );
    }

    /// Escape in the rename field: closes it and keeps the name; on an entity just
    /// created, cancels the creation by deleting it.
    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.edit.rename.take() else {
            return;
        };
        self.focus.focus(window, cx);
        if rename.created {
            self.command(TreeCommand::Delete(rename.id), window, cx);
        } else {
            cx.notify();
        }
    }

    /// F2 on the tree: renames the selected row.
    pub(super) fn rename_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_on_selected(MenuItem::Rename, window, cx);
    }

    /// Ctrl+D on the tree: duplicates the selected request or folder.
    pub(super) fn duplicate_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_on_selected(MenuItem::Duplicate, window, cx);
    }

    /// Delete on the tree: asks to delete the selected row.
    pub(super) fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_on_selected(MenuItem::Delete, window, cx);
    }

    /// Runs the menu item `item` on the selected row, when it applies there and no rename
    /// field takes the keys.
    fn pick_on_selected(&mut self, item: MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.rename.is_some() {
            return;
        }
        let Some(id) = self.selected().map(str::to_owned) else {
            return;
        };
        if self
            .node(&id)
            .is_some_and(|node| item.applies_to(&node.kind))
        {
            self.pick(&id, item, window, cx);
        }
    }

    /// The menu item `pick` on the row `id`.
    fn pick(&mut self, id: &str, pick: MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        self.edit.menu = None;
        // New items go into a collection or folder; on a request, into the request's parent.
        let container = match self.node(id) {
            Some(node) if node.is_expandable() => Some(id.to_owned()),
            Some(_) | None => self.parent(id),
        };
        match (pick, container) {
            (MenuItem::Open, _) => {
                self.open_if_request(id, cx);
                self.close_menu(window, cx);
            }
            (MenuItem::NewRequest, Some(parent)) => {
                self.command(TreeCommand::NewRequest(parent), window, cx);
            }
            (MenuItem::NewFolder, Some(parent)) => {
                self.command(TreeCommand::NewFolder(parent), window, cx);
            }
            (MenuItem::NewRequest | MenuItem::NewFolder, None) => self.close_menu(window, cx),
            (MenuItem::Rename, _) => self.start_rename(id, false, window, cx),
            (MenuItem::Duplicate, _) => {
                self.command(TreeCommand::Duplicate(id.to_owned()), window, cx);
            }
            (MenuItem::MoveTo, _) => {
                self.edit.menu = Some(id.to_owned());
                self.edit.page = MenuPage::MoveTo;
                self.edit.menu_focus.focus(window, cx);
                cx.notify();
            }
            (MenuItem::Delete, _) => self.prompt_delete(id, window, cx),
        }
    }

    /// Opens the delete dialog for `id` and tells the shell which requests it lists.
    fn prompt_delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.node(id).map(Node::request_ids).unwrap_or_default();
        self.edit.confirm = Some(id.to_owned());
        self.edit.unsaved.clear();
        self.focus.focus(window, cx);
        cx.emit(DeletePrompted { ids });
        cx.notify();
    }

    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.edit.confirm.take() {
            self.command(TreeCommand::Delete(id), window, cx);
        }
    }

    fn cancel_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit.confirm = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The rename field over the name of the row `id`, when it is open there.
    pub(super) fn rename_field(&self, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rename = self.edit.rename.as_ref().filter(|rename| rename.id == id)?;
        let colors = bolt::colors(cx);
        Some(
            region("tree-rename-input")
                .role(gpui::Role::TextInput)
                .relative()
                .flex_1()
                .min_w_0()
                .h(TreeMetrics::ROW_HEIGHT)
                .flex()
                .items_center()
                .pl(Spacing::XXS)
                .pr(Spacing::XXS)
                .child(
                    Chamfer::new(ChamferCut::CONTROL)
                        .fill(colors.background)
                        .border(colors.line),
                )
                .on_action(cx.listener(|this, _: &InputEnter, window, cx| {
                    this.commit_rename(window, cx);
                }))
                .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                    this.cancel_rename(window, cx);
                }))
                .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                    this.commit_rename(window, cx);
                }))
                // bolt gap: bolt's frameless Input draws its text in the medium weight with no
                // option, so the name would change weight from how the row shows it.
                .child(
                    GpuiInput::new(&rename.input)
                        .appearance(false)
                        .bordered(false)
                        .p_0()
                        .flex_1()
                        .min_w_0()
                        .text_size(TreeMetrics::FONT)
                        .text_color(colors.strong),
                )
                .into_any_element(),
        )
    }

    /// The hint `tree-rename-hint` under the row `id` at `depth`, while it is renamed.
    pub(super) fn rename_hint_row(&self, id: &str, depth: usize, cx: &App) -> Option<AnyElement> {
        let rename = self.edit.rename.as_ref().filter(|rename| rename.id == id)?;
        let hint = rename_hint(rename.created);
        let indent = bolt::fonts::ui_px(cx, TreeMetrics::INDENT) * depth as f32;
        Some(
            ui::role(text_region("tree-rename-hint"), TextVariant::Caption, cx)
                .aria_label(hint)
                .flex_none()
                .pl(TreeMetrics::PADDING_X + indent + tokens::TREE_HINT_OFFSET)
                .pr(TreeMetrics::PADDING_X)
                .pt(TreeMetrics::SELECTION_BAR)
                .pb(Spacing::XS)
                .child(hint)
                .into_any_element(),
        )
    }

    /// The `…` button of the row `id`, shown while the row is selected or hovered, and the
    /// row's menu while it is open.
    pub(super) fn row_actions(
        &self,
        node: &Node,
        selected: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let id = node.id.clone();
        let button = div()
            .flex_none()
            .when(!selected, |button| {
                button
                    .invisible()
                    .group_hover(ROW_GROUP, |style| style.visible())
            })
            .child(
                Button::new(
                    gpui::ElementId::Name(format!("tree-row-menu-{id}").into()),
                    "Actions",
                )
                .icon(ui::icon(IconName::Ellipsis))
                .icon_only()
                .inline()
                .ghost()
                .tab_stop(false)
                .on_click(cx.listener(
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_menu(&id, event.position(), window, cx);
                    },
                )),
            )
            .into_any_element();
        let mut actions = vec![button];
        if self.edit.menu.as_deref() == Some(node.id.as_str()) {
            actions.push(self.menu(node, window, cx));
        }
        actions
    }

    /// The error of the last failed operation, `sidebar-error`, in the danger tone.
    pub(super) fn error_label(&self, cx: &App) -> Option<AnyElement> {
        let error = self.edit.error.clone()?;
        let colors = bolt::colors(cx);
        Some(
            ui::role(text_region("sidebar-error"), TextVariant::Caption, cx)
                .aria_label(error.clone())
                .flex_none()
                .pl(HeaderMetrics::PANEL_PADDING_LEFT)
                .pr(HeaderMetrics::PANEL_PADDING_LEFT)
                .pb(Fields::GAP)
                .text_color(colors.danger)
                .child(error)
                .into_any_element(),
        )
    }
}
