//! The Body pane: the mode row, a segmented control that switches the body mode with, for a
//! raw body, the language select that opens the language menu, and the content type hint;
//! then the raw text in a field well, which opens a multi-line editor, the urlencoded or
//! form-data grid, which edits like the Params grid, the read-only GraphQL query or binary
//! file path in a well, or the empty state. Every edit lands in the request's draft.

pub(super) mod controls;

use bolt::tokens::{border, family, space};
use bolt::{Icon, IconName};
use gpui::{AnyElement, Context, ParentElement, Styled, Window, div, prelude::*};
use gpui_component::input::{Enter, Escape, Textarea};
use reqflux_core::model::{Body, FormPart, KeyValue, RawLanguage};
use reqflux_engine::Inspection;

use self::controls::{segment, select, text_well, track, well};
use super::pane_model::body_mode;
use super::{body_menu, code, grid};
use crate::shell::Shell;
use crate::shell::body_edit::{BODY_MODES, body_hint, body_kind};
use crate::shell::rows::part;
use crate::tokens;

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
    let (muted, line) = {
        let colors = bolt::colors(cx);
        (colors.muted, colors.line)
    };
    let current = body_kind(body);
    let segments: Vec<_> = BODY_MODES
        .into_iter()
        .map(|(kind, name)| {
            let request = request_id.to_owned();
            let count = row_count(body)
                .filter(|count| *count > 0 && kind == current)
                .map(|count| count.to_string());
            segment(
                format!("body-mode-{kind}"),
                name,
                mode_icon(kind),
                count,
                kind == current,
                cx,
            )
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.set_body_mode(&request, kind, window, cx);
            }))
        })
        .collect();
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
                        cx,
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
            .font_family(family::MONO)
            .text_size(tokens::RESPONSE_META_TEXT)
            .text_color(muted)
            .child(hint)
    });
    div()
        .flex_none()
        .h(tokens::SUB_TAB_BAR)
        .flex()
        .items_center()
        .gap(space::MD)
        .pl(space::MD)
        .pr(space::MD)
        .border_t(border::THIN)
        .border_color(line)
        .child(track("request-body-mode", segments, cx).aria_label(body_mode(body)))
        .children(language)
        .children(hint)
        .into_any_element()
}

/// The open raw text editor of the request `request_id`, `request-body-input`. Ctrl+Enter
/// sends before the editor turns it into a newline; Escape leaves the editor.
fn editor(shell: &Shell, request_id: &str, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let edit = shell
        .body_edit
        .as_ref()
        .filter(|edit| edit.request_id == request_id)?;
    let colors = bolt::colors(cx);
    Some(
        text_well("request-body-input".to_owned(), cx)
            .mx(space::MD)
            .mt(space::SM)
            .pl(space::MD)
            .pr(space::MD)
            .capture_action(cx.listener(|this, action: &Enter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.send_active(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.close_body_edit(window, cx);
            }))
            .child(
                Textarea::new(&edit.input)
                    .appearance(false)
                    .bordered(false)
                    .text_size(tokens::GRID_TEXT)
                    .font_family(family::MONO)
                    .text_color(colors.foreground),
            )
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
            "This request has no body",
            cx,
        )],
        Body::Raw { text, .. } => match editor(shell, request_id, cx) {
            Some(editor) => vec![editor],
            None => {
                let request = request_id.to_owned();
                let block = code_block(text, cx);
                vec![
                    text_well("request-body-raw".to_owned(), cx)
                        .mx(space::MD)
                        .mt(space::SM)
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
                    .mx(space::MD)
                    .mt(space::SM)
                    .child(query)
                    .into_any_element(),
                code::label("Variables", cx).into_any_element(),
                text_well("request-body-variables-well".to_owned(), cx)
                    .mx(space::MD)
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
                    .mx(space::MD)
                    .mt(space::SM)
                    .child(
                        Icon::new(IconName::File)
                            .size(tokens::GRID_TEXT)
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
