//! Export… (v1-close S4, design/export/01-one-export-dialog): one `bolt` modal that exports
//! what it opened on as Postman v2.1, a ReqFlux bundle or API docs, through the engine's
//! `export_postman`, `bundle` and `docs`, the ones `reqflux export|bundle|docs` run.
//!
//! - It opens from a tree row's `Export…` (`tree-menu-export`), the workspace menu's
//!   `Export workspace…` (`workspace-export`, bundle only), an environment's `Export…`
//!   (`variables-menu-export`, Postman only) and the palette's `export` (the selected tree
//!   row, else the active request) and `export-workspace`. The title names the scope.
//! - The primary button checks the passphrase of an encrypted bundle (empty, then mismatch,
//!   both shown under their field), makes the document, closes the dialog and asks the
//!   [`ExportHost`] for a file or, for API docs, a folder. A dismissed OS dialog writes
//!   nothing.
//! - A file is written by [`reqflux_engine::write_export_file`] with force: the OS save
//!   dialog has already asked before replacing a file. It is owner-only when Postman carries
//!   secret values in clear. API docs that exist ask once
//!   (`export-replace-dialog`); Replace files writes with force.
//! - The success toast names the path and offers Show in folder (Open index.html for docs);
//!   a failure keeps the error and offers Try again….
//!
//! The passphrase lives in the dialog's two masked inputs and in the call that encrypts; it
//! never reaches a log, a toast, an error or the automation tree (the fields stay secret while
//! Show draws their text), and closing the dialog drops it.

mod model;
mod view;

#[cfg(test)]
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bolt::controls::InputState;
use bolt::modals::ModalFocus;
use bolt::toast::ToastAction;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, PathPromptOptions, Window};
use reqflux_core::model::Id;
use reqflux_engine::{ApiDocs, BundleSecrets, DocsError, Engine, write_export_file};

use super::Shell;
use super::sidebar::Sidebar;
use super::toasts::{self, counted};
use super::tree_model::{self, NodeKind};
use super::workspace_picker::{Picked, picked_path};
use crate::workspace::OpenedWorkspace;
use model::{Format, PassphraseError, Scope, check_passphrase, listed, suggested_name};

pub(crate) use view::render;

/// The tree menu's `Export…` on the row with this id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportFor(pub(crate) String);

impl EventEmitter<ExportFor> for Sidebar {}

/// The OS dialogs and file manager an export uses; tests answer them.
pub(crate) trait ExportHost {
    /// A new file to write, the dialog suggesting `suggested`.
    fn save_path(&self, suggested: &str, cx: &App) -> Picked;
    /// A folder to write API docs into.
    fn pick_folder(&self, cx: &App) -> Picked;
    /// Shows `path` in the OS file manager.
    fn reveal(&self, path: &Path, cx: &App);
    /// Opens `path` with its default application.
    fn open(&self, path: &Path, cx: &App);
}

/// The platform's dialogs and file manager.
pub(crate) struct OsExportHost;

impl ExportHost for OsExportHost {
    fn save_path(&self, suggested: &str, cx: &App) -> Picked {
        let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let answer = cx.prompt_for_new_path(&start, Some(suggested));
        Box::pin(async move {
            match answer.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            }
        })
    }

    fn pick_folder(&self, cx: &App) -> Picked {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose folder".into()),
        });
        Box::pin(async move {
            match answer.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            }
        })
    }

    fn reveal(&self, path: &Path, cx: &App) {
        cx.reveal_path(path);
    }

    fn open(&self, path: &Path, cx: &App) {
        cx.open_with_system(path);
    }
}

/// Answers the save dialog with `path` and the folder dialog with `folder`, `None` meaning
/// dismissed, and records what was suggested, revealed and opened.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestExportHost {
    pub(crate) path: RefCell<Option<PathBuf>>,
    pub(crate) folder: RefCell<Option<PathBuf>>,
    pub(crate) suggested: RefCell<Vec<String>>,
    pub(crate) revealed: RefCell<Vec<PathBuf>>,
    pub(crate) opened: RefCell<Vec<PathBuf>>,
}

#[cfg(test)]
impl ExportHost for TestExportHost {
    fn save_path(&self, suggested: &str, _cx: &App) -> Picked {
        self.suggested.borrow_mut().push(suggested.to_owned());
        let answer = self.path.borrow().clone();
        Box::pin(async move { Ok(answer) })
    }

    fn pick_folder(&self, _cx: &App) -> Picked {
        let answer = self.folder.borrow().clone();
        Box::pin(async move { Ok(answer) })
    }

    fn reveal(&self, path: &Path, _cx: &App) {
        self.revealed.borrow_mut().push(path.to_owned());
    }

    fn open(&self, path: &Path, _cx: &App) {
        self.opened.borrow_mut().push(path.to_owned());
    }
}

/// The export of a window: its OS dialogs, the dialog and the Replace API docs question.
pub(super) struct Export {
    pub(super) host: Rc<dyn ExportHost>,
    focus: ModalFocus,
    dialog: Option<Dialog>,
    replace: Option<ReplaceDocs>,
    /// The scope of the last export, for Try again….
    last: Option<Scope>,
}

impl Export {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            host: Rc::new(OsExportHost),
            focus: ModalFocus::new(cx),
            dialog: None,
            replace: None,
            last: None,
        }
    }
}

/// The open dialog.
struct Dialog {
    scope: Scope,
    format: Format,
    /// Postman: write secret values in clear.
    include_secrets: bool,
    /// Bundle: encrypt the secrets rather than leave them out.
    encrypt: bool,
    /// Whether the passphrase fields show their text.
    reveal: bool,
    /// The secret variables Postman writes empty without Include secret values.
    postman_secrets: Vec<String>,
    /// The secret variables a bundle leaves out without a passphrase.
    bundle_secrets: Vec<String>,
    /// The passphrase and its confirmation, made on the first frame that shows them.
    passphrase: Option<[Entity<InputState>; 2]>,
    error: Option<PassphraseError>,
}

/// API docs that would overwrite files in `dir`, waiting for Replace files.
struct ReplaceDocs {
    docs: ApiDocs,
    dir: PathBuf,
}

/// What the primary button made, waiting for a path.
enum Output {
    File {
        text: String,
        /// Owner-only: it holds secret values in clear.
        private: bool,
        suggested: String,
        done: Done,
    },
    Docs(ApiDocs),
}

/// The toast a written file gets.
struct Done {
    title: String,
    subtitle: String,
    /// What to know besides the path.
    note: Option<String>,
    warn: bool,
}

/// `a`, `a and b`, with the verb that agrees: `is`/`are`.
fn agree(names: &[String], one: &'static str, many: &'static str) -> &'static str {
    if names.len() == 1 { one } else { many }
}

impl Shell {
    /// Opens the dialog on the tree node `id`; an example exports its request.
    pub(super) fn open_export_for(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(node) = tree_model::find(&self.roots, id) else {
            return;
        };
        let scope = match &node.kind {
            NodeKind::Collection => Scope::Collection {
                id: node.id.clone(),
                name: node.name.clone(),
            },
            NodeKind::Folder => Scope::Folder {
                id: node.id.clone(),
                name: node.name.clone(),
            },
            NodeKind::Request { .. } => Scope::Request {
                id: node.id.clone(),
                name: node.name.clone(),
            },
            NodeKind::Example { request, .. } => {
                let request = request.clone();
                return self.open_export_for(&request, cx);
            }
        };
        self.open_export(scope, cx);
    }

    /// The palette's Export…: the selected tree row, else the active request.
    pub(super) fn open_export_for_selection(&mut self, cx: &mut Context<Self>) {
        let target = self
            .sidebar
            .read(cx)
            .selected()
            .map(str::to_owned)
            .or_else(|| self.active_request().map(str::to_owned));
        if let Some(target) = target {
            self.open_export_for(&target, cx);
        }
    }

    /// Export workspace…: the whole workspace as a bundle.
    pub(super) fn open_export_workspace(&mut self, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let name = engine.workspace().manifest.name.clone();
        self.open_export(Scope::Workspace { name }, cx);
    }

    /// An environment's Export…: a Postman environment.
    pub(super) fn open_export_environment(&mut self, id: Id, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(node) = engine
            .workspace()
            .environments
            .iter()
            .find(|node| node.environment.id == id)
        else {
            return;
        };
        let scope = Scope::Environment {
            id: id.to_string(),
            name: node.environment.name.clone(),
        };
        self.open_export(scope, cx);
    }

    /// Opens the dialog on `scope`, on its first format, secrets left out.
    fn open_export(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let (postman_secrets, bundle_secrets) = secret_names(engine, &scope);
        self.export.dialog = Some(Dialog {
            format: scope.default_format(),
            scope,
            include_secrets: false,
            encrypt: false,
            reveal: false,
            postman_secrets,
            bundle_secrets,
            passphrase: None,
            error: None,
        });
        self.export.focus.focus_on_next_render();
        cx.notify();
    }

    fn set_export_format(&mut self, format: Format, cx: &mut Context<Self>) {
        if let Some(dialog) = self.export.dialog.as_mut()
            && dialog.scope.formats().contains(&format)
        {
            dialog.format = format;
            dialog.error = None;
            cx.notify();
        }
    }

    fn set_include_secrets(&mut self, include: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = self.export.dialog.as_mut() {
            dialog.include_secrets = include;
            cx.notify();
        }
    }

    fn set_encrypt(&mut self, encrypt: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = self.export.dialog.as_mut() {
            dialog.encrypt = encrypt;
            dialog.error = None;
            cx.notify();
        }
    }

    /// The eye button: shows or masks both passphrase fields.
    fn toggle_passphrase_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.export.dialog.as_mut() else {
            return;
        };
        dialog.reveal = !dialog.reveal;
        let masked = !dialog.reveal;
        for input in dialog.passphrase.iter().flatten() {
            input.update(cx, |state, cx| state.set_masked(masked, window, cx));
        }
        cx.notify();
    }

    /// Makes the passphrase fields the first frame that shows them.
    fn ensure_passphrase_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.export.dialog.as_mut() else {
            return;
        };
        if dialog.format == Format::Bundle && dialog.encrypt && dialog.passphrase.is_none() {
            let mut field = |cx: &mut Context<Self>| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .masked(true)
                        .placeholder("Passphrase")
                })
            };
            dialog.passphrase = Some([field(cx), field(cx)]);
        }
    }

    /// Cancel, Escape or the backdrop: closes the dialog and drops the passphrase.
    fn close_export(&mut self, cx: &mut Context<Self>) {
        if self.export.dialog.take().is_some() {
            self.export.focus.restore(cx);
        }
        cx.notify();
    }

    /// The primary button: checks the passphrase, makes the document, closes the dialog and
    /// asks where to write it.
    fn confirm_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(dialog) = self.export.dialog.as_mut() else {
            return;
        };
        let passphrase = match (&dialog.format, dialog.encrypt, &dialog.passphrase) {
            (Format::Bundle, true, Some([first, second])) => {
                let (first, second) = (first.read(cx).value(), second.read(cx).value());
                if let Err(error) = check_passphrase(&first, &second) {
                    dialog.error = Some(error);
                    cx.notify();
                    return;
                }
                Some(first)
            }
            (Format::Bundle, true, None) => {
                dialog.error = Some(PassphraseError::Empty);
                cx.notify();
                return;
            }
            _ => None,
        };
        let scope = dialog.scope.clone();
        let made = make_output(
            engine,
            &scope,
            dialog.format,
            dialog.include_secrets,
            passphrase.as_deref(),
            &self.roots,
        );
        self.close_export(cx);
        self.export.last = Some(scope);
        match made {
            Ok(output) => self.ask_where(output, window, cx),
            Err(error) => self.export_failed(error, cx),
        }
    }

    /// Asks the OS for a file or a folder for `output`, then writes it there.
    fn ask_where(&mut self, output: Output, window: &mut Window, cx: &mut Context<Self>) {
        let picked = match &output {
            Output::File { suggested, .. } => self.export.host.save_path(suggested, cx),
            Output::Docs(_) => self.export.host.pick_folder(cx),
        };
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = picked_path(picked.await, "Cannot open the save dialog", cx) else {
                return;
            };
            if let Err(error) = this.update(cx, |shell, cx| shell.write_output(output, &path, cx)) {
                log::debug!("the export path was picked after the shell closed: {error}");
            }
        })
        .detach();
    }

    /// Writes `output` to `path` and says how it went.
    fn write_output(&mut self, output: Output, path: &Path, cx: &mut Context<Self>) {
        match output {
            Output::File {
                text,
                private,
                done,
                ..
            } => match write_export_file(path, &text, true, private) {
                Ok(()) => self.exported(done, path, cx),
                Err(error) => self.export_failed(error.to_string(), cx),
            },
            Output::Docs(docs) => self.write_docs(docs, path, false, cx),
        }
    }

    /// Writes API docs into `dir`; files that exist ask Replace files unless `force`.
    fn write_docs(&mut self, docs: ApiDocs, dir: &Path, force: bool, cx: &mut Context<Self>) {
        match docs.write(dir, force) {
            Ok(_) => {
                let index = dir.join("index.html");
                let weak = cx.entity().downgrade();
                let action =
                    ToastAction::new("export-open-index", "Open index.html").on_click(move |cx| {
                        if let Err(error) =
                            weak.update(cx, |this, cx| this.export.host.open(&index, cx))
                        {
                            log::warn!("open index.html: {error}");
                        }
                    });
                let names = docs
                    .files
                    .iter()
                    .map(|file| file.name.clone())
                    .collect::<Vec<_>>();
                toasts::with_action(
                    "API docs written",
                    listed(&names),
                    dir.display().to_string(),
                    false,
                    action,
                    cx,
                );
            }
            Err(DocsError::Exists { .. }) if !force => {
                self.export.replace = Some(ReplaceDocs {
                    docs,
                    dir: dir.to_owned(),
                });
                self.export.focus.focus_on_next_render();
                cx.notify();
            }
            Err(error) => self.export_failed(error.to_string(), cx),
        }
    }

    /// Replace files: writes the waiting docs over the existing files.
    fn replace_docs(&mut self, cx: &mut Context<Self>) {
        if let Some(replace) = self.close_replace(cx) {
            self.write_docs(replace.docs, &replace.dir, true, cx);
        }
    }

    /// Choose another folder…: asks for a folder again for the waiting docs.
    fn browse_docs_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(replace) = self.close_replace(cx) {
            self.ask_where(Output::Docs(replace.docs), window, cx);
        }
    }

    /// Closes the Replace API docs question, giving back what it held.
    fn close_replace(&mut self, cx: &mut Context<Self>) -> Option<ReplaceDocs> {
        let replace = self.export.replace.take();
        if replace.is_some() {
            self.export.focus.restore(cx);
        }
        cx.notify();
        replace
    }

    /// The success toast of a written file, with Show in folder.
    fn exported(&mut self, done: Done, path: &Path, cx: &mut Context<Self>) {
        let shown = path.to_owned();
        let weak = cx.entity().downgrade();
        let action =
            ToastAction::new("export-show-in-folder", "Show in folder").on_click(move |cx| {
                if let Err(error) = weak.update(cx, |this, cx| this.export.host.reveal(&shown, cx))
                {
                    log::warn!("show in folder: {error}");
                }
            });
        let details = match done.note {
            Some(note) => format!("{}\n{note}", path.display()),
            None => path.display().to_string(),
        };
        toasts::with_action(done.title, done.subtitle, details, done.warn, action, cx);
    }

    /// The failure toast, with Try again… on the same scope.
    fn export_failed(&mut self, error: String, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        let action = ToastAction::new("export-try-again", "Try again…")
            .dismisses()
            .on_click(move |cx| {
                if let Err(error) = weak.update(cx, |this, cx| {
                    if let Some(scope) = this.export.last.clone() {
                        this.open_export(scope, cx);
                    }
                }) {
                    log::warn!("try the export again: {error}");
                }
            });
        toasts::failure_with_action("Export failed", error, action, cx);
    }
}

/// The secret variables of `scope`, by name: those Postman writes empty and those a bundle
/// leaves out, as the engine reports them.
fn secret_names(engine: &Engine, scope: &Scope) -> (Vec<String>, Vec<String>) {
    let postman = match scope.selector() {
        Some(selector) if scope.formats().contains(&Format::Postman) => engine
            .export_postman(selector, false)
            .map(|export| export.empty_secrets)
            .inspect_err(|error| log::warn!("cannot list the secrets of {selector}: {error}"))
            .unwrap_or_default(),
        Some(_) | None => Vec::new(),
    };
    let bundle = if scope.formats().contains(&Format::Bundle) {
        engine
            .bundle(scope.selector(), BundleSecrets::Omit)
            .map(|bundle| {
                bundle
                    .secrets_omitted
                    .into_iter()
                    .map(|(_, key)| key)
                    .collect()
            })
            .inspect_err(|error| log::warn!("cannot list the secrets to bundle: {error}"))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    (postman, bundle)
}

/// Makes the document of `scope` as `format`, with its toast; the error text otherwise.
fn make_output(
    engine: &Engine,
    scope: &Scope,
    format: Format,
    include_secrets: bool,
    passphrase: Option<&str>,
    roots: &[tree_model::Node],
) -> Result<Output, String> {
    let name = scope.name().to_owned();
    match format {
        Format::Postman => {
            let selector = scope.selector().unwrap_or_default();
            let export = engine
                .export_postman(selector, include_secrets)
                .map_err(|error| error.to_string())?;
            let text = export.text().map_err(|error| error.to_string())?;
            let empty = &export.empty_secrets;
            let done = if include_secrets && !empty.is_empty() {
                Done {
                    title: "Exported with empty secrets".to_owned(),
                    subtitle: format!("{name} · {} had no value", counted(empty.len(), "secret")),
                    note: Some(format!(
                        "{} {} empty in this workspace and {} written empty",
                        listed(empty),
                        agree(empty, "is", "are"),
                        agree(empty, "was", "were")
                    )),
                    warn: true,
                }
            } else if let Scope::Environment { id, .. } = scope {
                let variables = engine
                    .workspace()
                    .environments
                    .iter()
                    .find(|node| node.environment.id.to_string() == *id)
                    .map_or(0, |node| node.environment.variables.len());
                Done {
                    title: "Environment exported".to_owned(),
                    subtitle: format!("{name} · {}", counted(variables, "variable")),
                    note: None,
                    warn: false,
                }
            } else {
                let requests =
                    tree_model::find(roots, selector).map_or(0, |node| node.request_ids().len());
                Done {
                    title: "Exported to Postman".to_owned(),
                    subtitle: format!("{name} · {}", counted(requests, "request")),
                    note: None,
                    warn: false,
                }
            };
            Ok(Output::File {
                text,
                private: include_secrets,
                suggested: suggested_name(scope, format),
                done,
            })
        }
        Format::Bundle => {
            let secrets = match passphrase {
                Some(passphrase) => BundleSecrets::Encrypt { passphrase },
                None => BundleSecrets::Omit,
            };
            let bundle = engine
                .bundle(scope.selector(), secrets)
                .map_err(|error| error.to_string())?;
            let omitted = bundle
                .secrets_omitted
                .iter()
                .map(|(_, key)| key.clone())
                .collect::<Vec<_>>();
            let note = (!omitted.is_empty()).then(|| omitted.join(", "));
            let done = match (passphrase, omitted.is_empty()) {
                (Some(_), _) => Done {
                    title: "Bundle saved, secrets encrypted".to_owned(),
                    subtitle: format!(
                        "{name} · {} sealed with your passphrase",
                        counted(bundle.secrets_included, "secret")
                    ),
                    warn: note.is_some(),
                    note,
                },
                (None, true) => Done {
                    title: "Bundle saved".to_owned(),
                    subtitle: name,
                    note: None,
                    warn: false,
                },
                (None, false) => Done {
                    title: "Bundle saved without secrets".to_owned(),
                    subtitle: format!("{name} · {} left out", counted(omitted.len(), "secret")),
                    note,
                    warn: true,
                },
            };
            Ok(Output::File {
                text: bundle.text,
                private: false,
                suggested: suggested_name(scope, format),
                done,
            })
        }
        Format::Docs => {
            let selector = scope.selector().unwrap_or_default();
            engine
                .docs(selector)
                .map(Output::Docs)
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "export_bundle_tests.rs"]
mod bundle_tests;
