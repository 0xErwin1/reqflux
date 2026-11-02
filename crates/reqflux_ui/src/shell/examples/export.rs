//! The response's Save as example button (`response-save-example`) and Export button
//! (`response-export`), and the Export menu `response-export-menu` it opens at the pointer:
//! Save response body to file… (`response-export-save-body`), Save as example…
//! (`response-export-save-example`) and Copy body (`response-export-copy`). As drawn in
//! design/examples/01-tree-children, States. The response pane places the buttons; the shell
//! draws the menu over the window.

use bolt::composites::{MenuItem as RowItem, menu_row};
use bolt::controls::Button;
use gpui::{AnyElement, ClickEvent, Context, IntoElement, Pixels, Point, Window, prelude::*};
use gpui_kit_assets::IconName;

use crate::shell::Shell;
use crate::shell::request::{option_menu, ui};

/// The open Export menu: the request whose response it acts on, and where it opens.
pub(crate) struct ExportMenu {
    request: String,
    at: Point<Pixels>,
}

/// The Save as example and Export buttons of the response of the request `id`.
pub(crate) fn response_buttons(id: &str, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let request = id.to_owned();
    vec![
        Button::new("response-save-example", "Save as example")
            .secondary()
            .icon(ui::icon(IconName::Bookmark))
            .on_click(cx.listener(|this, _event, window, cx| this.open_save_example(window, cx)))
            .into_any_element(),
        Button::new("response-export", "Export")
            .secondary()
            .icon(ui::icon(IconName::FileDown))
            .trailing_icon(ui::icon(IconName::ChevronDown))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_export_menu(&request, event.position(), window, cx);
            }))
            .into_any_element(),
    ]
}

/// An item of the Export menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportItem {
    SaveBody,
    SaveExample,
    Copy,
}

impl ExportItem {
    const ALL: [Self; 3] = [Self::SaveBody, Self::SaveExample, Self::Copy];

    fn slug(self) -> &'static str {
        match self {
            Self::SaveBody => "save-body",
            Self::SaveExample => "save-example",
            Self::Copy => "copy",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SaveBody => "Save response body to file\u{2026}",
            Self::SaveExample => "Save as example\u{2026}",
            Self::Copy => "Copy body",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::SaveBody => IconName::FileDown,
            Self::SaveExample => IconName::Bookmark,
            Self::Copy => IconName::Copy,
        }
    }
}

impl Shell {
    /// Opens the Export menu of the response of `request` at `at`, in window coordinates.
    pub(crate) fn open_export_menu(
        &mut self,
        request: &str,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.examples.export = Some(ExportMenu {
            request: request.to_owned(),
            at,
        });
        self.examples.export_focus.focus(window, cx);
        cx.notify();
    }

    /// Escape or a click outside: closes the Export menu.
    pub(crate) fn close_export_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.examples.export = None;
        self.restore_focus(window, cx);
    }

    fn pick_export(&mut self, item: ExportItem, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.examples.export.take() else {
            return;
        };
        self.restore_focus(window, cx);
        match item {
            ExportItem::SaveBody => self.save_response_body(cx),
            ExportItem::SaveExample => self.open_save_example(window, cx),
            ExportItem::Copy => self.copy_response_body(&menu.request, cx),
        }
    }
}

/// The Export menu, while it is open.
pub(crate) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let menu = shell.examples.export.as_ref()?;
    let rows = ExportItem::ALL
        .into_iter()
        .map(|item| {
            let row = RowItem::new(item.label()).icon(ui::icon(item.icon()));
            menu_row(format!("response-export-{}", item.slug()), &row, false, cx)
                .aria_label(item.label())
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.pick_export(item, window, cx);
                }))
                .into_any_element()
        })
        .collect();
    Some(option_menu::render_at(
        ("response-export-menu", "response-export-backdrop"),
        menu.at,
        &shell.examples.export_focus,
        rows,
        Shell::close_export_menu,
        window,
        cx,
    ))
}
