//! Documentation (feedback-nov3 S11, design/docs/01): the markdown description of a request,
//! a folder or a collection, shown rendered with an Edit button that swaps it for a
//! multi-line editor, or, when there is none, an empty state that offers to add it.
//!
//! A request edits its description in its draft, as it does its scripts: every keystroke
//! lands in the draft, Done closes the editor and Ctrl+S saves it with the request. A
//! collection has no draft: its settings section writes through the engine when Save is
//! pressed, and Cancel (or Escape) drops the text. This module holds the part both share
//! and the editing of each.

use std::rc::Rc;

use bolt::composites::EmptyState;
use bolt::controls::{Button, InputEscape};
use bolt::primitives::{Chamfer, ChamferRing, Icon, TextVariant};
use bolt::tokens::{ChamferCut, Spacing};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, Entity, InteractiveElement,
    ParentElement, SharedString, Styled, Subscription, Window, div, prelude::*,
};
use gpui_component::input::{EditorState, InputEvent};
use gpui_kit_assets::IconName;
use reqflux_core::model::Id;

use crate::shell::request::ui;
use crate::shell::rows::part;
use crate::shell::{Shell, code_edit, region};
use crate::workspace::OpenedWorkspace;

/// The rows the description editor shows.
const EDITOR_ROWS: usize = 14;

/// A button's action, run with the window and app.
pub(crate) type Act = Rc<dyn Fn(&mut Window, &mut App)>;

/// An action that runs `act` on the shell, ignoring it when the shell is gone.
pub(crate) fn act(
    cx: &Context<Shell>,
    run: impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
) -> Act {
    let shell = cx.entity().downgrade();
    Rc::new(move |window, cx| {
        if let Err(error) = shell.update(cx, |this, cx| run(this, window, cx)) {
            log::debug!("a documentation action ran after the shell closed: {error}");
        }
    })
}

/// The open description editor.
pub(crate) struct DocsEdit {
    pub(crate) input: Entity<EditorState>,
    /// Whether the editor has the focus, which its ring shows.
    pub(crate) focused: bool,
    /// Follows the focus, and the text where the owner wants it.
    _change: Subscription,
}

/// The description editor of a collection's settings section.
pub(crate) struct CollectionDocsEdit {
    pub(crate) collection: Id,
    pub(crate) edit: DocsEdit,
}

/// What [`render`] draws.
pub(crate) struct Docs<'a> {
    /// The element id prefix: `<prefix>-view`, `-edit`, `-input`, `-empty` and so on.
    pub(crate) prefix: &'a str,
    /// The description, `None` when there is none.
    pub(crate) text: Option<&'a str>,
    /// The open editor, if any.
    pub(crate) edit: Option<&'a DocsEdit>,
    /// Opens the editor.
    pub(crate) start: Act,
    /// Closes the editor keeping its text: Done for a request, Save for a collection.
    pub(crate) done: Act,
    pub(crate) done_label: &'static str,
    /// Closes the editor dropping its text, when there is something to drop.
    pub(crate) cancel: Option<Act>,
    /// What the empty state says under its title.
    pub(crate) hint: &'static str,
}

/// A stateful button `id` running `act`.
fn button(id: String, label: &'static str, act: Act) -> Button {
    Button::new(ElementId::Name(id.into()), label).on_click(
        move |_event: &ClickEvent, window, cx| {
            act(window, cx);
        },
    )
}

/// The documentation block: its header with the Edit button, then the rendered markdown,
/// the editor or the empty state. Element ids are `<prefix>-header`, `-edit`, `-view`,
/// `-input`, `-done`, `-cancel`, `-empty` and `-add`.
pub(crate) fn render(docs: Docs<'_>, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let prefix = docs.prefix;
    let editing = docs.edit.is_some();
    let actions = if editing {
        let cancel = docs.cancel.clone().map(|act| {
            button(format!("{prefix}-cancel"), "Cancel", act)
                .ghost()
                .inline()
        });
        let done = button(format!("{prefix}-done"), docs.done_label, docs.done.clone())
            .primary()
            .inline();
        div()
            .flex()
            .items_center()
            .gap(Spacing::XS)
            .children(cancel)
            .child(done)
            .into_any_element()
    } else if docs.text.is_some() {
        button(format!("{prefix}-edit"), "Edit", docs.start.clone())
            .ghost()
            .inline()
            .icon(ui::icon(IconName::Pencil))
            .into_any_element()
    } else {
        div().into_any_element()
    };
    let header = part(format!("{prefix}-header"))
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            Icon::new(ui::icon(IconName::FileText))
                .size(bolt::tokens::TreeMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(
            ui::role(part(format!("{prefix}-label")), TextVariant::Label, cx)
                .child("DOCUMENTATION"),
        )
        .child(div().flex_1())
        .child(actions);
    let body = match (docs.edit, docs.text) {
        (Some(edit), _) => editor(&docs, edit, cx),
        (None, Some(text)) => view(prefix, text, cx),
        (None, None) => empty(&docs, cx),
    };
    region_of(prefix)
        .flex()
        .flex_col()
        .gap(Spacing::SM)
        .text_color(colors.foreground)
        .child(header)
        .child(body)
        .into_any_element()
}

fn region_of(prefix: &str) -> gpui::Stateful<gpui::Div> {
    part(format!("{prefix}-docs"))
}

/// The sunken box around the editor or the rendered text.
fn well(id: String, cx: &App) -> gpui::Stateful<gpui::Div> {
    let colors = bolt::colors(cx);
    part(id)
        .relative()
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .py(Spacing::SM)
        .child(
            Chamfer::new(ChamferCut::CONTROL)
                .fill(colors.background)
                .border(colors.line),
        )
}

/// The rendered markdown, selectable.
fn view(prefix: &str, text: &str, cx: &App) -> AnyElement {
    // bolt gap: bolt has no markdown view; gpui-component's `TextView` renders the text with
    // the theme `bolt` installs, until DBFlux's bolt carries a document component.
    let markdown = gpui_component::text::TextView::markdown(
        ElementId::Name(format!("{prefix}-markdown").into()),
        SharedString::from(text.to_owned()),
    )
    .selectable(true);
    well(format!("{prefix}-view"), cx)
        .child(markdown)
        .into_any_element()
}

fn empty(docs: &Docs<'_>, _cx: &App) -> AnyElement {
    let prefix = docs.prefix;
    let add = button(
        format!("{prefix}-add"),
        "Add documentation",
        docs.start.clone(),
    )
    .primary()
    .icon(ui::icon(IconName::Pencil));
    div()
        .p(Spacing::LG)
        .child(
            EmptyState::new(ui::icon(IconName::FileText), docs.hint)
                .id(ElementId::Name(format!("{prefix}-empty").into()))
                .title("No documentation")
                .primary_action(add),
        )
        .into_any_element()
}

/// The editor in its well, with the accent ring while it has the focus. Escape drops the
/// text when there is something to drop, else keeps it.
fn editor(docs: &Docs<'_>, edit: &DocsEdit, cx: &App) -> AnyElement {
    let prefix = docs.prefix;
    let escape = docs.cancel.clone().unwrap_or_else(|| docs.done.clone());
    let tint = bolt::brand::brand(cx).accent_tint;
    well(format!("{prefix}-input"), cx)
        .on_action(move |_: &InputEscape, window, cx| escape(window, cx))
        .child(code_edit::element(&edit.input, EDITOR_ROWS, cx))
        .when(edit.focused, |well| {
            well.child(
                part(format!("{prefix}-focus-ring"))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(Chamfer::new(ChamferCut::CONTROL).ring(ChamferRing::outline(tint))),
            )
        })
        .into_any_element()
}

/// A focused editor seeded with `text`: plain text, without line numbers, wrapped.
fn open_editor<T: 'static>(
    text: String,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<EditorState> {
    let input = cx.new(|cx| {
        EditorState::new(window, cx)
            .language(code_edit::PLAIN)
            .soft_wrap(true)
            .auto_close(false)
            .smart_indent(false)
    });
    input.update(cx, |state, cx| {
        state.set_value(text, window, cx);
        state.focus(window, cx);
    });
    input
}

/// The text as an editor would give it: blank text is no text.
fn normalized(text: &str) -> &str {
    if text.trim().is_empty() { "" } else { text }
}

impl Shell {
    /// Opens the description editor of the request `id` on its description, committing an
    /// open cell field first. Opening it again changes nothing.
    pub(in crate::shell) fn start_request_docs_edit(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let Some(state) = self.requests.get(id) else {
            return;
        };
        if state.docs_edit.is_some() {
            return;
        }
        let text = self
            .displayed_request(id, &state.inspection.request.request)
            .description_text()
            .unwrap_or_default();
        let input = open_editor(text, window, cx);
        let request = id.to_owned();
        let change = cx.subscribe(
            &input,
            move |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let value = input.read(cx).value().to_string();
                    this.set_draft_description(&request, &value, cx);
                }
                InputEvent::Focus | InputEvent::Blur => {
                    if let Some(edit) = this
                        .requests
                        .get_mut(&request)
                        .and_then(|state| state.docs_edit.as_mut())
                    {
                        edit.focused = matches!(event, InputEvent::Focus);
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { .. } => {}
            },
        );
        if let Some(state) = self.requests.get_mut(id) {
            state.docs_edit = Some(DocsEdit {
                input,
                focused: true,
                _change: change,
            });
        }
        cx.notify();
    }

    /// Puts `value` into the description of the request `id`'s draft. The text the request
    /// already shows, without a draft, creates none.
    fn set_draft_description(&mut self, id: &str, value: &str, cx: &mut Context<Self>) {
        let Some(state) = self.requests.get(id) else {
            return;
        };
        let current = self
            .displayed_request(id, &state.inspection.request.request)
            .description_text()
            .unwrap_or_default();
        let value = normalized(value);
        if current == value {
            return;
        }
        let value = value.to_owned();
        self.edit_draft(id, move |request| request.set_description(&value), cx);
    }

    /// Done: closes the editor of the request `id` and gives the focus back to the shell.
    /// The text is already in the draft.
    pub(in crate::shell) fn close_request_docs_edit(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.requests.get_mut(id)
            && state.docs_edit.take().is_some()
        {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Drops the description editor of the request `id`, as when its draft is saved or
    /// reverted and the text it holds is no longer the draft's.
    pub(in crate::shell) fn drop_docs_edit_of(&mut self, id: &str) {
        if let Some(state) = self.requests.get_mut(id) {
            state.docs_edit = None;
        }
    }

    /// Opens the description editor of the collection `id` on its description.
    pub(in crate::shell) fn start_collection_docs_edit(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self
            .settings
            .docs
            .as_ref()
            .is_some_and(|open| open.collection == id)
        {
            return;
        }
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let Some(node) = engine
            .workspace()
            .collections
            .iter()
            .find(|node| node.collection.id == id)
        else {
            return;
        };
        let text = node.collection.description_text().unwrap_or_default();
        let input = open_editor(text, window, cx);
        let focus = cx.subscribe(&input, |this, _input, event: &InputEvent, cx| {
            if let Some(open) = this.settings.docs.as_mut()
                && matches!(event, InputEvent::Focus | InputEvent::Blur)
            {
                open.edit.focused = matches!(event, InputEvent::Focus);
                cx.notify();
            }
        });
        self.settings.docs = Some(CollectionDocsEdit {
            collection: id,
            edit: DocsEdit {
                input,
                focused: true,
                _change: focus,
            },
        });
        cx.notify();
    }

    /// Save: writes the editor's text as the description of the collection `id` through the
    /// engine and closes the editor. A refused write keeps the editor open and shows why.
    pub(in crate::shell) fn save_collection_docs_edit(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self
            .settings
            .docs
            .as_ref()
            .filter(|open| open.collection == id)
            .map(|open| open.edit.input.read(cx).value().to_string())
        else {
            return;
        };
        let outcome = self.with_engine(|engine| {
            engine
                .set_description(id, &text)
                .map(|()| None)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok(_) => {
                self.show_variables_warning(None, cx);
                self.close_collection_docs_edit(window, cx);
            }
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    /// Cancel: drops the editor of the collection settings and its text, and gives the focus
    /// back to the shell.
    pub(in crate::shell) fn close_collection_docs_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.docs.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }
}

/// The Documentation block of the settings section of the collection `id`
/// (design/docs/01 frame B): its description rendered, or the editor with Save and Cancel.
/// Nothing without a loaded workspace.
pub(in crate::shell) fn collection_block(
    shell: &Shell,
    id: Id,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
        return div().into_any_element();
    };
    let Some(node) = engine
        .workspace()
        .collections
        .iter()
        .find(|node| node.collection.id == id)
    else {
        return div().into_any_element();
    };
    let text = node.collection.description_text();
    let edit = shell
        .settings
        .docs
        .as_ref()
        .filter(|open| open.collection == id)
        .map(|open| &open.edit);
    let docs = Docs {
        prefix: "collection-docs",
        text: text.as_deref(),
        edit,
        start: act(cx, move |this, window, cx| {
            this.start_collection_docs_edit(id, window, cx);
        }),
        done: act(cx, move |this, window, cx| {
            this.save_collection_docs_edit(id, window, cx);
        }),
        done_label: "Save",
        cancel: Some(act(cx, |this, window, cx| {
            this.close_collection_docs_edit(window, cx);
        })),
        hint: "Notes about this collection, in markdown: what it is for and how to use it.",
    };
    let colors = bolt::colors(cx);
    region("collection-docs-block")
        .flex_none()
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pt(Spacing::SM)
        .pb(Spacing::MD)
        .border_b(bolt::tokens::Borders::THIN)
        .border_color(colors.line)
        .child(render(docs, cx))
        .into_any_element()
}

#[cfg(test)]
#[path = "docs_tests.rs"]
mod tests;
