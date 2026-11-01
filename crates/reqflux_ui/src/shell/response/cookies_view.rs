//! The Cookies sub-tab (response-views/02): a table of every `Set-Cookie` header, name,
//! value, domain, path, expires and flags. A value that holds a secret the send knows (by
//! the send's masker), or that history stored masked, shows as dots with the SECRET badge;
//! every other text goes through the masker too.

use bolt::primitives::{Badge, BadgeTone, Icon};
use bolt::tokens::{Borders, Fields, FontSizes, GridMetrics, Heights, Spacing};
use gpui::{AnyElement, App, Div, FontWeight, ParentElement, Pixels, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_engine::Masker;

use super::cookies::{Cookie, CookieColumns, CookieValue, cookies};
use crate::shell::region;
use crate::shell::request::{Received, secret, ui};
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// What a hidden value shows.
const DOTS: &str = "••••••••••";

/// The cookies of `received`, element id `response-cookies`: the header row
/// `response-cookies-header`, then one row `response-cookie-<n>` each with its cells
/// `response-cookie-<n>-<column>` (`name`, `value`, `domain`, `path`, `expires`, `flags`)
/// and, on a hidden value, the badge `response-cookie-<n>-secret`; a header that does not
/// parse is one cell with a warning mark.
pub(super) fn render(received: &Received, masker: &Masker, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let rows = cookies(&received.headers)
        .into_iter()
        .enumerate()
        .map(|(index, cookie)| {
            let n = index + 1;
            let id = format!("response-cookie-{n}");
            match cookie {
                Cookie::Parsed(cookie) => row(n, cookie.columns(|text| masker.mask(text)), cx),
                Cookie::Raw(raw) => {
                    let raw = masker.mask(&raw);
                    labeled_row(id, format!("Could not parse: {raw}"))
                        .flex()
                        .items_center()
                        .gap(Spacing::SM)
                        .h(Heights::ROW)
                        .pl(Spacing::MD)
                        .border_b(Borders::THIN)
                        .border_color(colors.row_divider)
                        .child(
                            Icon::new(ui::icon(IconName::TriangleAlert))
                                .size(Fields::CHEVRON)
                                .color(colors.warning),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .pr(Spacing::MD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(colors.foreground)
                                .child(raw),
                        )
                        .into_any_element()
                }
            }
        });
    region("response-cookies")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .font_family(bolt::fonts::grid_family(cx))
        .text_size(GridMetrics::FONT)
        .child(header(cx))
        .children(rows)
        .into_any_element()
}

/// The cell of a column `width` wide, or filling the rest when `None`.
fn cell(width: Option<Pixels>) -> Div {
    let cell = div()
        .h_full()
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .pl(GridMetrics::CELL_PADDING_X)
        .pr(GridMetrics::CELL_PADDING_X);
    match width {
        Some(width) => cell.w(width).flex_none(),
        None => cell.flex_1(),
    }
}

/// The column widths after the row number, in order; the flags fill the rest.
const COLUMNS: [(&str, Option<Pixels>); 6] = [
    ("name", Some(tokens::COOKIE_NAME_COLUMN)),
    ("value", Some(tokens::COOKIE_VALUE_COLUMN)),
    ("domain", Some(tokens::COOKIE_DOMAIN_COLUMN)),
    ("path", Some(tokens::COOKIE_PATH_COLUMN)),
    ("expires", Some(tokens::COOKIE_EXPIRES_COLUMN)),
    ("flags", None),
];

/// The header row, `response-cookies-header`, in the data grid's header face.
fn header(cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    part("response-cookies-header".to_owned())
        .flex()
        .items_center()
        .h(bolt::fonts::grid_header_height(cx))
        .bg(colors.background)
        .text_size(FontSizes::XS)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(div().w(GridMetrics::ROW_NUMBER_WIDTH).flex_none())
        .children(
            COLUMNS
                .iter()
                .map(|(name, width)| cell(*width).child(*name)),
        )
        .into_any_element()
}

/// The row of the cookie `n` with `columns`; its label shows the dots, never a hidden value.
fn row(n: usize, columns: CookieColumns, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("response-cookie-{n}");
    let shown_value = match &columns.value {
        CookieValue::Shown(value) => value.clone(),
        CookieValue::Secret => DOTS.to_owned(),
    };
    let label = [
        format!("{}={shown_value}", columns.name),
        columns.domain.clone(),
        columns.path.clone(),
        columns.expires.clone(),
        columns.flags.join(" "),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let text = |column: &str, text: String, color: gpui::Hsla| {
        part(format!("{id}-{column}"))
            .overflow_hidden()
            .text_color(color)
            .child(text)
    };
    let value = match columns.value {
        CookieValue::Shown(value) => text("value", value, colors.foreground).into_any_element(),
        CookieValue::Secret => part(format!("{id}-value"))
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(div().text_color(colors.muted).child(DOTS))
            .child(part(format!("{id}-secret")).child(secret()))
            .into_any_element(),
    };
    let flags = part(format!("{id}-flags"))
        .flex()
        .gap(Spacing::XXS)
        .children(
            columns
                .flags
                .into_iter()
                .map(|flag| Badge::new(flag, BadgeTone::Neutral)),
        );
    let [name, value_cell, domain, path, expires, flags_cell] =
        COLUMNS.map(|(_, width)| cell(width));
    labeled_row(id.clone(), label)
        .flex()
        .items_center()
        .h(Heights::ROW)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .w(GridMetrics::ROW_NUMBER_WIDTH)
                .flex_none()
                .pr(Spacing::MD)
                .text_right()
                .text_color(colors.line_2)
                .child(n.to_string()),
        )
        .child(name.child(text("name", columns.name, colors.strong)))
        .child(value_cell.child(value))
        .child(domain.child(text("domain", columns.domain, colors.foreground)))
        .child(path.child(text("path", columns.path, colors.foreground)))
        .child(expires.child(text("expires", columns.expires, colors.foreground)))
        .child(flags_cell.child(flags))
        .into_any_element()
}
