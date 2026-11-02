//! The Save as example dialog, `example-save-dialog`: `bolt`'s modal with the request beside
//! the title, the name field `example-save-name` (the reason phrase at first, all selected),
//! a summary of the response `example-save-summary`, the note on what is kept and masked,
//! and Cancel (`example-save-cancel`) and Save example (`example-save-confirm`, Enter).
//! Escape or the backdrop closes it. As drawn in design/examples/01-tree-children, States.

use bolt::controls::{Button, Input, InputEnter, InputState};
use bolt::modals::{Modal, modal_field, modal_frame};
use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Fields, ModalMetrics, Spacing};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Point, Role, Styled, Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::model;
use crate::shell::request::{size_text, ui};
use crate::shell::tree_model::{self, example_node_id};
use crate::shell::{Shell, region, text_region, toasts};
use crate::tokens::method_hue;
use crate::workspace::OpenedWorkspace;

/// The dialog title.
pub(crate) const TITLE: &str = "Save as example";

/// What the open dialog saves.
pub(crate) struct SaveDialog {
    /// The id of the request the response belongs to.
    request: String,
    request_name: String,
    method: String,
    status: u16,
    /// `application/json · 214 B · 5 headers`.
    summary: String,
    input: Entity<InputState>,
}

impl Shell {
    /// Save as example: opens the dialog for the active request's last response, the name
    /// starting as its reason phrase, all selected. Does nothing without a response.
    pub(crate) fn open_save_example(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.active_request().map(str::to_owned) else {
            return;
        };
        let Some((response, _)) = self.received(&id) else {
            return;
        };
        let (request_name, method) = match tree_model::find(&self.roots, &id) {
            Some(node) => match &node.kind {
                tree_model::NodeKind::Request { method } => (node.name.clone(), method.clone()),
                _ => (node.name.clone(), String::new()),
            },
            None => (String::new(), String::new()),
        };
        let content_type = model::content_type(
            response
                .headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );
        let bytes = u64::try_from(response.body.len()).unwrap_or(u64::MAX);
        let summary = [
            Some(content_type).filter(|kind| !kind.is_empty()),
            Some(size_text(bytes)),
            Some(toasts::counted(response.headers.len(), "header")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
        let name = model::default_name(response.status, response.reason.as_deref());
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(name, window, cx);
            state.select_all(window, cx);
        });
        let field = input.read(cx).focus_handle(cx);
        self.examples.focus.focus(Some(&field), window, cx);
        self.examples.dialog = Some(SaveDialog {
            request: id,
            request_name,
            method,
            status: response.status,
            summary,
            input,
        });
        cx.notify();
    }

    /// Save example: saves the response under the name typed, closes the dialog, confirms
    /// it in a toast and selects the new leaf in the tree without opening it. A failure keeps
    /// the dialog open and shows an error toast.
    pub(crate) fn confirm_save_example(&mut self, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(dialog) = self.examples.dialog.as_ref() else {
            return;
        };
        let request = dialog.request.clone();
        let name = dialog.input.read(cx).value().trim().to_owned();
        let request_name = dialog.request_name.clone();
        let (Some(written), Some((response, masker))) =
            (self.written_request(&request), self.received(&request))
        else {
            return;
        };
        match engine.save_example(&request, &name, &written, &response, &masker) {
            Ok(example) => {
                let masked = model::masked_count(&response, |text| masker.mask(text));
                let node = example_node_id(&request, &example.to_string());
                self.examples.dialog = None;
                self.examples.focus.restore(cx);
                self.refresh_examples(Some(node.clone()), cx);
                let saved =
                    tree_model::find(&self.roots, &node).map_or(name, |leaf| leaf.name.clone());
                toasts::success(
                    model::saved_title(&saved),
                    model::saved_subtitle(&request_name, masked),
                    cx,
                );
            }
            Err(error) => toasts::failure("Could not save the example", error.to_string(), cx),
        }
        cx.notify();
    }

    /// Cancel, Escape or the backdrop: closes the dialog without saving.
    pub(crate) fn close_save_example(&mut self, cx: &mut Context<Self>) {
        if self.examples.dialog.take().is_some() {
            self.examples.focus.restore(cx);
        }
        cx.notify();
    }

    /// The name in the open dialog, for tests.
    #[cfg(test)]
    pub(crate) fn save_example_name(&self, cx: &gpui::App) -> Option<String> {
        self.examples
            .dialog
            .as_ref()
            .map(|dialog| dialog.input.read(cx).value().to_string())
    }
}

/// The dialog, while it is open, over the window.
pub(crate) fn render(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    shell.examples.focus.apply_pending(window, cx);
    let dialog = shell.examples.dialog.as_ref()?;
    let colors = bolt::colors(cx);
    let status_color = crate::shell::sidebar_history::class_color(
        crate::shell::request::StatusClass::of(dialog.status),
        &colors,
    );
    let header = ui::role(
        text_region("example-save-request"),
        TextVariant::Caption,
        cx,
    )
    .aria_label(format!("{} {}", dialog.method, dialog.request_name))
    .flex()
    .items_center()
    .gap(Spacing::SM)
    .child(
        div()
            .font_family(bolt::fonts::editor_family(cx))
            .font_weight(FontWeight::BOLD)
            .text_color(method_hue(&dialog.method, cx))
            .child(dialog.method.clone()),
    )
    .child(dialog.request_name.clone());
    let field = region("example-save-name")
        .role(Role::TextInput)
        .on_action(cx.listener(|this, _: &InputEnter, _window, cx| {
            this.confirm_save_example(cx);
        }))
        .child(Input::new(&dialog.input));
    let summary = modal_frame(cx).child(
        region("example-save-summary")
            .role(Role::Label)
            .aria_label(format!("{} {}", dialog.status, dialog.summary))
            .flex()
            .flex_col()
            .gap(Spacing::XS)
            .p(Spacing::MD)
            .text_color(colors.muted)
            .child(
                div()
                    .flex()
                    .gap(Spacing::SM)
                    .child(
                        div()
                            .font_family(bolt::fonts::editor_family(cx))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_color)
                            .child(dialog.status.to_string()),
                    )
                    .child(dialog.summary.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::SM)
                    .child(
                        Icon::new(ui::icon(IconName::Shield))
                            .size(Fields::CHEVRON)
                            .color(colors.muted),
                    )
                    .child(
                        "The request is kept as written. Secrets in the response are masked \
                         before it is written.",
                    ),
            ),
    );
    let hint = "Saved next to the request. A taken name gets a number.";
    let body = div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::BODY_GAP)
        .child(modal_field("Name", field, cx))
        .child(summary)
        .child(
            ui::role(text_region("example-save-hint"), TextVariant::Caption, cx)
                .aria_label(hint)
                .child(hint),
        );
    let footer = div()
        .flex()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            Button::new("example-save-cancel", "Cancel")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| this.close_save_example(cx))),
        )
        .child(
            Button::new("example-save-confirm", "Save example")
                .primary()
                .icon(ui::icon(IconName::Check))
                .kbd("\u{21b5}")
                .on_click(cx.listener(|this, _event, _window, cx| this.confirm_save_example(cx))),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(TITLE)
        .id("example-save-backdrop")
        .icon(ui::icon(IconName::Bookmark))
        .header_extra(header)
        .focus_handle(shell.examples.focus.handle())
        .body(
            region("example-save-dialog")
                .role(Role::Dialog)
                .aria_label(TITLE)
                .child(body),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_save_example(cx)) {
                log::warn!("save as example close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.confirm_save_example(cx)) {
                log::warn!("save as example: {error}");
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
