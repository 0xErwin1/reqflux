//! `Import Postman collection…` (app-pending S6, design/bolt-v2/11-import-postman): `bolt`'s
//! modal over the open workspace. It asks for a collection file through the [`ImportPicker`]
//! seam, reads it and shows what the engine's importer (the one `reqflux import` runs) would
//! write, from [`reqflux_engine::postman_preview`]: the file, `import-file`, with the
//! choose-again button `import-browse` and its size, `import-detected`; the counts,
//! `import-count-<kind>`; the files, `import-file-<n>`, under `import-files-count`; or why it
//! cannot import, `import-error`. `Import` (`import-confirm`, Enter) writes it into the open
//! workspace and reloads it; `Cancel` (`import-cancel`), Escape or the backdrop closes the
//! dialog and writes nothing.
//!
//! The artboard's stepper is a `bolt` gap; its environments, warnings and per-request
//! methods are not drawn because the importer of a single collection file does not
//! produce them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bolt::controls::Button;
use bolt::modals::{Modal, ModalFocus, modal_frame, modal_value_field};
use bolt::primitives::{Badge, BadgeTone, BannerBlock, BannerVariant, Icon, TextVariant};
use bolt::tokens::{MenuMetrics, ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, IntoElement, ParentElement, PathPromptOptions, Point, Role, Styled,
    Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::{ImportError, ImportPreview};
use serde_json::Value;

use super::request::ui;
use super::rows::{labeled_row, part};
use super::toasts;
use super::workspace_picker::{Picked, picked_path};
use super::{Shell, region, text_region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// The dialog title and the workspace menu entry without its ellipsis.
pub(crate) const TITLE: &str = "Import Postman collection";

/// The note at the left of the footer.
pub(crate) const NOTHING_WRITTEN: &str = "Nothing is written until you import";

/// The file field before a file is chosen.
const FILE_PLACEHOLDER: &str = "Choose a Postman collection (.json)";

/// What the badge says once the importer reads the file.
const DETECTED: &str = "Postman collection";

/// Asks the user for the collection file.
pub(crate) trait ImportPicker {
    /// An existing file to import.
    fn pick_file(&self, cx: &App) -> Picked;
}

/// The platform's file dialog.
pub(crate) struct OsImportPicker;

impl ImportPicker for OsImportPicker {
    fn pick_file(&self, cx: &App) -> Picked {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(TITLE.into()),
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

/// Answers the file dialog with a preset path, `None` meaning dismissed, or with `failure`,
/// an OS dialog error, when it is set.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestImportPicker {
    pub(crate) file: Option<PathBuf>,
    pub(crate) failure: Option<String>,
}

#[cfg(test)]
impl ImportPicker for TestImportPicker {
    fn pick_file(&self, _cx: &App) -> Picked {
        let answer = self.failure.clone().map_or(Ok(self.file.clone()), Err);
        Box::pin(async move { answer })
    }
}

/// The import dialog of a window: its file dialog, its focus and, while open, its flow.
pub(super) struct Import {
    pub(super) picker: Rc<dyn ImportPicker>,
    focus: ModalFocus,
    flow: Option<Flow>,
}

impl Import {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            picker: Rc::new(OsImportPicker),
            focus: ModalFocus::new(cx),
            flow: None,
        }
    }
}

/// An open dialog: the chosen file and what the importer made of it.
struct Flow {
    file: Option<PathBuf>,
    state: State,
}

enum State {
    /// No file yet.
    Empty,
    /// The file maps and nothing it writes exists.
    Ready {
        document: Value,
        preview: ImportPreview,
        size: usize,
    },
    /// The file cannot be imported, or the import failed.
    Failed(Failure),
}

/// Why the file cannot be imported: a short title over the error.
struct Failure {
    title: &'static str,
    detail: String,
}

impl Failure {
    fn new(title: &'static str, detail: impl ToString) -> Self {
        Self {
            title,
            detail: detail.to_string(),
        }
    }

    fn of(error: &ImportError) -> Self {
        let title = match error {
            ImportError::Postman(_) => "This is not a Postman collection.",
            ImportError::Exists { .. } => "The workspace already has what this file holds.",
            ImportError::Store { .. } => "The workspace cannot be written.",
        };
        Self::new(title, error)
    }
}

/// Reads `file` and previews its import into `root`.
fn review(root: &Path, file: &Path) -> State {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => return State::Failed(Failure::new("The file cannot be read.", error)),
    };
    let document: Value = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(error) => return State::Failed(Failure::new("The file is not JSON.", error)),
    };
    match reqflux_engine::postman_preview(root, &document, &[]) {
        Ok(preview) => State::Ready {
            document,
            preview,
            size: bytes.len(),
        },
        Err(error) => State::Failed(Failure::of(&error)),
    }
}

/// `size` bytes as the artboard writes it: `86 KB`.
pub(super) fn size_label(size: usize) -> String {
    const KB: usize = 1024;
    match size {
        size if size < KB => format!("{size} B"),
        size if size < KB * KB => format!("{} KB", size.div_ceil(KB)),
        size => format!("{:.1} MB", size as f64 / (KB * KB) as f64),
    }
}

impl Shell {
    /// Opens the dialog over the loaded workspace and asks for the file.
    pub(super) fn open_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.workspace, OpenedWorkspace::Loaded(_)) {
            return;
        }
        self.import.flow = Some(Flow {
            file: None,
            state: State::Empty,
        });
        self.import.focus.focus(None, window, cx);
        self.browse_import(window, cx);
        cx.notify();
    }

    /// Asks for the collection file and reviews it; a dialog that fails shows an error toast.
    fn browse_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.import.picker.pick_file(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = picked_path(picked.await, "Cannot open the file dialog", cx) else {
                return;
            };
            if let Err(error) = this.update(cx, |shell, cx| shell.set_import_file(path, cx)) {
                log::debug!("the file was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Reviews `file` for the open dialog.
    fn set_import_file(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let state = review(engine.root(), &file);
        if let Some(flow) = self.import.flow.as_mut() {
            flow.file = Some(file);
            flow.state = state;
        }
        cx.notify();
    }

    /// Imports the reviewed file into the open workspace, reloads it, closes the dialog and
    /// confirms it in a toast with the counts and the workspace; a failure stays in the
    /// dialog and shows in an error toast.
    fn run_import(&mut self, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(State::Ready {
            document, preview, ..
        }) = self.import.flow.as_ref().map(|flow| &flow.state)
        else {
            return;
        };
        match reqflux_engine::postman(engine.root(), document, &[]) {
            Ok(summary) => {
                log::info!(
                    "imported {} into {}: {} folders, {} requests",
                    summary.collection,
                    summary.root.display(),
                    summary.folders,
                    summary.requests
                );
                let title = format!(
                    "Imported {} into {}",
                    toasts::counted(summary.requests, "request"),
                    engine.workspace().manifest.name
                );
                let subtitle = format!(
                    "{} · {}",
                    preview.collection,
                    toasts::counted(summary.folders, "folder")
                );
                self.close_import(cx);
                let reloaded = self.reload_workspace(cx);
                toasts::success(title, subtitle, cx);
                if !reloaded {
                    toasts::reload_failed("Imported", cx);
                }
            }
            Err(error) => {
                toasts::failure("Import failed", error.to_string(), cx);
                if let Some(flow) = self.import.flow.as_mut() {
                    flow.state = State::Failed(Failure::of(&error));
                }
                cx.notify();
            }
        }
    }

    /// Closes the dialog without writing.
    fn close_import(&mut self, cx: &mut Context<Self>) {
        if self.import.flow.take().is_some() {
            self.import.focus.restore(cx);
        }
        cx.notify();
    }
}

/// The dialog, while it is open, over the window.
pub(super) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let flow = shell.import.flow.as_ref()?;
    let ready = matches!(flow.state, State::Ready { .. });
    let into = shell
        .workspace
        .name()
        .map(|name| format!("into {name}"))
        .unwrap_or_default();
    let body = div()
        .flex()
        .gap(ModalMetrics::PADDING)
        .child(
            region("import-review")
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(ModalMetrics::BODY_GAP)
                .child(file_section(flow, cx))
                .children(counts_section(&flow.state, cx))
                .children(error_banner(&flow.state)),
        )
        .children(files_section(&flow.state, cx));
    let footer = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            ui::role(text_region("import-note"), TextVariant::Caption, cx)
                .aria_label(NOTHING_WRITTEN)
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(Icon::new(ui::icon(IconName::Info)).size(MenuMetrics::HEADER_ICON))
                .child(NOTHING_WRITTEN),
        )
        .child(div().flex_1())
        .child(
            Button::new("import-cancel", "Cancel")
                .secondary()
                .kbd("Esc")
                .on_click(cx.listener(|this, _event, _window, cx| this.close_import(cx))),
        )
        .child(
            Button::new("import-confirm", "Import")
                .primary()
                .icon(ui::icon(IconName::Import))
                .kbd("↵")
                .disabled(!ready)
                .on_click(cx.listener(|this, _event, _window, cx| this.run_import(cx))),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(TITLE)
        .id("import-backdrop")
        .icon(ui::icon(IconName::Import))
        .width(tokens::IMPORT_DIALOG_WIDTH)
        .header_extra(
            ui::role(text_region("import-into"), TextVariant::Caption, cx)
                .aria_label(into.clone())
                .child(into),
        )
        .focus_handle(shell.import.focus.handle())
        .body(
            region("import-dialog")
                .role(Role::Dialog)
                .aria_label(TITLE)
                .child(body),
        )
        .footer(footer)
        .confirm_enabled(ready)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_import(cx)) {
                log::warn!("import dialog close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.run_import(cx)) {
                log::warn!("import dialog confirm: {error}");
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

/// A section caption, in capitals.
fn caption(id: &'static str, text: String, cx: &App) -> impl IntoElement {
    ui::role(
        labeled_row(id.to_owned(), text.clone()),
        TextVariant::Label,
        cx,
    )
    .child(text.to_uppercase())
}

/// The file field with its choose-again button and, once read, what it is.
fn file_section(flow: &Flow, cx: &mut Context<Shell>) -> impl IntoElement {
    let path = flow.file.as_ref().map(|file| file.display().to_string());
    let field = region("import-file")
        .role(Role::Label)
        .aria_label(path.clone().unwrap_or_else(|| FILE_PLACEHOLDER.to_owned()))
        .flex_1()
        .min_w_0()
        .child(modal_value_field(
            path.map(Into::into),
            FILE_PLACEHOLDER,
            None,
            cx,
        ));
    let browse = Button::new("import-browse", "Choose another file")
        .secondary()
        .icon(ui::icon(IconName::FolderOpen))
        .icon_only()
        .on_click(cx.listener(|this, _event, window, cx| this.browse_import(window, cx)));
    let detected = match &flow.state {
        State::Ready { size, .. } => {
            let detail = format!("{DETECTED} · {}", size_label(*size));
            Some(
                ui::role(
                    labeled_row("import-detected".to_owned(), detail),
                    TextVariant::Caption,
                    cx,
                )
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(Badge::new(DETECTED, BadgeTone::Success).icon(ui::icon(IconName::Check)))
                .child(size_label(*size)),
            )
        }
        State::Empty | State::Failed(_) => None,
    };
    div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::FIELD_GAP)
        .child(caption("import-file-caption", "File".to_owned(), cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(field)
                .child(browse),
        )
        .children(detected)
}

/// What the import creates: the collection and its folders, requests and scripts.
fn counts_section(state: &State, cx: &App) -> Option<impl IntoElement> {
    let State::Ready { preview, .. } = state else {
        return None;
    };
    let rows = [
        ("collection", "Collection", 1),
        ("folders", "Folders", preview.folders),
        ("requests", "Requests", preview.requests),
        ("scripts", "Scripts", preview.scripts),
    ];
    let mut cells = div().flex();
    for (kind, label, count) in rows {
        cells = cells.child(
            labeled_row(format!("import-count-{kind}"), format!("{label} {count}"))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(Spacing::XS)
                .px(ModalMetrics::LIST_ROW_PADDING_X)
                .py(ModalMetrics::CODE_PADDING_Y)
                .child(ui::role(div(), TextVariant::Caption, cx).child(label))
                .child(ui::role(div(), TextVariant::Heading, cx).child(count.to_string())),
        );
    }
    let frame = modal_frame(cx).child(cells);
    Some(
        div()
            .flex()
            .flex_col()
            .gap(ModalMetrics::FIELD_GAP)
            .child(caption(
                "import-counts-caption",
                "What will be created".to_owned(),
                cx,
            ))
            .child(frame),
    )
}

/// Why the file does not import, as a danger banner.
fn error_banner(state: &State) -> Option<impl IntoElement> {
    let State::Failed(failure) = state else {
        return None;
    };
    Some(
        region("import-error")
            .role(Role::Label)
            .aria_label(format!("{} {}", failure.title, failure.detail))
            .child(
                BannerBlock::new(BannerVariant::Danger, failure.title)
                    .with_body(failure.detail.clone()),
            ),
    )
}

/// Every file the import writes, under the count.
fn files_section(state: &State, cx: &App) -> Option<impl IntoElement> {
    let State::Ready { preview, .. } = state else {
        return None;
    };
    let count = match preview.files.len() {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    };
    let colors = bolt::colors(cx);
    let mut frame = modal_frame(cx);
    for (index, file) in preview.files.iter().enumerate() {
        let script = file.extension().is_some_and(|extension| extension == "js");
        let icon = if script {
            IconName::Code
        } else {
            IconName::File
        };
        let path = file.display().to_string();
        frame = frame.child(
            ui::role(
                part(format!("import-file-{}", index + 1)),
                TextVariant::Code,
                cx,
            )
            .role(Role::Label)
            .aria_label(path.clone())
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .h(ModalMetrics::DEPENDENT_ROW_HEIGHT)
            .px(ModalMetrics::LIST_ROW_PADDING_X)
            .child(div().text_color(colors.success).child("+"))
            .child(
                Icon::new(ui::icon(icon))
                    .size(MenuMetrics::HEADER_ICON)
                    .color(colors.muted),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(path),
            ),
        );
    }
    Some(
        region("import-files")
            .w(tokens::IMPORT_FILES_WIDTH)
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(ModalMetrics::FIELD_GAP)
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(caption("import-files-caption", "Files".to_owned(), cx))
                    .child(div().flex_1())
                    .child(
                        ui::role(
                            labeled_row("import-files-count".to_owned(), count.clone()),
                            TextVariant::Caption,
                            cx,
                        )
                        .child(count),
                    ),
            )
            .child(frame),
    )
}

#[cfg(test)]
#[path = "import_flow_tests.rs"]
mod tests;
