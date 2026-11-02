//! Generate code (v1-features S2, design/codegen/01-dialog-and-toast): `bolt`'s modal over the
//! window with the request as code, from the request line's `</>` button (`codegen-open`),
//! Ctrl+Shift+C, the palette and the tree menu's `Generate code…`, which opens it for its
//! request without switching tabs.
//!
//! The code is [`reqflux_engine::code`] of the request as the view shows it, the draft's when
//! it has one: every `{{variable}}` stays as written, secret or not, and nothing is read from
//! the keyring. The language is a segmented control, `codegen-language`, with the segments
//! `codegen-language-<curl|httpie|python|fetch>`; the code shows read only with line numbers in
//! `codegen-code`, one `codegen-line-<n>` per line; the note `codegen-secrets` names the secret
//! variables left in it. Copy (`codegen-copy`, Enter) puts the code on the clipboard, closes
//! the dialog and confirms it in a toast; Close (`codegen-close`), Escape or the backdrop
//! closes it.

use bolt::controls::Button;
use bolt::modals::{Modal, ModalFocus, modal_frame};
use bolt::primitives::{BannerBlock, BannerVariant, SegmentedItem, TextVariant};
use bolt::tokens::{EditorMetrics, GridMetrics, ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, ClipboardItem, Context, Div, EventEmitter, FontWeight, IntoElement,
    ParentElement, Point, Role, SharedString, Stateful, Styled, Window, anchored, deferred, div,
    prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::codegen::Language;
use reqflux_core::model::Item;
use reqflux_core::model::Request;
use reqflux_engine::{Engine, ExecuteError, Inspection, Span};

use super::request::{segmented, spans, ui};
use super::rows::{labeled_row, part};
use super::sidebar::Sidebar;
use super::{Shell, region, text_region, toasts, tree_model};
use crate::tokens::{self, method_hue};
use crate::workspace::OpenedWorkspace;

/// The dialog title and the tree menu entry without its ellipsis.
pub(crate) const TITLE: &str = "Generate code";

/// The tree menu's `Generate code…` on the request with this id: the shell opens the dialog
/// for it without switching tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenerateCodeFor(pub(crate) String);

impl EventEmitter<GenerateCodeFor> for Sidebar {}

/// The dialog of a window: its focus and, while open, what it shows.
pub(super) struct Codegen {
    focus: ModalFocus,
    open: Option<Dialog>,
}

impl Codegen {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            focus: ModalFocus::new(cx),
            open: None,
        }
    }
}

/// An open dialog: the request, the language and the code it shows.
struct Dialog {
    request_id: String,
    name: String,
    method: String,
    /// Whether the code comes from the draft rather than the saved request.
    from_draft: bool,
    language: Language,
    code: String,
    /// The secret variables the code leaves as `{{name}}`, in order of appearance.
    secrets: Vec<String>,
    /// The request inspected against the active environment, which styles the references.
    inspection: Option<Inspection>,
}

/// The name the dialog and the toast give `language`.
pub(crate) fn language_label(language: Language) -> &'static str {
    match language {
        Language::Curl => "curl",
        Language::Httpie => "HTTPie",
        Language::Python => "Python requests",
        Language::Fetch => "JavaScript fetch",
    }
}

/// The code of the request `id` in `language`, of `draft` when it has one: the draft takes
/// the request's place in a copy of the workspace, so its effective auth is inherited as the
/// saved request's is.
fn generate(
    engine: &Engine,
    id: &str,
    draft: Option<&Request>,
    language: Language,
) -> Result<String, ExecuteError> {
    let Some(draft) = draft else {
        return engine.code(id, language);
    };
    let mut workspace = engine.workspace().clone();
    let replaced = workspace
        .collections
        .iter_mut()
        .any(|collection| replace_request(&mut collection.items, id, draft));
    if !replaced {
        return Err(ExecuteError::UnknownRequest(id.to_owned()));
    }
    reqflux_engine::code(&workspace, id, language)
}

/// Puts `request` in place of the request `id` under `items`; whether it was found.
fn replace_request(items: &mut [Item], id: &str, request: &Request) -> bool {
    items.iter_mut().any(|item| match item {
        Item::Request(node) if node.request.id.to_string() == id => {
            node.request = request.clone();
            true
        }
        Item::Request(_) => false,
        Item::Folder(folder) => replace_request(&mut folder.items, id, request),
    })
}

/// The secret variables `code` references, each once, in order of appearance.
fn secret_names(inspection: Option<&Inspection>, code: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for span in inspection
        .map(|inspection| inspection.spans(code))
        .unwrap_or_default()
    {
        if let Span::Reference {
            name, secret: true, ..
        } = span
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

/// `names` as the code writes them: `{{a}}, {{b}}`.
fn references(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("{{{{{name}}}}}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the toast after Copy says under its title: the request and the secrets left.
pub(crate) fn copied_subtitle(name: &str, secrets: &[String]) -> String {
    if secrets.is_empty() {
        name.to_owned()
    } else {
        format!(
            "{name} · {} left as {}",
            toasts::counted(secrets.len(), "secret"),
            references(secrets)
        )
    }
}

/// The body of the secrets note.
pub(crate) fn secrets_note(secrets: &[String]) -> String {
    const WRITTEN: &str = "Every {{variable}} is written as is: nothing is resolved.";
    match secrets {
        [] => WRITTEN.to_owned(),
        [name] => {
            format!("{WRITTEN} The value of {name} is not in the code; set it before you run it.")
        }
        names => format!(
            "{WRITTEN} The values of {} are not in the code; set them before you run it.",
            names.join(", ")
        ),
    }
}

impl Shell {
    /// Ctrl+Shift+C and the palette: opens the dialog for the active request.
    pub(super) fn open_codegen_active(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.active_request().map(str::to_owned) {
            self.request_codegen(&id, cx);
        }
    }

    /// Opens the dialog for the request `id` in curl; the focus moves into it on its first
    /// frame. A request the engine cannot write as code shows an error toast instead.
    pub(super) fn request_codegen(&mut self, id: &str, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let draft = self.drafts.get(id).map(|draft| &draft.request);
        let language = Language::Curl;
        let code = match generate(engine, id, draft, language) {
            Ok(code) => code,
            Err(error) => {
                toasts::failure("Cannot generate code", error.to_string(), cx);
                return;
            }
        };
        let inspection = match self.requests.get(id) {
            Some(state) => Some(state.inspection.clone()),
            None => engine
                .inspect_current(id, self.active_environment.as_deref(), &self.current)
                .inspect_err(|error| log::warn!("cannot inspect request {id}: {error}"))
                .ok(),
        };
        let method = draft
            .or_else(|| {
                inspection
                    .as_ref()
                    .map(|inspection| &inspection.request.request)
            })
            .map(|request| request.method.to_string())
            .unwrap_or_default();
        let name = tree_model::find(&self.roots, id)
            .map(|node| node.name.clone())
            .unwrap_or_default();
        let secrets = secret_names(inspection.as_ref(), &code);
        self.codegen.open = Some(Dialog {
            request_id: id.to_owned(),
            name,
            method,
            from_draft: draft.is_some(),
            language,
            code,
            secrets,
            inspection,
        });
        self.codegen.focus.focus_on_next_render();
        cx.notify();
    }

    /// Shows the code in `language`.
    fn set_codegen_language(&mut self, language: Language, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(dialog) = self.codegen.open.as_ref() else {
            return;
        };
        let draft = self
            .drafts
            .get(&dialog.request_id)
            .map(|draft| &draft.request);
        match generate(engine, &dialog.request_id, draft, language) {
            Ok(code) => {
                if let Some(dialog) = self.codegen.open.as_mut() {
                    dialog.secrets = secret_names(dialog.inspection.as_ref(), &code);
                    dialog.code = code;
                    dialog.language = language;
                }
            }
            Err(error) => toasts::failure("Cannot generate code", error.to_string(), cx),
        }
        cx.notify();
    }

    /// Copy: puts the code on the clipboard, closes the dialog and confirms it.
    fn copy_codegen(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.codegen.open.as_ref() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(dialog.code.clone()));
        let title = format!("Copied as {}", language_label(dialog.language));
        let subtitle = copied_subtitle(&dialog.name, &dialog.secrets);
        self.close_codegen(cx);
        toasts::success(title, subtitle, cx);
    }

    /// Closes the dialog and gives the focus back.
    fn close_codegen(&mut self, cx: &mut Context<Self>) {
        if self.codegen.open.take().is_some() {
            self.codegen.focus.restore(cx);
        }
        cx.notify();
    }

    /// The code the open dialog shows, for tests.
    #[cfg(test)]
    pub(super) fn codegen_code(&self) -> Option<(Language, String)> {
        self.codegen
            .open
            .as_ref()
            .map(|dialog| (dialog.language, dialog.code.clone()))
    }
}

/// The dialog, while it is open, over the window.
pub(super) fn render(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    shell.codegen.focus.apply_pending(window, cx);
    let dialog = shell.codegen.open.as_ref()?;
    let header = header(dialog, shell.active_environment.as_deref(), cx);
    let body = div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::BODY_GAP)
        .child(toolbar(dialog, cx))
        .child(code_block(dialog, cx))
        .child(
            region("codegen-secrets")
                .role(Role::Label)
                .aria_label(secrets_note(&dialog.secrets))
                .child(
                    BannerBlock::new(BannerVariant::Info, "Secrets stay as {{variables}}.")
                        .default_icon(ui::icon(IconName::Lock))
                        .with_body(secrets_note(&dialog.secrets)),
                ),
        );
    let lines = toasts::counted(dialog.code.lines().count(), "line");
    let footer = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            ui::role(text_region("codegen-lines"), TextVariant::Caption, cx)
                .aria_label(lines.clone())
                .child(lines),
        )
        .child(div().flex_1())
        .child(
            Button::new("codegen-close", "Close")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| this.close_codegen(cx))),
        )
        .child(
            Button::new("codegen-copy", "Copy")
                .primary()
                .icon(ui::icon(IconName::Copy))
                .kbd("↵")
                .on_click(cx.listener(|this, _event, _window, cx| this.copy_codegen(cx))),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(TITLE)
        .id("codegen-backdrop")
        .icon(ui::icon(IconName::Code))
        .width(tokens::CODEGEN_DIALOG_WIDTH)
        .header_extra(header)
        .focus_handle(shell.codegen.focus.handle())
        .body(
            region("codegen-dialog")
                .role(Role::Dialog)
                .aria_label(TITLE)
                .child(body),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_codegen(cx)) {
                log::warn!("generate code close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.copy_codegen(cx)) {
                log::warn!("generate code copy: {error}");
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

/// The request beside the title, `codegen-request`: its method in its hue, its name and
/// the active environment.
fn header(dialog: &Dialog, environment: Option<&str>, cx: &App) -> Stateful<Div> {
    let place = match environment {
        Some(environment) => format!("{} · {environment}", dialog.name),
        None => dialog.name.clone(),
    };
    ui::role(text_region("codegen-request"), TextVariant::Caption, cx)
        .aria_label(format!("{} {place}", dialog.method))
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
        .child(place)
}

/// The language control and where the code comes from.
fn toolbar(dialog: &Dialog, cx: &mut Context<Shell>) -> Div {
    let items = Language::ALL
        .into_iter()
        .map(|language| {
            let item = SegmentedItem::new(language.as_str(), language_label(language))
                .element_id(format!("codegen-language-{language}"));
            if language == Language::Curl {
                item.icon(ui::icon(IconName::SquareTerminal))
            } else {
                item
            }
        })
        .collect();
    let shell = cx.entity().downgrade();
    let control = segmented::control(
        "codegen-language",
        Some(SharedString::from("Language")),
        true,
        items,
        dialog.language.as_str(),
        move |id, _window, cx| {
            let Ok(language) = id.parse::<Language>() else {
                return;
            };
            if let Err(error) = shell.update(cx, |this, cx| this.set_codegen_language(language, cx))
            {
                log::warn!("generate code language: {error}");
            }
        },
    );
    let source = if dialog.from_draft {
        "Read only · from the current draft"
    } else {
        "Read only · from the saved request"
    };
    div()
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .child(control)
        .child(div().flex_1())
        .child(
            ui::role(text_region("codegen-source"), TextVariant::Caption, cx)
                .aria_label(source)
                .child(source),
        )
}

/// The code, read only, one numbered line each, with its references styled.
// bolt gap: `modal_code` has no line numbers or reference styling; the lines are drawn as
// the Body pane draws its code, inside `bolt`'s modal frame.
fn code_block(dialog: &Dialog, cx: &App) -> Div {
    let colors = bolt::colors(cx);
    let lines = (1..)
        .zip(dialog.code.lines())
        .map(|(n, line): (usize, &str)| {
            let text: Vec<AnyElement> = match dialog.inspection.as_ref() {
                Some(inspection) => spans::render(inspection, line, cx),
                None => vec![line.to_owned().into_any_element()],
            };
            labeled_row(format!("codegen-line-{n}"), line.to_owned())
                .flex()
                .items_center()
                .h(EditorMetrics::CODE_LINE_HEIGHT)
                .child(
                    div()
                        .w(GridMetrics::ROW_NUMBER_WIDTH)
                        .flex_none()
                        .pr(Spacing::MD)
                        .text_right()
                        .text_color(colors.line_2)
                        .child(n.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .whitespace_nowrap()
                        .text_color(colors.foreground)
                        .children(text),
                )
        })
        .collect::<Vec<_>>();
    modal_frame(cx).child(
        part("codegen-code".to_owned())
            .role(Role::Label)
            .aria_label(format!(
                "Generated {} code",
                language_label(dialog.language)
            ))
            .py(ModalMetrics::CODE_PADDING_Y)
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(EditorMetrics::CODE_FONT)
            .children(lines),
    )
}

#[cfg(test)]
#[path = "codegen_tests.rs"]
mod tests;
