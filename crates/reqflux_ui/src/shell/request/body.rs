//! The Body pane: the mode row, a segmented control that switches the body mode with, for a
//! raw body, the language select that opens the language menu, and the content type hint;
//! then the raw text in a field well, which opens a multi-line editor, the urlencoded or
//! form-data grid, which edits like the Params grid, the read-only GraphQL query or binary
//! file path in a well, or the empty state. Every edit lands in the request's draft.

pub(super) mod controls;

use bolt::controls::{InputEnter, InputEscape};
use bolt::primitives::{Icon, SegmentedItem};
use bolt::tokens::{Borders, EditorMetrics, Fields, ResultMetrics, Spacing};
use gpui::{AnyElement, Context, ParentElement, SharedString, Styled, Window, div, prelude::*};
use gpui_component::input::Textarea;
use gpui_kit_assets::IconName;
use reqflux_core::model::{Body, FormPart, KeyValue, RawLanguage};
use reqflux_engine::Inspection;

use self::controls::{focus_ring, select, text_well, well};
use super::pane_model::body_mode;
use super::segmented::control;
use super::{body_menu, code, grid, ui};
use crate::shell::Shell;
use crate::shell::body_edit::{BODY_MODES, body_hint, body_kind};
use crate::shell::rows::part;

/// The icon of the mode `kind` in the mode control.
fn mode_icon(kind: &str) -> IconName {
    match kind {
        "raw" => IconName::Braces,
        "urlencoded" => IconName::List,
        "formdata" => IconName::Paperclip,
        "binary" => IconName::File,
        "graphql" => IconName::Network,
        _ => IconName::Ban,
    }
}

/// The name of `language` in the language select.
fn language_name(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Json => "JSON",
        RawLanguage::Xml => "XML",
        RawLanguage::Text => "Text",
        RawLanguage::Html => "HTML",
        RawLanguage::Javascript => "JavaScript",
    }
}

/// The rows of a urlencoded or form-data `body`, which its segment counts.
fn row_count(body: &Body) -> Option<usize> {
    match body {
        Body::Urlencoded { fields } => Some(fields.len()),
        Body::FormData { parts } => Some(parts.len()),
        _ => None,
    }
}

/// The mode row of `body` of the request `request_id`: the mode control
/// `request-body-mode`, labeled with the mode, the language select `request-body-language` of a raw body and the
/// hint `request-body-hint`.
fn mode_row(
    request_id: &str,
    body: &Body,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let (muted, line) = (colors.muted, colors.line);
    let current = body_kind(body);
    let segments: Vec<_> = BODY_MODES
        .into_iter()
        .map(|(kind, name)| {
            let item = SegmentedItem::new(kind, name)
                .element_id(format!("body-mode-{kind}"))
                .icon(ui::icon(mode_icon(kind)));
            match row_count(body).filter(|count| *count > 0 && kind == current) {
                Some(count) => item.count(count.to_string()),
                None => item,
            }
        })
        .collect();
    let request = request_id.to_owned();
    let on_select = cx.listener(move |this, kind: &SharedString, window, cx| {
        this.set_body_mode(&request, kind, window, cx);
    });
    let language = match body {
        Body::Raw { language, .. } => Some(
            div()
                .relative()
                .flex_none()
                .child(
                    select(
                        "request-body-language".to_owned(),
                        None,
                        language_name(*language),
                    )
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.toggle_body_menu(window, cx);
                    })),
                )
                .when(shell.body_menu_open, |anchor| {
                    anchor.child(body_menu::render(request_id, *language, shell, window, cx))
                }),
        ),
        _ => None,
    };
    let hint = body_hint(body).map(|hint| {
        part("request-body-hint".to_owned())
            .whitespace_nowrap()
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(ResultMetrics::STATEMENT_FONT)
            .text_color(muted)
            .child(hint)
    });
    div()
        .flex_none()
        .h(ResultMetrics::VIEW_ROW_HEIGHT)
        .flex()
        .items_center()
        .gap(ResultMetrics::VIEW_ROW_GAP)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .border_t(Borders::THIN)
        .border_color(line)
        .child(control(
            "request-body-mode",
            Some(body_mode(body).into()),
            true,
            segments,
            current,
            on_select,
        ))
        .children(language)
        .children(hint)
        .into_any_element()
}

/// The open raw text editor of the request `request_id`, `request-body-input`. Ctrl+Enter
/// sends before the editor turns it into a newline; Escape leaves the editor. Its focus ring,
/// `request-body-input-focus-ring`, shows only while it has the focus.
fn editor(shell: &Shell, request_id: &str, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let edit = shell
        .body_edit
        .as_ref()
        .filter(|edit| edit.request_id == request_id)?;
    let colors = bolt::colors(cx);
    Some(
        text_well("request-body-input".to_owned(), cx)
            .mx(Spacing::MD)
            .mt(Spacing::SM)
            .pl(Spacing::MD)
            .pr(Spacing::MD)
            .capture_action(cx.listener(|this, action: &InputEnter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.send_active(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.close_body_edit(window, cx);
            }))
            .child(
                Textarea::new(&edit.input)
                    .appearance(false)
                    .bordered(false)
                    .text_size(EditorMetrics::CODE_FONT)
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_color(colors.foreground),
            )
            .when(edit.focused, |well| {
                well.child(focus_ring("request-body-input-focus-ring".to_owned(), cx))
            })
            .into_any_element(),
    )
}

/// The pane of `body` of the request `request_id`, the draft's when the request has one,
/// else the saved file's.
pub(super) fn render(
    request_id: &str,
    inspection: &Inspection,
    body: &Body,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let mode = mode_row(request_id, body, shell, window, cx);
    let code_block = |text: &str, cx: &mut Context<Shell>| {
        code::block(
            inspection,
            "request-body-code".to_owned(),
            "request-body-line",
            text,
            cx,
        )
    };
    let content = match body {
        Body::None => vec![code::empty(
            "request-body-empty".to_owned(),
            IconName::Ban,
            "This request has no body",
        )],
        Body::Raw { text, .. } => match editor(shell, request_id, cx) {
            Some(editor) => vec![editor],
            None => {
                let request = request_id.to_owned();
                let block = code_block(text, cx);
                vec![
                    text_well("request-body-raw".to_owned(), cx)
                        .mx(Spacing::MD)
                        .mt(Spacing::SM)
                        .cursor_text()
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.start_body_edit(&request, window, cx);
                        }))
                        .child(block)
                        .into_any_element(),
                ]
            }
        },
        Body::Graphql { query, variables } => {
            let query = code_block(query, cx);
            let variables = code::block(
                inspection,
                "request-body-variables".to_owned(),
                "request-body-variables-line",
                variables,
                cx,
            );
            vec![
                text_well("request-body-query".to_owned(), cx)
                    .mx(Spacing::MD)
                    .mt(Spacing::SM)
                    .child(query)
                    .into_any_element(),
                code::label("Variables", cx).into_any_element(),
                text_well("request-body-variables-well".to_owned(), cx)
                    .mx(Spacing::MD)
                    .child(variables)
                    .into_any_element(),
            ]
        }
        Body::Urlencoded { fields } => {
            vec![grid::render(
                inspection,
                "body",
                "field",
                fields,
                &[],
                Some(request_id),
                shell,
                cx,
            )]
        }
        Body::FormData { parts } => {
            let (rows, files): (Vec<KeyValue>, Vec<bool>) = parts.iter().map(form_row).unzip();
            vec![grid::render(
                inspection,
                "body",
                "part",
                &rows,
                &files,
                Some(request_id),
                shell,
                cx,
            )]
        }
        Body::Binary { path } => {
            let muted = bolt::colors(cx).muted;
            let shown = if path.is_empty() {
                div().text_color(muted).child("No file")
            } else {
                div().child(path.clone())
            };
            vec![
                well("request-body-file".to_owned(), cx)
                    .mx(Spacing::MD)
                    .mt(Spacing::SM)
                    .child(
                        Icon::new(ui::icon(IconName::File))
                            .size(Fields::LEADING_ICON)
                            .color(muted),
                    )
                    .child(shown)
                    .into_any_element(),
            ]
        }
    };
    std::iter::once(mode).chain(content).collect()
}

/// A form part as a grid row, with whether its value is a file path.
fn form_row(part: &FormPart) -> (KeyValue, bool) {
    match part {
        FormPart::Text {
            key,
            value,
            description,
            enabled,
        } => (
            KeyValue {
                description: description.clone(),
                enabled: *enabled,
                ..KeyValue::new(key, value)
            },
            false,
        ),
        FormPart::File {
            key,
            path,
            description,
            enabled,
        } => (
            KeyValue {
                description: description.clone(),
                enabled: *enabled,
                ..KeyValue::new(key, path)
            },
            true,
        ),
    }
}
