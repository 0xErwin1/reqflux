//! Drawing Workspace > Cookies (design/network/01-settings-sections, States: Cookie jar,
//! Filtered, Deleted one, Clear all, Empty jar, Add cookie, Edit cookie): the title row with
//! `cookies-add` and `cookies-clear`, the
//! filter `cookies-filter` and its count `cookies-count`, and the table `cookies-table` with
//! the header `cookies-header` and a row `cookie-<n>` per cookie, its cells
//! `cookie-<n>-<column>` (`domain`, `path`, `name`, `value`, `expires`, `flags`), the HOST
//! badge `cookie-<n>-host`, on a hidden value the SECRET badge `cookie-<n>-secret`, and its
//! pencil `cookie-<n>-edit` and trash button `cookie-<n>-delete`. An empty jar shows `cookies-empty`; a filter that
//! matches nothing `cookies-no-match`. Clear all asks first in `cookies-clear-dialog`.
//!
//! It draws with `bolt`: `Button`, `Input`, `Badge`, `EmptyState` and `Modal`, the table in
//! the data grid's metrics as the response Cookies view draws it.

use bolt::composites::EmptyState;
use bolt::controls::{Button, Input};
use bolt::modals::{Modal, modal_lead};
use bolt::primitives::{Badge, BadgeTone, TextVariant};
use bolt::tokens::{Borders, GridMetrics, HeaderMetrics, Heights, Spacing};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, ParentElement, Pixels, Point, Role, Styled, Window,
    anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::Workspace;

use super::{CookieValue, JarRow, count_text};
use crate::shell::request::{secret, ui};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{Shell, region, text_region, toasts};
use crate::tokens;

/// What a hidden value shows.
const DOTS: &str = "••••••••••";

/// The column widths after the row number, in order; the flags fill the rest before the
/// trash column.
const COLUMNS: [(&str, Option<Pixels>); 6] = [
    ("domain", Some(tokens::COOKIE_DOMAIN_COLUMN)),
    ("path", Some(tokens::COOKIE_PATH_COLUMN)),
    ("name", Some(tokens::COOKIE_NAME_COLUMN)),
    ("value", Some(tokens::COOKIE_VALUE_COLUMN)),
    ("expires", Some(tokens::COOKIE_EXPIRES_COLUMN)),
    ("flags", None),
];

/// The content island of Workspace > Cookies.
pub(crate) fn content(
    shell: &Shell,
    workspace: &Workspace,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Div {
    let colors = bolt::colors(cx);
    let cookies = shell.jar_cookies();
    let rows = shell.jar_rows(cx);
    let name = workspace.manifest.name.clone();
    let description = format!(
        "Stored for {name} on this machine and sent with every matching request. Never \
         written to the workspace."
    );
    let title = div()
        .flex_none()
        .flex()
        .items_start()
        .gap(Spacing::MD)
        .pt(HeaderMetrics::SECTION_PADDING_TOP)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .pb(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(HeaderMetrics::SECTION_GAP)
                .child(
                    ui::role(text_region("settings-heading"), TextVariant::Title, cx)
                        .aria_label("Cookies")
                        .child("Cookies"),
                )
                .child(
                    ui::role(text_region("settings-description"), TextVariant::Body, cx)
                        .aria_label(description.clone())
                        .text_color(colors.muted)
                        .child(description),
                ),
        )
        .child(
            Button::new("cookies-add", "Add cookie")
                .primary()
                .icon(ui::icon(IconName::Plus))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.open_cookie_add(window, cx);
                })),
        )
        .child(
            Button::new("cookies-clear", "Clear all…")
                .secondary()
                .icon(ui::icon(IconName::Trash))
                .disabled(cookies.is_empty())
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.ask_clear_cookie_jar(window, cx);
                })),
        );
    let body = if cookies.is_empty() {
        empty(&name)
    } else {
        let filtered = !shell.settings.cookies.filter_text(cx).trim().is_empty();
        let count = count_text(&cookies, rows.len(), filtered);
        let field = shell.settings.cookies.filter.as_ref().map(|filter| {
            Input::new(filter)
                .id("cookies-filter-input")
                .aria_label("Filter by domain or name")
        });
        let no_match = rows.is_empty().then(|| {
            ui::role(text_region("cookies-no-match"), TextVariant::Caption, cx)
                .aria_label("No cookies match the filter")
                .py(Spacing::SM)
                .child("No cookies match the filter")
        });
        let table_rows: Vec<AnyElement> = rows.into_iter().map(|row| jar_row(row, cx)).collect();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .pl(HeaderMetrics::SECTION_PADDING_X)
            .pr(HeaderMetrics::SECTION_PADDING_X)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(Spacing::MD)
                    .py(Spacing::MD)
                    .child(
                        region("cookies-filter")
                            .flex_none()
                            .w(tokens::COOKIE_DOMAIN_COLUMN + tokens::COOKIE_EXPIRES_COLUMN)
                            .children(field),
                    )
                    .child(
                        ui::role(text_region("cookies-count"), TextVariant::Caption, cx)
                            .aria_label(count.clone())
                            .child(count),
                    ),
            )
            .child(
                region("cookies-table")
                    .role(Role::Table)
                    .aria_label("Cookies")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .font_family(bolt::fonts::grid_family(cx))
                    .text_size(GridMetrics::FONT)
                    .child(header(cx))
                    .children(table_rows)
                    .children(no_match),
            )
            .into_any_element()
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(title)
        .child(body)
        .children(clear_dialog(shell, cookies.len(), &name, window, cx))
        .children(super::edit_view::dialog(shell, window, cx))
}

/// `modal` over the whole window, above the page.
pub(super) fn overlay(modal: Modal, window: &Window) -> AnyElement {
    let viewport = window.viewport_size();
    div()
        .absolute()
        .top_0()
        .left_0()
        .child(
            deferred(
                anchored().position(Point::default()).child(
                    div()
                        .relative()
                        .w(viewport.width)
                        .h(viewport.height)
                        .child(modal),
                ),
            )
            .with_priority(3),
        )
        .into_any_element()
}

/// The actions column: the pencil and the trash button.
fn actions_width() -> Pixels {
    tokens::GRID_ACTIONS_COLUMN * 2.0
}

/// `cookies-empty`: no cookies yet, or after Clear all.
fn empty(name: &str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .child(
            region("cookies-empty")
                .role(Role::Label)
                .aria_label(format!("No cookies in {name}"))
                .child(
                    EmptyState::new(
                        ui::icon(IconName::Cookie),
                        "Cookies that responses set land here and go out with the next \
                         matching request. reqflux run keeps its own jar for the run only.",
                    )
                    .title(format!("No cookies in {name}")),
                ),
        )
        .into_any_element()
}

/// The cell of a column `width` wide, or filling the rest when `None`.
fn cell(width: Option<Pixels>) -> Div {
    let cell = div()
        .h_full()
        .flex()
        .items_center()
        .gap(Spacing::XS)
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

/// The header row, `cookies-header`, in the data grid's header face.
fn header(cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    part("cookies-header".to_owned())
        .flex()
        .items_center()
        .h(bolt::fonts::grid_header_height(cx))
        .bg(colors.background)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(div().w(GridMetrics::ROW_NUMBER_WIDTH).flex_none())
        .children(
            COLUMNS
                .iter()
                .map(|(name, width)| cell(*width).child(*name)),
        )
        .child(div().w(actions_width()).flex_none())
        .into_any_element()
}

/// The row `cookie-<n>`; its label shows the dots, never a hidden value.
fn jar_row(row: JarRow, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("cookie-{}", row.n);
    let shown_value = match &row.value {
        CookieValue::Shown(value) => value.clone(),
        CookieValue::Secret => DOTS.to_owned(),
    };
    let label = [
        format!("{}={shown_value}", row.name),
        row.domain.clone(),
        row.path.clone(),
        row.expires.clone(),
        row.flags.join(" "),
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
    let value = match row.value {
        CookieValue::Shown(value) => text("value", value, colors.foreground).into_any_element(),
        CookieValue::Secret => part(format!("{id}-value"))
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(div().text_color(colors.muted).child(DOTS))
            .child(part(format!("{id}-secret")).child(secret()))
            .into_any_element(),
    };
    let host = row
        .host_only
        .then(|| part(format!("{id}-host")).child(Badge::new("HOST", BadgeTone::Neutral)));
    let flags = part(format!("{id}-flags"))
        .flex()
        .gap(Spacing::XXS)
        .children(
            row.flags
                .iter()
                .map(|flag| Badge::new(flag.clone(), BadgeTone::Neutral)),
        );
    let identity = (row.domain.clone(), row.path.clone(), row.name.clone());
    let (domain, path, name) = identity.clone();
    let delete = Button::new(
        gpui::ElementId::Name(format!("{id}-delete").into()),
        format!("Delete cookie {}", row.name),
    )
    .ghost()
    .inline()
    .icon(ui::icon(IconName::Trash))
    .icon_only()
    .on_click(cx.listener(move |this, _event, _window, cx| {
        this.delete_jar_cookie(&domain, &path, &name, cx);
    }));
    let edit = Button::new(
        gpui::ElementId::Name(format!("{id}-edit").into()),
        format!("Edit cookie {}", row.name),
    )
    .ghost()
    .inline()
    .icon(ui::icon(IconName::Pencil))
    .icon_only()
    .on_click(cx.listener(move |this, _event, window, cx| {
        let (domain, path, name) = &identity;
        this.open_cookie_edit((domain, path, name), window, cx);
    }));
    let [
        domain_cell,
        path_cell,
        name_cell,
        value_cell,
        expires_cell,
        flags_cell,
    ] = COLUMNS.map(|(_, width)| cell(width));
    labeled_row(id.clone(), label)
        .role(Role::Row)
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
                .child(row.n.to_string()),
        )
        .child(
            domain_cell
                .child(text("domain", row.domain, colors.foreground))
                .children(host),
        )
        .child(path_cell.child(text("path", row.path, colors.foreground)))
        .child(name_cell.child(text("name", row.name, colors.strong)))
        .child(value_cell.child(value))
        .child(expires_cell.child(text("expires", row.expires, colors.foreground)))
        .child(flags_cell.child(flags))
        .child(
            div()
                .w(actions_width())
                .flex_none()
                .flex()
                .child(edit)
                .child(delete),
        )
        .into_any_element()
}

/// The Clear all confirmation, `cookies-clear-dialog`, while open: `bolt`'s danger modal
/// with `cookies-clear-cancel` and `cookies-clear-confirm`, which names the count.
fn clear_dialog(
    shell: &Shell,
    count: usize,
    name: &str,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    if !shell.settings.cookies.confirm {
        return None;
    }
    let title = format!("Clear the cookie jar of {name}?");
    let lead = format!(
        "Deletes all {} on this machine. Sessions they hold end, so the next requests may \
         need to sign in again. Other workspaces keep their cookies.",
        toasts::counted(count, "cookie")
    );
    let footer = div()
        .flex()
        .gap(Spacing::SM)
        .child(
            Button::new("cookies-clear-cancel", "Cancel")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.cancel_clear_cookie_jar(cx);
                })),
        )
        .child(
            Button::new(
                "cookies-clear-confirm",
                format!("Delete {}", toasts::counted(count, "cookie")),
            )
            .danger()
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.confirm_clear_cookie_jar(cx);
            })),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(title.clone())
        .id("cookies-clear-backdrop")
        .danger()
        .icon(ui::icon(IconName::Trash))
        .focus_handle(shell.settings.cookies.focus.handle())
        .body(
            region("cookies-clear-dialog")
                .role(Role::Dialog)
                .aria_label(title)
                .child(modal_lead(lead, cx)),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.cancel_clear_cookie_jar(cx)) {
                log::warn!("cookie jar clear close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.confirm_clear_cookie_jar(cx))
            {
                log::warn!("cookie jar clear: {error}");
            }
        });
    Some(overlay(modal, window))
}
