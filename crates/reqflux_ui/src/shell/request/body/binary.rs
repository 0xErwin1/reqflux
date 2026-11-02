//! The binary body editing (v1-features S7, design/layouts/01 D4, D5): the file chosen
//! with the OS dialog shows its path in a read-only well beside the Choose button, with a
//! Clear button once a path is set, and the empty well is the no-file state. Only the path
//! lands in the draft as `[body] path`; the engine reads and sends the file itself. The
//! dialog is the [`BodyFilePicker`] seam: the app asks the platform, tests answer with
//! preset paths, since no test can drive an OS dialog.

use std::path::PathBuf;
use std::rc::Rc;

use bolt::controls::Button;
use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Fields, Spacing};
use gpui::{
    AnyElement, App, Context, Global, PathPromptOptions, Role, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::Body;

use super::super::ui;
use super::controls::well;
use crate::shell::rows::part;
use crate::shell::workspace_picker::{Picked, picked_path};
use crate::shell::{Shell, region};

/// Asks the user for the file a binary body sends.
pub(in crate::shell) trait BodyFilePicker {
    /// An existing file to send as the body.
    fn pick_file(&self, cx: &App) -> Picked;
}

/// The platform's file dialog.
pub(in crate::shell) struct OsBodyFilePicker;

impl BodyFilePicker for OsBodyFilePicker {
    fn pick_file(&self, cx: &App) -> Picked {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose the file to send".into()),
        });
        Box::pin(async move {
            match answer.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            }
        })
    }
}

struct BodyFileDialog(Rc<dyn BodyFilePicker>);

impl Global for BodyFileDialog {}

/// The installed file dialog, the OS one unless a test replaced it.
fn picker(cx: &App) -> Rc<dyn BodyFilePicker> {
    cx.try_global::<BodyFileDialog>()
        .map(|dialog| Rc::clone(&dialog.0))
        .unwrap_or_else(|| Rc::new(OsBodyFilePicker))
}

/// Answers the body's file dialog with `file`, `None` meaning dismissed.
#[cfg(test)]
pub(crate) fn answer_body_file_dialog(file: Option<PathBuf>, cx: &mut App) {
    struct Preset(Option<PathBuf>);

    impl BodyFilePicker for Preset {
        fn pick_file(&self, _cx: &App) -> Picked {
            let file = self.0.clone();
            Box::pin(async move { Ok(file) })
        }
    }

    cx.set_global(BodyFileDialog(Rc::new(Preset(file))));
}

/// The no-file state the well shows while the path is empty.
const NO_FILE: &str = "No file";

/// The binary body of the request `request_id`: the read-only path well, the Choose
/// button and, once a path is set, the Clear one, over the note about where it is stored.
pub(super) fn render(request_id: &str, path: &str, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let empty = path.is_empty();
    let shown = if empty { NO_FILE } else { path };
    let choose_id = request_id.to_owned();
    let clear_id = request_id.to_owned();
    let row = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            well("request-body-file".to_owned(), cx)
                .role(Role::Label)
                .aria_label(shown.to_owned())
                .flex_1()
                .min_w_0()
                .text_color(if empty { colors.muted } else { colors.strong })
                .child(
                    Icon::new(ui::icon(IconName::File))
                        .size(Fields::LEADING_ICON)
                        .color(colors.muted),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(shown.to_owned()),
                )
                .when(!empty, |field| {
                    field.child(div().flex_1()).child(
                        ui::role(
                            part("request-body-file-readonly".to_owned()),
                            TextVariant::Caption,
                            cx,
                        )
                        .child("read-only"),
                    )
                }),
        )
        .child(
            Button::new("request-body-choose", "Choose file…")
                .secondary()
                .icon(ui::icon(IconName::FolderOpen))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.choose_binary_file(&choose_id, window, cx);
                })),
        )
        .children((!empty).then(|| {
            Button::new("request-body-clear", "Clear")
                .ghost()
                .inline()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.clear_binary_file(&clear_id, cx);
                }))
        }));
    region("request-body-binary")
        .flex()
        .flex_col()
        .gap(Spacing::XS)
        .mx(Spacing::MD)
        .mt(Spacing::SM)
        .child(row)
        .child(
            ui::role(
                part("request-body-binary-hint".to_owned()),
                TextVariant::Caption,
                cx,
            )
            .child("Stored as [body] path in the request; the file itself never lands in the workspace."),
        )
        .into_any_element()
}

/// The body the Body pane of the request `id` shows: the draft's, else the file's.
pub(super) fn displayed_body(shell: &Shell, id: &str) -> Option<Body> {
    let state = shell.requests.get(id)?;
    Some(
        shell
            .displayed_request(id, &state.inspection.request.request)
            .body
            .clone(),
    )
}

impl Shell {
    /// Asks for the file of the binary body of the request `id` and puts its path into the
    /// draft. A dismissed dialog changes nothing; a failed one says so in a toast.
    pub(super) fn choose_binary_file(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(displayed_body(self, id), Some(Body::Binary { .. })) {
            return;
        }
        let picked = picker(cx).pick_file(cx);
        let request = id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = picked_path(picked.await, "Cannot open the file dialog", cx) else {
                return;
            };
            if let Err(error) = this.update(cx, |shell, cx| {
                shell.set_draft_binary_path(&request, path, cx)
            }) {
                log::debug!("the file was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Puts `path` into the binary body of the request `id`'s draft. The path the request
    /// already has, without a draft, creates none.
    fn set_draft_binary_path(&mut self, id: &str, path: PathBuf, cx: &mut Context<Self>) {
        let path = path.display().to_string();
        if !self.drafts.contains_key(id)
            && matches!(displayed_body(self, id), Some(Body::Binary { path: current }) if current == path)
        {
            return;
        }
        self.edit_draft(
            id,
            |request| {
                if let Body::Binary { path: current } = &mut request.body {
                    *current = path;
                }
            },
            cx,
        );
    }

    /// Empties the binary path of the request `id`: the well is the no-file state again.
    /// The already empty path, without a draft, creates none.
    pub(super) fn clear_binary_file(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.drafts.contains_key(id)
            && matches!(displayed_body(self, id), Some(Body::Binary { path }) if path.is_empty())
        {
            return;
        }
        self.edit_draft(
            id,
            |request| {
                if let Body::Binary { path } = &mut request.body {
                    path.clear();
                }
            },
            cx,
        );
    }
}
