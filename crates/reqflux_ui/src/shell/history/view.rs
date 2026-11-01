//! The history tab as design/history/01-sidebar-readonly-tab draws it: the tab in the tab
//! row, and the document: the banner with Open source request and Resend, the recorded
//! request line read only, its Params, Headers and Body as sent, and the recorded response.
//!
//! The recorded views draw the request grid's layout and tokens: the grid itself resolves
//! `{{references}}` through an engine inspection, which a recorded send, already resolved and
//! masked, has none of. Everything draws with DBFlux's `bolt`: a document tab, the editor
//! banner, `Button`, `TabMetrics`, `Fields` and `GridMetrics`.

use bolt::composites::{EmptyState, document_tab, document_tab_title};
use bolt::controls::Button;
use bolt::primitives::{Chamfer, Icon, TextVariant};
use bolt::tokens::{Borders, ChamferCut, EditorMetrics, Fields, GridMetrics, Spacing, TabMetrics};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, Div, ElementId, FontWeight, ParentElement, Role, Stateful, Styled,
    div, prelude::*,
};
use reqflux_engine::SentBody;

use super::{HistoryTab, RESEND_NOTE, RecordedTab};
use crate::shell::request::ui;
use crate::shell::rows::{labeled_row, part};
use crate::shell::{Shell, region, response, text_region};
use crate::tokens::{self, method_hue};

/// The title of the banner while the request exists.
pub(crate) const READ_ONLY: &str = "Recorded request, read only";
/// The caption at the right of the recorded tabs.
pub(crate) const AS_SENT: &str = "Values as sent; variables already resolved";
/// The caption of the banner when the request no longer exists.
pub(crate) const CANNOT_SEND: &str = "This record can no longer be sent.";

/// The history tab, `doc-tab-history`, a `bolt` document tab: the History icon in the tint,
/// the method, the name, the time `doc-tab-history-time` and the close button
/// `doc-tab-history-close`.
pub(in crate::shell) fn tab_chip(tab: &HistoryTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let close = part("doc-tab-history-close".to_owned())
        .role(Role::Button)
        .aria_label(format!("Close {} from history", tab.name))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(
            Icon::new(ui::icon(IconName::CircleX))
                .size(TabMetrics::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(cx.listener(|this, _event, _window, cx| {
            cx.stop_propagation();
            this.close_history_tab(cx);
        }));
    let method = tab.sent.method.clone();
    let title = document_tab_title(tab.name.clone(), tab.active, cx);
    let title = if tab.active {
        title
    } else {
        title.font_weight(FontWeight::NORMAL)
    };
    document_tab("doc-tab-history", tab.active, cx)
        .role(Role::Tab)
        .aria_label(format!("{method} {}", tab.name))
        .aria_selected(tab.active)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            Icon::new(ui::icon(IconName::RotateCcwClock))
                .size(TabMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(method_label(&method, cx))
        .child(title)
        .child(
            part("doc-tab-history-time".to_owned())
                .role(Role::Label)
                .aria_label(tab.time.clone())
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TabMetrics::RESULT_META_SIZE)
                .text_color(colors.muted)
                .child(tab.time.clone()),
        )
        .child(close)
        .on_click(cx.listener(|this, _event, _window, cx| this.activate_history_tab(cx)))
}

/// The document of the history tab, `history-readonly`.
pub(in crate::shell) fn render(
    shell: &Shell,
    tab: &HistoryTab,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let key = tab.key();
    region("history-readonly")
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(banner(tab, cx))
        .child(request_line(tab, cx))
        .child(recorded_tabs(tab, cx))
        .child(pane(tab, cx))
        .child(response::render(&key, shell.sends.get(&key), cx))
}

fn method_label(method: &str, cx: &App) -> Div {
    div()
        .flex_none()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(tokens::METHOD_LABEL_SIZE)
        .font_weight(FontWeight::BOLD)
        .text_color(method_hue(method, cx))
        .child(method.to_owned())
}

/// The banner, `history-banner`, DBFlux's editor banner: the lock and `Recorded request, read
/// only` with the moment of the send, Open source request `history-open-source` and Resend;
/// or, when the request no longer exists, the warning that it was deleted with Resend
/// disabled and its note.
fn banner(tab: &HistoryTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let (icon_name, color, title, meta) = if tab.source_exists {
        (
            IconName::Lock,
            bolt::brand::brand(cx).accent_tint,
            READ_ONLY.to_owned(),
            tab.moment.clone(),
        )
    } else {
        (
            IconName::TriangleAlert,
            colors.warning,
            format!("The source request {} was deleted.", tab.name),
            CANNOT_SEND.to_owned(),
        )
    };
    let open_source = tab.source_exists.then(|| open_source(cx));
    let note = (!tab.source_exists).then(|| {
        ui::role(text_region("history-resend-note"), TextVariant::Caption, cx)
            .aria_label(RESEND_NOTE)
            .whitespace_nowrap()
            .child(RESEND_NOTE)
    });
    region("history-banner")
        .flex_none()
        .h(EditorMetrics::BANNER_HEIGHT)
        .flex()
        .items_center()
        .gap(EditorMetrics::BANNER_GAP)
        .pl(EditorMetrics::BANNER_PADDING_X)
        .pr(EditorMetrics::BANNER_PADDING_X)
        .bg(color.opacity(EditorMetrics::BANNER_FILL_ALPHA))
        .border_b(Borders::THIN)
        .border_color(color.opacity(EditorMetrics::BANNER_LINE_ALPHA))
        .child(
            Icon::new(ui::icon(icon_name))
                .size(EditorMetrics::BANNER_ICON)
                .color(color),
        )
        .child(
            ui::role(text_region("history-banner-title"), TextVariant::BodySm, cx)
                .aria_label(title.clone())
                .whitespace_nowrap()
                .text_color(colors.strong)
                .child(title),
        )
        .child(
            ui::role(text_region("history-banner-meta"), TextVariant::Code, cx)
                .aria_label(meta.clone())
                .text_size(EditorMetrics::FILE_FONT)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(meta),
        )
        .child(div().flex_1())
        .children(open_source)
        .children(note)
        .child(resend(tab.source_exists, cx))
}

/// Open source request, a secondary button.
fn open_source(cx: &mut Context<Shell>) -> Button {
    Button::new("history-open-source", "Open source request")
        .secondary()
        .inline()
        .icon(ui::icon(IconName::ExternalLink))
        .on_click(cx.listener(|this, _event, _window, cx| this.open_history_source(cx)))
}

/// Resend, `history-resend`, the primary button with its `Ctrl ↵` keycap; disabled, it sits
/// under a blocker, `history-resend-disabled`, that takes the pointer.
fn resend(enabled: bool, cx: &mut Context<Shell>) -> Div {
    let button = Button::new("history-resend", "Resend")
        .aria_label("Resend")
        .primary()
        .inline()
        .icon(ui::icon(IconName::RotateCcw))
        .disabled(!enabled);
    let button = if enabled {
        button
            .kbd("Ctrl ↵")
            .on_click(cx.listener(|this, _event, window, cx| this.resend_history(window, cx)))
    } else {
        button
    };
    div()
        .relative()
        .flex_none()
        .child(button)
        .when(!enabled, |element| {
            element.child(
                div()
                    .id(ElementId::Name("history-resend-disabled".into()))
                    .debug_selector(|| "history-resend-disabled".to_owned())
                    .absolute()
                    .inset_0()
                    .occlude(),
            )
        })
}

/// The recorded method `history-method` with a lock, and the URL `history-url` as sent,
/// both read only, in the request line's fields.
fn request_line(tab: &HistoryTab, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let method = tab.sent.method.clone();
    let url = tab.sent.url.clone();
    div()
        .flex_none()
        .h(tokens::REQUEST_LINE)
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .child(
            part("history-method".to_owned())
                .role(Role::Label)
                .aria_label(method.clone())
                .relative()
                .flex_none()
                .w(tokens::METHOD_CHIP_WIDTH)
                .h(Fields::HEIGHT)
                .flex()
                .items_center()
                .gap(Fields::GAP)
                .pl(Fields::PADDING_X)
                .pr(Fields::PADDING_X)
                .child(Chamfer::new(ChamferCut::CONTROL).fill(colors.background))
                .child(method_label(&method, cx).flex_1().text_size(Fields::TEXT))
                .child(
                    Icon::new(ui::icon(IconName::Lock))
                        .size(Fields::CHEVRON)
                        .color(colors.muted),
                ),
        )
        .child(
            ui::role(part("history-url".to_owned()), TextVariant::Code, cx)
                .role(Role::Label)
                .aria_label(url.clone())
                .relative()
                .flex_1()
                .min_w_0()
                .h(Fields::HEIGHT)
                .flex()
                .items_center()
                .pl(Fields::PADDING_X)
                .pr(Fields::PADDING_X)
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    Chamfer::new(ChamferCut::CONTROL)
                        .fill(colors.background)
                        .border(colors.line),
                )
                .child(url),
        )
}

/// The query pairs of `url` as sent, undecoded: `(key, value)`, the value empty without `=`.
fn params(url: &str) -> Vec<(String, String)> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    let query = query.split_once('#').map_or(query, |(query, _)| query);
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect()
}

/// The Params, Headers and Body tabs, `history-tab-<name>`, with their counts
/// `history-tab-<name>-count`, and the caption `history-tabs-note`: the request's sub-tab
/// bar.
fn recorded_tabs(tab: &HistoryTab, cx: &mut Context<Shell>) -> Div {
    let line = bolt::colors(cx).line;
    let counts = [
        (RecordedTab::Params, Some(params(&tab.sent.url).len())),
        (RecordedTab::Headers, Some(tab.sent.headers.len())),
        (RecordedTab::Body, None),
    ];
    div()
        .flex_none()
        .h(TabMetrics::INLINE_TAB_HEIGHT)
        .flex()
        .items_center()
        .gap(TabMetrics::DOCUMENT_BAR_GAP)
        .pl(TabMetrics::INLINE_BAR_PADDING_X)
        .pr(TabMetrics::INLINE_BAR_PADDING_X)
        .border_t(Borders::THIN)
        .border_color(line)
        .children(
            counts.map(|(recorded, count)| recorded_tab(recorded, tab.tab == recorded, count, cx)),
        )
        .child(div().flex_1())
        .child(
            ui::role(text_region("history-tabs-note"), TextVariant::Caption, cx)
                .aria_label(AS_SENT)
                .whitespace_nowrap()
                .child(AS_SENT),
        )
}

fn recorded_tab(
    recorded: RecordedTab,
    selected: bool,
    count: Option<usize>,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let (label, icon_name) = match recorded {
        RecordedTab::Params => ("Params", IconName::Hash),
        RecordedTab::Headers => ("Headers", IconName::List),
        RecordedTab::Body => ("Body", IconName::Braces),
    };
    let icon_color = if selected {
        bolt::brand::brand(cx).accent_tint
    } else {
        colors.muted
    };
    let id = format!("history-tab-{}", label.to_lowercase());
    document_tab(ElementId::Name(id.clone().into()), selected, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .child(
            Icon::new(ui::icon(icon_name))
                .size(TabMetrics::ICON)
                .color(icon_color),
        )
        .child(document_tab_title(label, selected, cx))
        .children(count.map(|count| {
            part(format!("{id}-count"))
                .role(Role::Label)
                .aria_label(count.to_string())
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TabMetrics::RESULT_META_SIZE)
                .text_color(colors.muted)
                .child(count.to_string())
        }))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.select_recorded_tab(recorded, cx);
        }))
}

/// The selected view: the params or headers grid, `history-grid`, or the body,
/// `history-body`.
fn pane(tab: &HistoryTab, cx: &App) -> AnyElement {
    match tab.tab {
        RecordedTab::Params => grid("param", "params", &params(&tab.sent.url), cx),
        RecordedTab::Headers => grid("header", "headers", &tab.sent.headers, cx),
        RecordedTab::Body => body(&tab.sent.body, cx),
    }
}

/// The grid of `rows`: `bolt`'s grid header and rows with the request grid's key column.
fn grid(kind: &str, table: &str, rows: &[(String, String)], cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    if rows.is_empty() {
        let text = format!("No {table}");
        return labeled_row("history-grid-empty".to_owned(), text.clone())
            .py(Spacing::XL)
            .child(EmptyState::new(ui::icon(IconName::List), text))
            .into_any_element();
    }
    let mono = bolt::fonts::editor_family(cx);
    let header = div()
        .flex()
        .items_center()
        .h(GridMetrics::HEADER_HEIGHT)
        .bg(colors.background)
        .font_family(mono.clone())
        .text_size(GridMetrics::FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(div().flex_none().w(GridMetrics::ROW_NUMBER_WIDTH))
        .child(
            div()
                .flex_none()
                .w(tokens::GRID_CELL_KEY_COLUMN)
                .pl(GridMetrics::CELL_PADDING_X)
                .pr(GridMetrics::CELL_PADDING_X)
                .flex()
                .gap(GridMetrics::HEADER_GAP)
                .child("key")
                .child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_size(GridMetrics::TYPE_FONT)
                        .text_color(colors.muted)
                        .child(kind.to_owned()),
                ),
        )
        .child(
            div()
                .flex_1()
                .pl(GridMetrics::CELL_PADDING_X)
                .pr(GridMetrics::CELL_PADDING_X)
                .child("value"),
        );
    region("history-grid")
        .flex_none()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(header)
        .children(rows.iter().enumerate().map(|(index, (key, value))| {
            let n = index + 1;
            div()
                .flex()
                .items_center()
                .h(GridMetrics::ROW_HEIGHT)
                .border_b(Borders::THIN)
                .border_color(colors.row_divider)
                .font_family(mono.clone())
                .text_size(GridMetrics::FONT)
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_none()
                        .w(GridMetrics::ROW_NUMBER_WIDTH)
                        .pl(GridMetrics::CELL_PADDING_X)
                        .pr(GridMetrics::CELL_PADDING_X)
                        .text_color(colors.muted)
                        .child(n.to_string()),
                )
                .child(
                    part(format!("history-grid-key-{n}"))
                        .role(Role::Label)
                        .aria_label(key.clone())
                        .flex_none()
                        .w(tokens::GRID_CELL_KEY_COLUMN)
                        .pl(GridMetrics::CELL_PADDING_X)
                        .pr(GridMetrics::CELL_PADDING_X)
                        .overflow_hidden()
                        .text_color(colors.strong)
                        .child(key.clone()),
                )
                .child(
                    part(format!("history-grid-value-{n}"))
                        .role(Role::Label)
                        .aria_label(value.clone())
                        .flex_1()
                        .min_w_0()
                        .pl(GridMetrics::CELL_PADDING_X)
                        .pr(GridMetrics::CELL_PADDING_X)
                        .overflow_hidden()
                        .text_color(colors.foreground)
                        .child(value.clone()),
                )
        }))
        .into_any_element()
}

fn body(body: &SentBody, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let (text, color) = match body {
        SentBody::None => ("No body".to_owned(), colors.muted),
        SentBody::Text(text) => (text.clone(), colors.foreground),
        SentBody::Omitted(description) => (description.clone(), colors.muted),
    };
    part("history-body".to_owned())
        .role(Role::Label)
        .aria_label(text.clone())
        .flex_none()
        .overflow_y_scroll()
        .p(Spacing::MD)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(EditorMetrics::CODE_FONT)
        .line_height(EditorMetrics::CODE_LINE_HEIGHT)
        .text_color(color)
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::params;

    #[test]
    fn params_are_the_query_pairs_as_sent() {
        assert!(params("https://api/v1").is_empty());
        assert_eq!(
            params("https://api/v1?expand=lines&flag&q=a%20b#top"),
            vec![
                ("expand".to_owned(), "lines".to_owned()),
                ("flag".to_owned(), String::new()),
                ("q".to_owned(), "a%20b".to_owned()),
            ]
        );
    }
}
