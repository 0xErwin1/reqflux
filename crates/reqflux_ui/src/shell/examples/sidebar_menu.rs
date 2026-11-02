//! The row menu of an example, `tree-menu` as on any row, reduced to what an example supports:
//! Open (`tree-menu-open`, Enter), Rename (`tree-menu-rename`, F2), Save body to file…
//! (`tree-menu-save-body`), then Delete (`tree-menu-delete`, Del), which asks first in the
//! dialog `example-delete-prompt`: it names the example's file and says its request is not
//! changed, with `example-delete-cancel` and `example-delete-confirm`.

use bolt::composites::{MenuItem as RowItem, menu_row, render_separator};
use bolt::controls::Button;
use bolt::modals::{Modal, modal_lead};
use bolt::primitives::TextVariant;
use bolt::tokens::ModalMetrics;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Point, Role, Styled, Window, anchored,
    deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::ExampleCommand;
use crate::shell::documents::locate_example;
use crate::shell::request::{option_menu, ui};
use crate::shell::sidebar::Sidebar;
use crate::shell::tree_model::Node;
use crate::shell::{region, text_region};
use crate::tokens;

/// An item of an example's row menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExampleItem {
    Open,
    Rename,
    SaveBody,
    Delete,
}

impl ExampleItem {
    const ALL: [Self; 4] = [Self::Open, Self::Rename, Self::SaveBody, Self::Delete];

    fn slug(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Rename => "rename",
            Self::SaveBody => "save-body",
            Self::Delete => "delete",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Rename => "Rename",
            Self::SaveBody => "Save body to file\u{2026}",
            Self::Delete => "Delete",
        }
    }

    fn key(self) -> Option<&'static str> {
        match self {
            Self::Open => Some("Enter"),
            Self::Rename => Some("F2"),
            Self::Delete => Some("Del"),
            Self::SaveBody => None,
        }
    }

    fn icon(self) -> Option<IconName> {
        match self {
            Self::Open => Some(IconName::ExternalLink),
            Self::Rename => Some(IconName::Pencil),
            Self::SaveBody => Some(IconName::FileDown),
            Self::Delete => Some(IconName::Trash),
        }
    }
}

/// The title of the delete dialog.
pub(crate) fn delete_title(name: &str) -> String {
    format!("Delete example {name}?")
}

/// What the delete dialog says: the file it removes and that the request stays.
pub(crate) fn delete_body(file: &str, request: &str) -> String {
    format!("Removes {file}. {request} is not changed.")
}

impl Sidebar {
    /// The open menu of the example row `node`.
    pub(in crate::shell) fn example_menu(
        &self,
        node: &Node,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = Vec::new();
        for item in ExampleItem::ALL {
            if item == ExampleItem::Delete {
                rows.push(
                    render_separator(cx)
                        .debug_selector(|| "tree-menu-separator-0".to_owned())
                        .into_any_element(),
                );
            }
            let mut row = RowItem::new(item.label());
            if let Some(icon) = item.icon() {
                row = row.icon(ui::icon(icon));
            }
            if let Some(key) = item.key() {
                row = row.shortcut(key);
            }
            if item == ExampleItem::Delete {
                row = row.danger();
            }
            let id = node.id.clone();
            rows.push(
                menu_row(format!("tree-menu-{}", item.slug()), &row, false, cx)
                    .aria_label(item.label())
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.pick_example(&id, item, window, cx);
                    }))
                    .into_any_element(),
            );
        }
        option_menu::render_at(
            ("tree-menu", "tree-menu-backdrop"),
            self.edit.menu_at,
            &self.edit.menu_focus,
            rows,
            Sidebar::close_menu,
            window,
            cx,
        )
    }

    /// Runs `item` of the example menu on the example `id`.
    fn pick_example(
        &mut self,
        id: &str,
        item: ExampleItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item {
            ExampleItem::Open => {
                self.close_menu(window, cx);
                self.open_if_example(id, cx);
            }
            ExampleItem::Rename => self.start_rename(id, false, window, cx),
            ExampleItem::SaveBody => {
                self.close_menu(window, cx);
                cx.emit(ExampleCommand::SaveBody(id.to_owned()));
            }
            ExampleItem::Delete => {
                self.close_menu(window, cx);
                self.prompt_example_delete(id, window, cx);
            }
        }
    }

    /// Opens the delete dialog of the example `id`.
    pub(in crate::shell) fn prompt_example_delete(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit.confirm_example = Some(id.to_owned());
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn confirm_example_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.edit.confirm_example.take() {
            cx.emit(ExampleCommand::Delete(id));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn cancel_example_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit.confirm_example = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The delete dialog of an example, while its delete waits for it.
    pub(in crate::shell) fn example_delete_prompt(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let place = locate_example(self.roots(), self.edit.confirm_example.as_deref()?)?;
        let title = delete_title(&place.name);
        let body = delete_body(&place.file, &place.location.name);
        let prompt = region("example-delete-prompt")
            .role(Role::Dialog)
            .aria_label(title.clone())
            .flex()
            .flex_col()
            .gap(ModalMetrics::BODY_GAP)
            .child(
                ui::role(
                    text_region("example-delete-title"),
                    TextVariant::Heading,
                    cx,
                )
                .child(title.clone()),
            )
            .child(modal_lead(
                text_region("example-delete-body")
                    .aria_label(body.clone())
                    .child(body),
                cx,
            ));
        let footer = div()
            .flex()
            .gap(ModalMetrics::FOOTER_GAP)
            .child(
                Button::new("example-delete-cancel", "Cancel")
                    .ghost()
                    .kbd("Esc")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.cancel_example_delete(window, cx);
                    })),
            )
            .child(
                Button::new("example-delete-confirm", "Delete")
                    .danger()
                    .icon(ui::icon(IconName::Trash))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.confirm_example_delete(window, cx);
                    })),
            );
        let sidebar = cx.entity().downgrade();
        let modal = Modal::new(title)
            .id("example-delete-backdrop")
            .danger()
            .without_header()
            .top_offset(tokens::TREE_DIALOG_TOP)
            .body(prompt)
            .footer(footer)
            .on_close(move |window, cx| {
                let closed = sidebar.update(cx, |this, cx| this.cancel_example_delete(window, cx));
                if let Err(error) = closed {
                    log::warn!("example delete dialog close: {error}");
                }
            });
        let viewport = window.viewport_size();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(
                    deferred(
                        anchored().position(Point::default()).child(
                            div()
                                .relative()
                                .w(viewport.width)
                                .h(viewport.height)
                                .child(modal),
                        ),
                    )
                    .with_priority(3),
                )
                .into_any_element(),
        )
    }
}
