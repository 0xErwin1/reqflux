//! The GraphQL body editing (v1-features S7, design/layouts/01 D6): the query, plain text
//! (no graphql grammar yet), and the variables, highlighted as JSON (design/syntax/01), each
//! edited in a code editor opened from its read-only well, one at a time per request, like the raw body's. Every keystroke lands in the
//! draft as it is typed; Enter inserts a newline, Ctrl+Enter sends — and refuses while the
//! variables are not valid JSON — and Escape closes the editor keeping the text. The
//! variables are validated on every edit against what the engine sends: a failure shows
//! the engine's exact message under the editor and turns the verdict badge danger.

use bolt::controls::{InputEnter, InputEscape};
use bolt::primitives::{Badge, BadgeTone, Icon, TextVariant};
use bolt::tokens::{Fields, Spacing};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, InteractiveElement, Role, Styled, Subscription, Window, div,
};
use gpui_component::input::{EditorState, InputEvent};
use gpui_kit_assets::IconName;
use reqflux_core::model::Body;
use reqflux_engine::Inspection;

use super::super::{code, ui};
use super::controls::{focus_ring, text_well};
use crate::shell::code_edit;
use crate::shell::rows::part;
use crate::shell::{Shell, region};

/// The icon of the query column.
const QUERY_ICON: IconName = IconName::Code;
/// The rows the query and variables editors show.
const EDITOR_ROWS: usize = 8;

/// Which GraphQL field the editor edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphqlField {
    /// The query text.
    Query,
    /// The variables, as JSON text.
    Variables,
}

impl GraphqlField {
    /// The id of the read-only well, which the editor replaces with `-input`.
    fn well_id(self) -> &'static str {
        match self {
            Self::Query => "request-body-query",
            Self::Variables => "request-body-variables",
        }
    }

    /// The grammar its editor and well highlight with.
    pub(crate) fn language(self) -> &'static str {
        match self {
            Self::Query => code_edit::PLAIN,
            Self::Variables => "json",
        }
    }

    /// The name the column label shows.
    fn label(self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Variables => "Variables",
        }
    }
}

/// The GraphQL editor of one request: the field it edits and its state.
pub(crate) struct GraphqlEdit {
    pub(crate) field: GraphqlField,
    pub(crate) input: Entity<EditorState>,
    /// Whether the editor has the focus, which its ring shows.
    pub(crate) focused: bool,
    /// Puts the text into the draft on every change and follows the focus.
    _change: Subscription,
}

/// Why the GraphQL `variables` cannot be sent, with the engine's exact message; `None`
/// when they are blank or valid JSON, as the engine's prepare accepts blank variables too.
pub(super) fn variables_error(variables: &str) -> Option<String> {
    if variables.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(variables)
        .err()
        .map(|error| reqflux_engine::ExecuteError::GraphqlVariables(error.to_string()).to_string())
}

/// The GraphQL pane of the request `request_id`: the query and its variables beside each
/// other, each a read-only well until clicked, with the variables' verdict and the
/// engine's message when they do not parse.
pub(super) fn render(
    request_id: &str,
    inspection: &Inspection,
    query: &str,
    variables: &str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let columns = [
        (GraphqlField::Query, QUERY_ICON, query, None),
        (
            GraphqlField::Variables,
            IconName::Braces,
            variables,
            variables_error(variables),
        ),
    ];
    let mut columns_rendered = Vec::with_capacity(columns.len());
    for (field, icon, text, error) in columns {
        columns_rendered.push(column(
            request_id, inspection, field, icon, text, error, shell, cx,
        ));
    }
    region("request-body-graphql")
        .flex()
        .gap(Spacing::MD)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pt(Spacing::SM)
        .children(columns_rendered)
        .into_any_element()
}

/// One column: the label, the verdict badge of the variables, the well or the editor, and
/// the failure banner.
#[expect(clippy::too_many_arguments, reason = "one column of one pane's render")]
fn column(
    request_id: &str,
    inspection: &Inspection,
    field: GraphqlField,
    icon: IconName,
    text: &str,
    error: Option<String>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let editing = editor(request_id, field, shell, cx);
    let head = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            Icon::new(ui::icon(icon))
                .size(Fields::SEGMENT_ICON)
                .color(colors.muted),
        )
        .child(
            ui::role(
                part(format!("{}-label", field.well_id())),
                TextVariant::Body,
                cx,
            )
            .text_color(colors.strong)
            .child(field.label()),
        )
        .children((field == GraphqlField::Variables).then(|| {
            let verdict = match &error {
                Some(_) => "NOT VALID JSON",
                None => "VALID JSON",
            };
            part("request-body-variables-badge".to_owned())
                .role(Role::Label)
                .aria_label(verdict)
                .flex_none()
                .child(match &error {
                    Some(_) => Badge::new("NOT VALID JSON", BadgeTone::Danger),
                    None => Badge::new("VALID JSON", BadgeTone::Success),
                })
        }));
    let banner = error.map(|message| {
        region("request-body-variables-error")
            .role(Role::Label)
            .aria_label(message.clone())
            .flex()
            .items_start()
            .gap(Spacing::SM)
            .text_size(bolt::density::font_xs(cx))
            .text_color(colors.danger)
            .font_family(bolt::fonts::editor_family(cx))
            .child(
                Icon::new(ui::icon(IconName::TriangleAlert))
                    .size(Fields::SEGMENT_ICON)
                    .color(colors.danger),
            )
            .child(message)
    });
    match editing {
        Some(editor) => div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(Spacing::XS)
            .child(head)
            .child(editor)
            .children(banner)
            .into_any_element(),
        None => {
            let request = request_id.to_owned();
            let block = code::block(
                inspection,
                format!("{}-code", field.well_id()),
                field.well_id(),
                text,
                field.language(),
                cx,
            );
            let well = text_well(field.well_id().to_owned(), cx)
                .cursor_text()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.start_graphql_edit(&request, field, window, cx);
                }))
                .child(block);
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(Spacing::XS)
                .child(head)
                .child(well)
                .children(banner)
                .into_any_element()
        }
    }
}

/// The open editor of `field`, over the well it replaces, or `None` when another field or
/// no editor is open.
fn editor(
    request_id: &str,
    field: GraphqlField,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let state = shell.requests.get(request_id)?;
    let edit = state
        .graphql_edit
        .as_ref()
        .filter(|edit| edit.field == field)?;
    let request = request_id.to_owned();
    let escape_request = request.clone();
    Some(
        text_well(format!("{}-input", field.well_id()), cx)
            .capture_action(cx.listener(move |this, action: &InputEnter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    if this.graphql_variables_refused(&request).is_none() {
                        this.send_active(window, cx);
                    }
                }
            }))
            .on_action(cx.listener(move |this, _: &InputEscape, window, cx| {
                this.close_graphql_edit(&escape_request, window, cx);
            }))
            .child(code_edit::element(&edit.input, EDITOR_ROWS, cx))
            .when(edit.focused, |well| {
                well.child(focus_ring(format!("{}-focus-ring", field.well_id()), cx))
            })
            .into_any_element(),
    )
}

impl Shell {
    /// The GraphQL field of the request `id` whose editor is open.
    pub(super) fn graphql_edit_of(&self, id: &str) -> Option<GraphqlField> {
        self.requests
            .get(id)?
            .graphql_edit
            .as_ref()
            .map(|edit| edit.field)
    }

    /// Why Ctrl+Enter must not send the request `id`: the variables of its GraphQL body,
    /// as the draft has it, do not parse.
    fn graphql_variables_refused(&self, id: &str) -> Option<String> {
        let state = self.requests.get(id)?;
        let request = self.displayed_request(id, &state.inspection.request.request);
        match &request.body {
            Body::Graphql { variables, .. } => variables_error(variables),
            _ => None,
        }
    }

    /// Swaps the GraphQL `field` of the request `id` for a focused code editor
    /// seeded with it. An open cell field commits first; the editor of the other field, if
    /// open, closes for it. Opening the field that is already open changes nothing.
    pub(super) fn start_graphql_edit(
        &mut self,
        id: &str,
        field: GraphqlField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        if self.graphql_edit_of(id) == Some(field) {
            return;
        }
        let Some(state) = self.requests.get(id) else {
            return;
        };
        let Body::Graphql { query, variables } = &self
            .displayed_request(id, &state.inspection.request.request)
            .body
        else {
            return;
        };
        let text = match field {
            GraphqlField::Query => query,
            GraphqlField::Variables => variables,
        };
        let input = code_edit::open(field.language(), text.clone(), window, cx);
        let request_id = id.to_owned();
        let change = cx.subscribe(
            &input,
            move |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let value = input.read(cx).value().to_string();
                    this.set_draft_graphql_text(&request_id, field, value, cx);
                }
                InputEvent::Focus | InputEvent::Blur => {
                    if let Some(edit) = this
                        .requests
                        .get_mut(&request_id)
                        .and_then(|state| state.graphql_edit.as_mut())
                    {
                        edit.focused = matches!(event, InputEvent::Focus);
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { .. } => {}
            },
        );
        if let Some(state) = self.requests.get_mut(id) {
            state.graphql_edit = Some(GraphqlEdit {
                field,
                input,
                focused: true,
                _change: change,
            });
        }
        cx.notify();
    }

    /// Puts `value` into the query or the variables of the request `id`'s draft. The text
    /// the request already has, without a draft, creates none.
    fn set_draft_graphql_text(
        &mut self,
        id: &str,
        field: GraphqlField,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let current = self.requests.get(id).and_then(|state| {
            match &self
                .displayed_request(id, &state.inspection.request.request)
                .body
            {
                Body::Graphql { query, variables } => Some(match field {
                    GraphqlField::Query => query.clone(),
                    GraphqlField::Variables => variables.clone(),
                }),
                _ => None,
            }
        });
        if !self.drafts.contains_key(id) && current.as_deref() == Some(value.as_str()) {
            return;
        }
        self.edit_draft(
            id,
            move |request| {
                if let Body::Graphql { query, variables } = &mut request.body {
                    match field {
                        GraphqlField::Query => *query = value,
                        GraphqlField::Variables => *variables = value,
                    }
                }
            },
            cx,
        );
    }

    /// Escape in the editor: closes it and gives the focus back to the shell. The text is
    /// already in the draft.
    pub(super) fn close_graphql_edit(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.requests.get_mut(id)
            && state.graphql_edit.take().is_some()
        {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Drops the GraphQL editor of the request `id`, as when its body mode changes.
    pub(super) fn drop_graphql_edit_of(&mut self, id: &str) {
        if let Some(state) = self.requests.get_mut(id) {
            state.graphql_edit = None;
        }
    }

    /// The mode control's switch: an open GraphQL editor of the request `id` closes with
    /// the mode it edited, then the body mode changes.
    pub(super) fn change_body_mode(
        &mut self,
        id: &str,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_graphql_edit_of(id);
        self.set_body_mode(id, kind, window, cx);
    }
}
