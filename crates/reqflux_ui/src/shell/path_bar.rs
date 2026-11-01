//! The path bar under the tab row: where the active request sits, as collection › folders ›
//! request; empty with no active request.

use bolt_legacy::tokens::{border, icon, space, text};
use bolt_legacy::{Icon, IconName};
use gpui::{App, Div, ParentElement, Stateful, Styled, div};

use super::documents::Location;
use super::rows::part;
use super::{region, text as text_role};
use crate::tokens;

/// The path bar of `active`, element id `document-path`. Its segments are
/// `path-collection`, `path-folder-<n>` from the outermost folder, and `path-request`.
pub(super) fn render(active: Option<&Location>, cx: &App) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    let bar = text_role::role(region("document-path"), text::BODY_SMALL, cx)
        .flex_none()
        .h(tokens::PATH_BAR)
        .flex()
        .items_center()
        .gap(space::HALF)
        .pl(tokens::PATH_BAR_PADDING_X)
        .pr(tokens::PATH_BAR_PADDING_X)
        .border_t(border::THIN)
        .border_color(colors.line)
        .whitespace_nowrap()
        .overflow_hidden();
    let Some(location) = active else {
        return bar;
    };

    let brand = bolt_legacy::brand(cx);
    let segment = |id: String, icon: Option<Icon>, name: &str| {
        part(id)
            .flex()
            .items_center()
            .gap(space::HALF)
            .text_color(colors.muted)
            .children(icon)
            .child(name.to_owned())
    };
    let separator = || {
        div().flex_none().child(
            Icon::new(IconName::ChevronRight)
                .size(tokens::PATH_SEPARATOR)
                .color(colors.line_2),
        )
    };

    let collection = Icon::new(IconName::Box)
        .size(icon::SM)
        .color(brand.accent_tint);
    let mut bar = bar.child(segment(
        "path-collection".to_owned(),
        Some(collection),
        &location.collection,
    ));
    for (index, folder) in location.folders.iter().enumerate() {
        let icon = Icon::new(IconName::Folder)
            .size(icon::SM)
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
