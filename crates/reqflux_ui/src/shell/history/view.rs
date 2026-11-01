//! The history tab as design/history/01-sidebar-readonly-tab draws it: the tab chip in the
//! tab row, and the document: the banner with Open source request and Resend, the recorded
//! request line read only, its Params, Headers and Body as sent, and the recorded response.
//!
//! The recorded views draw the request grid's layout and tokens: the grid itself resolves
//! `{{references}}` through an engine inspection, which a recorded send, already resolved and
//! masked, has none of.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::tokens::{Colors, border, chrome, cut, doc_tab, family, icon, space, text};
use bolt_legacy::{Icon, IconName};
use gpui::{
    AnyElement, App, Context, Div, ElementId, FontWeight, ParentElement, Role, Stateful, Styled,
    div, prelude::*,
};
use reqflux_engine::SentBody;

use super::{HistoryTab, RESEND_NOTE, RecordedTab};
use crate::shell::request::accent_keycap;
use crate::shell::rows::part;
use crate::shell::{Shell, region, response, text as text_role, text_region};
use crate::tokens::{self, method_color};

/// The title of the banner while the request exists.
pub(crate) const READ_ONLY: &str = "Recorded request, read only";
/// The caption at the right of the recorded tabs.
pub(crate) const AS_SENT: &str = "Values as sent; variables already resolved";
/// The caption of the banner when the request no longer exists.
pub(crate) const CANNOT_SEND: &str = "This record can no longer be sent.";

/// The history tab chip, `doc-tab-history`: the History icon in the tint, the method, the
/// name, the time `doc-tab-history-time` and the close button `doc-tab-history-close`.
pub(in crate::shell) fn tab_chip(tab: &HistoryTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    let (name_color, weight) = if tab.active {
        (colors.strong, FontWeight::SEMIBOLD)
    } else {
        (colors.muted, text::BODY.weight)
    };
    let close = part("doc-tab-history-close".to_owned())
        .role(Role::Button)
        .aria_label(format!("Close {} from history", tab.name))
        .flex_none()
        .cursor_pointer()
        .child(
            Icon::new(IconName::CircleX)
                .size(doc_tab::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(cx.listener(|this, _event, _window, cx| {
            cx.stop_propagation();
            this.close_history_tab(cx);
        }));
    let method = tab.sent.method.clone();
    text_role::role(part("doc-tab-history".to_owned()), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(format!("{method} {}", tab.name))
        .aria_selected(tab.active)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(doc_tab::GAP)
        .h(chrome::DOC_TAB)
        .pl(doc_tab::PADDING_X)
        .pr(doc_tab::PADDING_X)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(tab.active, |chip| {
            chip.child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .child(
            Icon::new(IconName::RotateCcwClock)
                .size(icon::MD)
                .color(bolt_legacy::brand(cx).accent_tint),
        )
        .child(method_label(&method, cx))
        .child(
            div()
                .text_color(name_color)
                .font_weight(weight)
                .child(tab.name.clone()),
        )
        .child(
            part("doc-tab-history-time".to_owned())
                .role(Role::Label)
                .aria_label(tab.time.clone())
                .font_family(family::MONO)
                .text_size(tokens::TREE_COUNT_SIZE)
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
        .font_family(family::MONO)
        .text_size(tokens::METHOD_LABEL_SIZE)
        .font_weight(FontWeight::BOLD)
        .text_color(method_color(method, bolt_legacy::palette(cx)))
        .child(method.to_owned())
}

/// The banner, `history-banner`: the lock and `Recorded request, read only` with the moment
/// of the send, Open source request `history-open-source` and Resend; or, when the request
/// no longer exists, the warning that it was deleted with Resend disabled and its note.
fn banner(tab: &HistoryTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let (warning, line, strong) = {
        let colors = bolt_legacy::colors(cx);
        (colors.warning, colors.line, colors.strong)
    };
    let (icon_name, icon_color, fill, title, meta) = if tab.source_exists {
        (
            IconName::Lock,
            bolt_legacy::brand(cx).accent_tint,
            bolt_legacy::accent_washes(cx).selected_row,
            READ_ONLY.to_owned(),
            tab.moment.clone(),
        )
    } else {
        (
            IconName::TriangleAlert,
            warning,
            tokens::wash(warning, tokens::HISTORY_BANNER_WASH),
            format!("The source request {} was deleted.", tab.name),
            CANNOT_SEND.to_owned(),
        )
    };
    let open_source = tab.source_exists.then(|| open_source(cx));
    let note = (!tab.source_exists).then(|| {
        text_role::role(text_region("history-resend-note"), text::CAPTION, cx)
            .aria_label(RESEND_NOTE)
            .whitespace_nowrap()
            .child(RESEND_NOTE)
    });
    region("history-banner")
        .flex_none()
        .h(tokens::HISTORY_BANNER)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .bg(fill)
        .border_b(border::THIN)
        .border_color(line)
        .child(Icon::new(icon_name).size(icon::MD).color(icon_color))
        .child(
            text_role::role(text_region("history-banner-title"), text::BODY, cx)
                .aria_label(title.clone())
                .whitespace_nowrap()
                .text_color(strong)
                .child(title),
        )
        .child(
            text_role::role(text_region("history-banner-meta"), text::CAPTION, cx)
                .aria_label(meta.clone())
                .whitespace_nowrap()
                .overflow_hidden()
                .font_family(family::MONO)
                .child(meta),
        )
        .child(div().flex_1())
        .children(open_source)
        .children(note)
        .child(resend(tab.source_exists, cx))
}

/// Open source request, a ghost button with a `line_2` edge.
fn open_source(cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    region("history-open-source")
        .role(Role::Button)
        .aria_label("Open source request")
        .relative()
        .flex_none()
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .cursor_pointer()
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.foreground)
        .child(Chamfer::new(cut::CONTROL).edge(colors.line_2, border::THIN))
        .child(
            Icon::new(IconName::ExternalLink)
                .size(icon::MD)
                .color(colors.foreground),
        )
        .child("Open source request")
        .on_click(cx.listener(|this, _event, _window, cx| this.open_history_source(cx)))
}

/// Resend, `history-resend`, on the accent with its `Ctrl ↵` keycap
/// `history-resend-keycap`; disabled, it sits muted under a blocker,
/// `history-resend-disabled`, that takes the pointer.
fn resend(enabled: bool, cx: &mut Context<Shell>) -> Div {
    let colors = bolt_legacy::colors(cx);
    let brand = bolt_legacy::brand(cx);
    let (fill, color) = if enabled {
        (brand.accent, brand.on_accent)
    } else {
        (colors.raised, colors.muted)
    };
    let mut button = region("history-resend")
        .role(Role::Button)
        .aria_label("Resend")
        .relative()
        .flex_none()
        .h(tokens::URL_FIELD)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .text_size(tokens::REQUEST_LINE_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(Chamfer::new(cut::CONTROL).fill(fill))
        .child(Icon::new(IconName::RotateCcw).size(icon::MD).color(color))
        .child("Resend");
    if enabled {
        button = button
            .cursor_pointer()
            .child(
                part("history-resend-keycap".to_owned())
                    .role(Role::Label)
                    .aria_label("Ctrl ↵")
                    .child(accent_keycap()),
            )
            .on_click(cx.listener(|this, _event, window, cx| this.resend_history(window, cx)));
    }
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
/// both read only.
fn request_line(tab: &HistoryTab, cx: &mut Context<Shell>) -> Div {
    let colors = bolt_legacy::colors(cx);
    let method = tab.sent.method.clone();
    let url = tab.sent.url.clone();
    div()
        .flex_none()
        .h(tokens::REQUEST_LINE)
        .flex()
        .items_center()
        .gap(space::SM)
        .pl(space::MD)
        .pr(space::MD)
        .child(
            part("history-method".to_owned())
                .role(Role::Label)
                .aria_label(method.clone())
                .relative()
                .flex_none()
                .w(tokens::METHOD_CHIP_WIDTH)
                .h(tokens::URL_FIELD)
                .flex()
                .items_center()
                .pl(space::MD)
                .pr(space::SM)
                .child(Chamfer::new(cut::CONTROL).fill(colors.background))
                .child(
                    method_label(&method, cx)
                        .flex_1()
                        .text_size(tokens::REQUEST_LINE_TEXT),
                )
                .child(Icon::new(IconName::Lock).size(icon::SM).color(colors.muted)),
        )
        .child(
            part("history-url".to_owned())
                .role(Role::Label)
                .aria_label(url.clone())
                .relative()
                .flex_1()
                .min_w_0()
                .h(tokens::URL_FIELD)
                .flex()
                .items_center()
                .pl(space::MD)
                .pr(space::MD)
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(family::MONO)
                .text_size(tokens::URL_TEXT)
                .text_color(colors.foreground)
                .child(
                    Chamfer::new(cut::CONTROL)
                        .fill(colors.background)
                        .edge(colors.line, border::THIN),
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
/// `history-tab-<name>-count`, and the caption `history-tabs-note`.
fn recorded_tabs(tab: &HistoryTab, cx: &mut Context<Shell>) -> Div {
    let colors = bolt_legacy::colors(cx);
    let line = colors.line;
    let counts = [
        (RecordedTab::Params, Some(params(&tab.sent.url).len())),
        (RecordedTab::Headers, Some(tab.sent.headers.len())),
        (RecordedTab::Body, None),
    ];
    div()
        .flex_none()
        .h(tokens::SUB_TAB_BAR)
        .flex()
        .items_center()
        .gap(space::XS)
        .pl(space::MD)
        .pr(space::MD)
        .border_t(border::THIN)
        .border_color(line)
        .children(
            counts.map(|(recorded, count)| recorded_tab(recorded, tab.tab == recorded, count, cx)),
        )
        .child(div().flex_1())
        .child(
            text_role::role(text_region("history-tabs-note"), text::CAPTION, cx)
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
    let colors = bolt_legacy::colors(cx);
    let (label, icon_name) = match recorded {
        RecordedTab::Params => ("Params", IconName::Hash),
        RecordedTab::Headers => ("Headers", IconName::List),
        RecordedTab::Body => ("Body", IconName::Braces),
    };
    let (label_color, weight, icon_color) = if selected {
        (
            colors.strong,
            FontWeight::SEMIBOLD,
            bolt_legacy::brand(cx).accent_tint,
        )
    } else {
        (colors.muted, text::BODY.weight, colors.muted)
    };
    let id = format!("history-tab-{}", label.to_lowercase());
    let muted = colors.muted;
    text_role::role(part(id.clone()), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(chrome::DOC_TAB)
        .pl(space::MD)
        .pr(space::MD)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(selected, |chip| {
            chip.child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .child(Icon::new(icon_name).size(icon::MD).color(icon_color))
        .child(
            div()
                .text_color(label_color)
                .font_weight(weight)
                .child(label),
        )
        .children(count.map(|count| {
            part(format!("{id}-count"))
                .role(Role::Label)
                .aria_label(count.to_string())
                .font_family(family::MONO)
                .text_size(tokens::SMALL_MONO_TEXT)
                .text_color(muted)
                .child(count.to_string())
        }))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.select_recorded_tab(recorded, cx);
        }))
}

/// The selected view: the params or headers grid, `history-grid`, or the body,
/// `history-body`.
fn pane(tab: &HistoryTab, cx: &App) -> AnyElement {
    let colors = bolt_legacy::colors(cx);
    match tab.tab {
        RecordedTab::Params => grid("param", "params", &params(&tab.sent.url), colors),
        RecordedTab::Headers => grid("header", "headers", &tab.sent.headers, colors),
        RecordedTab::Body => body(&tab.sent.body, colors),
    }
}

fn grid(kind: &str, table: &str, rows: &[(String, String)], colors: &Colors) -> AnyElement {
    if rows.is_empty() {
        return part("history-grid-empty".to_owned())
            .role(Role::Label)
            .aria_label(format!("No {table}"))
            .flex_none()
            .p(space::MD)
            .text_size(tokens::GRID_TEXT)
            .text_color(colors.muted)
            .child(format!("No {table}"))
            .into_any_element();
    }
    let header = div()
        .flex()
        .items_center()
        .h(tokens::GRID_HEADER)
        .bg(colors.background)
        .font_family(family::MONO)
        .text_size(tokens::GRID_HEADER_TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(div().flex_none().w(tokens::GRID_NUMBER_COLUMN))
        .child(
            div()
                .flex_none()
                .w(tokens::GRID_KEY_COLUMN)
                .flex()
                .gap(space::HALF)
                .child("key")
                .child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_size(tokens::SMALL_MONO_TEXT)
                        .text_color(colors.muted)
                        .child(kind.to_owned()),
                ),
        )
        .child(div().flex_1().child("value"));
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
                .h(tokens::GRID_ROW)
                .border_b(border::THIN)
                .border_color(colors.line)
                .font_family(family::MONO)
                .text_size(tokens::GRID_TEXT)
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_none()
                        .w(tokens::GRID_NUMBER_COLUMN)
                        .pl(space::MD)
                        .text_color(colors.muted)
                        .child(n.to_string()),
                )
                .child(
                    part(format!("history-grid-key-{n}"))
                        .role(Role::Label)
                        .aria_label(key.clone())
                        .flex_none()
                        .w(tokens::GRID_KEY_COLUMN)
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
                        .overflow_hidden()
                        .text_color(colors.foreground)
                        .child(value.clone()),
                )
        }))
        .into_any_element()
}

fn body(body: &SentBody, colors: &Colors) -> AnyElement {
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
        .p(space::MD)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
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
