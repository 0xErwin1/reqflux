//! The path bar under the tab row: where the active request sits, as collection › folders ›
//! request; empty with no active request.

use bolt::primitives::{Icon, TextVariant};
use bolt::tokens::{Borders, EditorMetrics, Fields, Spacing};
use gpui::{App, Div, ParentElement, Stateful, Styled, div};
use gpui_kit_assets::IconName;

use super::documents::Location;
use super::region;
use super::request::ui;
use super::rows::part;
use crate::tokens;

/// The path bar of `active`, element id `document-path`. Its segments are
/// `path-collection`, `path-folder-<n>` from the outermost folder, and `path-request`.
pub(super) fn render(active: Option<&Location>, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let bar = ui::role(region("document-path"), TextVariant::BodySm, cx)
        .flex_none()
        .h(tokens::PATH_BAR)
        .flex()
        .items_center()
        .gap(Spacing::XXS)
        .pl(EditorMetrics::BAR_PADDING_X)
        .pr(EditorMetrics::BAR_PADDING_X)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .whitespace_nowrap()
        .overflow_hidden();
    let Some(location) = active else {
        return bar;
    };

    let tint = bolt::brand::brand(cx).accent_tint;
    let segment = |id: String, icon: Option<Icon>, name: &str| {
        part(id)
            .flex()
            .items_center()
            .gap(Spacing::XXS)
            .text_color(colors.muted)
            .children(icon)
            .child(name.to_owned())
    };
    let separator = || {
        div().flex_none().child(
            Icon::new(ui::icon(IconName::ChevronRight))
                .size(tokens::PATH_SEPARATOR)
                .color(colors.line_2),
        )
    };

    let collection = Icon::new(ui::icon(IconName::Box))
        .size(Fields::CHEVRON)
        .color(tint);
    let mut bar = bar.child(segment(
        "path-collection".to_owned(),
        Some(collection),
        &location.collection,
    ));
    for (index, folder) in location.folders.iter().enumerate() {
        let icon = Icon::new(ui::icon(IconName::Folder))
            .size(Fields::CHEVRON)
            .color(colors.muted);
        bar = bar.child(separator()).child(segment(
            format!("path-folder-{index}"),
            Some(icon),
            folder,
        ));
    }
    bar.child(separator()).child(
        segment("path-request".to_owned(), None, &location.name).text_color(colors.foreground),
    )
}
