//! The read-only example tab (design/examples/01-tree-children, main frame): the tab chip
//! `example-tab` with its close button `example-tab-close`, and the document under it.
//!
//! - The header `example-header`: the name, `example of` and the request's breadcrumb, the
//!   Read-only badge, and Rename (`example-rename`), Delete (`example-delete`) and Open
//!   request (`example-open-request`).
//! - Left, `example-request`: the request as written, method and URL in
//!   `example-request-line`, then its headers and auth type, one `example-request-row-<n>`
//!   each; `{{variables}}` stay as written.
//! - Right, `example-response`: the status, the Body and Headers segments
//!   (`example-view-body`, `example-view-headers`), the body one `example-line-<n>` per line
//!   with `example-line-<n>-secret` on a line holding a masked value, or the headers one
//!   `example-header-<n>` each, a masked value as dots with `example-header-<n>-secret`. A
//!   body that is not UTF-8 shows `example-binary` instead, with Save body to file….
//! - The footer: the masking note, Copy (`example-copy`) and Save body to file…
//!   (`example-save-body`).

use bolt::composites::{document_tab, document_tab_title};
use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, Icon, SegmentedItem, TextVariant};
use bolt::tokens::{Borders, EditorMetrics, Fields, GridMetrics, Heights, Spacing, TabMetrics};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, IntoElement, ParentElement, Role, SharedString,
    Stateful, Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::{Body, KeyValue};

use super::model::{self, ShownBody};
use super::{ExampleTab, ExampleView};
use crate::shell::request::{StatusClass, secret, segmented, size_text, ui};
use crate::shell::rows::{labeled_row, part};
use crate::shell::sidebar_history::class_color;
use crate::shell::tabs::close_button;
use crate::shell::{Shell, region, text_region};
use crate::tokens::{self, method_hue};

/// What a masked value shows.
const DOTS: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

/// The tab chip of the open example: the bookmark, the name and the close button.
pub(crate) fn tab_chip(tab: &ExampleTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let close = close_button(
        "example-tab-close".to_owned(),
        format!("Close {}", tab.place.name),
        cx.listener(|this, _event, _window, cx| this.close_example_tab(cx)),
        cx,
    );
    document_tab("example-tab", tab.active, cx)
        .role(Role::Tab)
        .aria_label(format!("Example {}", tab.place.name))
        .aria_selected(tab.active)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            Icon::new(ui::icon(IconName::Bookmark))
                .size(TabMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(document_tab_title(tab.place.name.clone(), tab.active, cx))
        .child(close)
        .on_click(cx.listener(|this, _event, _window, cx| this.activate_example_tab(cx)))
}

/// The example document, `example-readonly`.
pub(crate) fn render(shell: &Shell, _window: &Window, cx: &mut Context<Shell>) -> AnyElement {
    let Some(tab) = shell.examples.tab.as_ref() else {
        return region("example-readonly").flex_1().into_any_element();
    };
    let colors = bolt::colors(cx);
    region("example-readonly")
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(header(tab, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .child(request_side(tab, cx))
                .child(response_side(tab, cx)),
        )
        .into_any_element()
}

/// The header strip with the name, where it belongs and the actions.
fn header(tab: &ExampleTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let node = tab.node.clone();
    let delete_node = tab.node.clone();
    let request = tab.place.request.clone();
    region("example-header")
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::MD)
        .h(tokens::REQUEST_LINE)
        .pl(Spacing::LG)
        .pr(Spacing::LG)
        .child(
            Icon::new(ui::icon(IconName::Bookmark))
                .size(TabMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(
            ui::role(text_region("example-name"), TextVariant::Heading, cx)
                .aria_label(tab.place.name.clone())
                .whitespace_nowrap()
                .child(tab.place.name.clone()),
        )
        .child(
            div()
                .text_color(colors.muted)
                .whitespace_nowrap()
                .child("example of"),
        )
        .child(
            text_region("example-breadcrumb")
                .aria_label(tab.place.breadcrumb())
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(GridMetrics::FONT)
                .text_color(colors.foreground)
                .child(tab.place.breadcrumb()),
        )
        .child(Badge::new("Read-only", BadgeTone::Neutral).icon(ui::icon(IconName::Lock)))
        .child(div().flex_1())
        .child(
            Button::new("example-rename", "Rename")
                .secondary()
                .icon(ui::icon(IconName::Pencil))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.sidebar.update(cx, |sidebar, cx| {
                        sidebar.update_tree(cx, |tree| tree.reveal(&node));
                        sidebar.start_rename(&node, false, window, cx);
                    });
                })),
        )
        .child(
            Button::new("example-delete", "Delete")
                .danger()
                .icon(ui::icon(IconName::Trash))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.sidebar.update(cx, |sidebar, cx| {
                        sidebar.prompt_example_delete(&delete_node, window, cx);
                    });
                })),
        )
        .child(
            Button::new("example-open-request", "Open request")
                .secondary()
                .icon(ui::icon(IconName::ExternalLink))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.leave_example_tab();
                    this.open_document(&request, cx);
                })),
        )
}

/// A caption over a side: `REQUEST AS WRITTEN`, `RESPONSE`.
fn caption(id: &'static str, text: &'static str, cx: &App) -> Stateful<Div> {
    ui::role(text_region(id), TextVariant::Label, cx)
        .aria_label(text)
        .child(text.to_uppercase())
}

/// The request as written: method and URL with its query, its headers and auth type, and a
/// raw body.
fn request_side(tab: &ExampleTab, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let request = &tab.saved.example.request;
    let method = request.method.to_string();
    let query: Vec<String> = request
        .params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| format!("{}={}", param.key, param.value))
        .collect();
    let url = if query.is_empty() {
        request.url.clone()
    } else {
        format!("{}?{}", request.url, query.join("&"))
    };
    let mut rows: Vec<(String, String)> = request
        .headers
        .iter()
        .filter(|header| header.enabled)
        .map(|header: &KeyValue| (header.key.clone(), header.value.clone()))
        .collect();
    if !request.auth.is_inherit() {
        rows.push(("Auth".to_owned(), request.auth.kind().to_owned()));
    }
    let raw = match &request.body {
        Body::Raw { text, .. } if !text.is_empty() => Some(text.clone()),
        _ => None,
    };
    region("example-request")
        .w(tokens::EXAMPLE_REQUEST_COLUMN)
        .flex_none()
        .flex()
        .flex_col()
        .gap(Spacing::MD)
        .p(Spacing::LG)
        .border_r(Borders::THIN)
        .border_color(colors.line)
        .overflow_hidden()
        .child(caption("example-request-caption", "Request as written", cx))
        .child(
            text_region("example-request-line")
                .aria_label(format!("{method} {url}"))
                .flex()
                .gap(Spacing::SM)
                .p(Spacing::MD)
                .border(Borders::THIN)
                .border_color(colors.line)
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(EditorMetrics::CODE_FONT)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(method_hue(&method, cx))
                        .child(method.clone()),
                )
                .child(div().text_color(colors.strong).child(url)),
        )
        .when(!rows.is_empty(), |side| {
            side.child(grid("example-request-row", &rows, |_| false, cx))
        })
        .children(raw.map(|text| {
            text_region("example-request-body")
                .aria_label(text.clone())
                .p(Spacing::MD)
                .border(Borders::THIN)
                .border_color(colors.line)
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(EditorMetrics::CODE_FONT)
                .text_color(colors.foreground)
                .child(text)
        }))
        .child(
            ui::role(
                text_region("example-request-hint"),
                TextVariant::Caption,
                cx,
            )
            .aria_label(REQUEST_HINT)
            .flex()
            .gap(Spacing::SM)
            .child(
                Icon::new(ui::icon(IconName::Info))
                    .size(Fields::CHEVRON)
                    .color(colors.muted),
            )
            .child(REQUEST_HINT),
        )
}

/// The note under the request.
const REQUEST_HINT: &str =
    "Kept as written: variables stay {{name}}, so the example never holds a resolved secret.";

/// Key and value rows `<id>-<n>`; a value `masked` shows as dots with the SECRET badge,
/// `<id>-<n>-secret`.
fn grid(id: &str, rows: &[(String, String)], masked: impl Fn(&str) -> bool, cx: &App) -> Div {
    let colors = bolt::colors(cx);
    div()
        .flex()
        .flex_col()
        .border(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::grid_family(cx))
        .text_size(GridMetrics::FONT)
        .children(rows.iter().enumerate().map(|(index, (key, value))| {
            let n = index + 1;
            let hidden = masked(value);
            let shown = if hidden {
                DOTS.to_owned()
            } else {
                value.clone()
            };
            labeled_row(format!("{id}-{n}"), format!("{key}: {shown}"))
                .flex()
                .items_center()
                .h(Heights::ROW)
                .border_b(Borders::THIN)
                .border_color(colors.row_divider)
                .child(
                    div()
                        .w(tokens::GRID_KEY_COLUMN)
                        .flex_none()
                        .pl(GridMetrics::CELL_PADDING_X)
                        .pr(GridMetrics::CELL_PADDING_X)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.strong)
                        .child(key.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(Spacing::SM)
                        .pl(GridMetrics::CELL_PADDING_X)
                        .pr(GridMetrics::CELL_PADDING_X)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(colors.foreground)
                        .child(shown)
                        .when(hidden, |cell| {
                            cell.child(part(format!("{id}-{n}-secret")).child(secret()))
                        }),
                )
        }))
}

/// The response as saved: the status, the Body and Headers segments, the part chosen and the
/// footer.
fn response_side(tab: &ExampleTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let response = &tab.saved.example.response;
    let status = model::status_line(response.status, response.reason.as_deref());
    let headers: Vec<(String, String)> = response
        .headers
        .iter()
        .map(|header| (header.key.clone(), header.value.clone()))
        .collect();
    let shell = cx.entity().downgrade();
    let segments = segmented::control(
        "example-view",
        Some(SharedString::from("Saved response")),
        true,
        vec![
            SegmentedItem::new("body", "Body")
                .element_id("example-view-body")
                .icon(ui::icon(IconName::Braces)),
            SegmentedItem::new("headers", format!("Headers {}", headers.len()))
                .element_id("example-view-headers")
                .icon(ui::icon(IconName::List)),
        ],
        match tab.view {
            ExampleView::Body => "body",
            ExampleView::Headers => "headers",
        },
        move |id, _window, cx| {
            let view = if id.as_ref() == "headers" {
                ExampleView::Headers
            } else {
                ExampleView::Body
            };
            if let Err(error) = shell.update(cx, |this, cx| this.set_example_view(view, cx)) {
                log::warn!("example view: {error}");
            }
        },
    );
    let content = match tab.view {
        ExampleView::Body => body(tab, cx),
        ExampleView::Headers => div()
            .flex_1()
            .min_h_0()
            .p(Spacing::MD)
            .child(grid("example-header", &headers, model::is_masked, cx))
            .into_any_element(),
    };
    let note = "secrets were masked on save";
    region("example-response")
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::MD)
                .h(tokens::RESULTS_BAR_HEIGHT)
                .pl(Spacing::MD)
                .pr(Spacing::MD)
                .child(caption("example-response-caption", "Response", cx))
                .child(
                    text_region("example-status")
                        .aria_label(status.clone())
                        .font_family(bolt::fonts::editor_family(cx))
                        .font_weight(FontWeight::BOLD)
                        .text_color(class_color(StatusClass::of(response.status), &colors))
                        .child(status),
                )
                .child(div().flex_1())
                .child(segments),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .bg(colors.background)
                .child(content),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::MD)
                .h(tokens::RESULTS_BAR_HEIGHT)
                .pl(Spacing::MD)
                .pr(Spacing::MD)
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .text_color(colors.muted)
                .child(
                    Icon::new(ui::icon(IconName::Shield))
                        .size(Fields::CHEVRON)
                        .color(colors.muted),
                )
                .child(
                    text_region("example-masked-note")
                        .aria_label(note)
                        .whitespace_nowrap()
                        .child(note),
                )
                .child(div().flex_1())
                .child(
                    Button::new("example-copy", "Copy")
                        .secondary()
                        .icon(ui::icon(IconName::Copy))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.copy_open_example_body(cx);
                        })),
                )
                .child(save_body_button("example-save-body", cx)),
        )
}

/// A Save body to file… button `id`.
fn save_body_button(id: &'static str, cx: &mut Context<Shell>) -> Button {
    Button::new(id, "Save body to file\u{2026}")
        .secondary()
        .icon(ui::icon(IconName::FileDown))
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.save_open_example_body(cx);
        }))
}

/// The saved body: numbered lines, the masked ones with the SECRET badge; or what a binary
/// body is, with Save body to file…; or that there is none.
fn body(tab: &ExampleTab, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    match model::shown_body(&tab.saved) {
        ShownBody::Text(lines) => region("example-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(Spacing::SM)
            .pb(Spacing::SM)
            .font_family(bolt::fonts::editor_family(cx))
            .text_size(EditorMetrics::CODE_FONT)
            .children(
                lines
                    .into_iter()
                    .enumerate()
                    .map(|(index, (line, masked))| {
                        let n = index + 1;
                        labeled_row(format!("example-line-{n}"), line.clone())
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
                                    .whitespace_nowrap()
                                    .text_color(colors.foreground)
                                    .child(line),
                            )
                            .when(masked, |row| {
                                row.child(
                                    part(format!("example-line-{n}-secret"))
                                        .ml(Spacing::SM)
                                        .child(secret()),
                                )
                            })
                    }),
            )
            .into_any_element(),
        ShownBody::Binary {
            file,
            content_type,
            bytes,
        } => {
            let kind = if content_type.is_empty() {
                "binary".to_owned()
            } else {
                content_type
            };
            let size = u64::try_from(bytes).unwrap_or(u64::MAX);
            let title = format!("{kind} \u{b7} {}", size_text(size));
            let hint = format!(
                "Kept as received in {}. Binary bodies are not masked.",
                model::body_file(&tab.place.file, &file)
            );
            region("example-binary")
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(Spacing::MD)
                .child(
                    Icon::new(ui::icon(IconName::Image))
                        .size(TabMetrics::ICON)
                        .color(colors.muted),
                )
                .child(
                    text_region("example-binary-title")
                        .aria_label(title.clone())
                        .text_color(colors.strong)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    ui::role(text_region("example-binary-hint"), TextVariant::Caption, cx)
                        .aria_label(hint.clone())
                        .child(hint),
                )
                .child(save_body_button("example-binary-save", cx))
                .into_any_element()
        }
        ShownBody::Empty => ui::role(text_region("example-no-body"), TextVariant::Caption, cx)
            .aria_label("No body was saved.")
            .p(Spacing::MD)
            .child("No body was saved.")
            .into_any_element(),
    }
}
