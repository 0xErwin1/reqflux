//! The Auth pane, as artboard 05 draws it: a label/control row per setting. Source is a
//! segmented control between the parent's auth and the request's own; Type is a select that
//! opens the type menu; every field sits in a field well. Masked values show only `••••••••`
//! with the SECRET badge, on screen and in the automation label. Username, Key and the
//! secrets open a field; Add to is a segmented control. Every edit lands in the request's
//! draft. Other auth types stay read-only.

use bolt_legacy::IconName;
use bolt_legacy::tokens::{border, chrome, family, space, text};
use gpui::{
    AnyElement, App, Context, Div, Entity, ParentElement, Stateful, Styled, Window, div, prelude::*,
};
use gpui_component::input::{Enter, Escape, Input, InputState};
use reqflux_core::model::{ApiKeyLocation, Auth};
use reqflux_engine::Inspection;

use super::body::controls::{segment, select, track, well};
use super::grid::secret_badge;
use super::grid_model::MASK;
use super::pane_model::{AuthField, auth_fields, auth_type};
use super::{auth_menu, code, spans};
use crate::shell::Shell;
use crate::shell::documents::locate;
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// What a click on a field row does.
#[derive(Clone, Copy)]
enum Edit {
    /// Opens the plain field (Username, Key), or the secret field (Password, Token, Value).
    Text { secret: bool },
    /// Nothing: Add to, which its segments edit, or a field of an unsupported type.
    None,
}

fn edit_of(auth: &Auth, label: &str) -> Edit {
    match (auth, label) {
        (Auth::Other { .. }, _) => Edit::None,
        (_, "Username" | "Key") => Edit::Text { secret: false },
        (_, "Password" | "Token" | "Value") => Edit::Text { secret: true },
        _ => Edit::None,
    }
}

/// A form row: the `label` column, then `content` and the hints under it.
fn form_row(row: Stateful<Div>, label: &str, content: Vec<AnyElement>, cx: &App) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    crate::shell::text::role(row, text::BODY, cx)
        .flex()
        .items_start()
        .gap(space::LG)
        .py(tokens::AUTH_ROW_PADDING_Y)
        .child(
            div()
                .w(tokens::AUTH_LABEL_COLUMN)
                .flex_none()
                .h(chrome::CONTROL)
                .flex()
                .items_center()
                .text_color(colors.foreground)
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_start()
                .gap(space::HALF)
                .children(content),
        )
}

/// A muted hint `text` under a control, element `id`.
fn hint(id: &str, text_value: String, cx: &App) -> AnyElement {
    let muted = bolt_legacy::colors(cx).muted;
    crate::shell::text::role(
        labeled_row(id.to_owned(), text_value.clone()),
        text::CAPTION,
        cx,
    )
    .text_color(muted)
    .child(text_value)
    .into_any_element()
}

/// The name of the folder or collection the request `request_id` inherits its auth from.
fn parent_name(shell: &Shell, request_id: &str) -> String {
    locate(&shell.roots, request_id).map_or_else(
        || "parent".to_owned(),
        |location| {
            location
                .folders
                .last()
                .cloned()
                .unwrap_or(location.collection)
        },
    )
}

/// The Source row: `auth-source-inherit` and `auth-source-request`, and, when inherited,
/// the hint `request-auth-hint` naming the parent.
fn source_row(request_id: &str, auth: &Auth, shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    let inherits = matches!(auth, Auth::Inherit);
    let parent = parent_name(shell, request_id);
    let sources = [
        (
            "inherit",
            format!("Inherited from {parent}"),
            IconName::Folder,
            true,
        ),
        ("request", "This request".to_owned(), IconName::Key, false),
    ];
    let segments: Vec<_> = sources
        .into_iter()
        .map(|(id, label, icon_name, inherit)| {
            let request = request_id.to_owned();
            segment(
                format!("auth-source-{id}"),
                label,
                icon_name,
                None,
                inherit == inherits,
                cx,
            )
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.set_auth_source(&request, inherit, window, cx);
            }))
        })
        .collect();
    let mut content = vec![track("request-auth-source", segments, cx).into_any_element()];
    if inherits {
        content.push(hint(
            "request-auth-hint",
            format!("This request sends the auth of {parent}."),
            cx,
        ));
    }
    form_row(
        part("request-auth-source-row".to_owned()),
        "Source",
        content,
        cx,
    )
    .into_any_element()
}

/// The Type row: the select `request-auth-type` and, when open, the type menu under it.
fn type_row(
    request_id: &str,
    auth: &Auth,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let control = div()
        .relative()
        .child(
            select(
                "request-auth-type".to_owned(),
                Some(IconName::Key),
                auth_type(auth).to_owned(),
                cx,
            )
            .w(tokens::AUTH_MENU_WIDTH)
            .on_click(cx.listener(|this, _event, window, cx| {
                this.toggle_auth_menu(window, cx);
            })),
        )
        .when(shell.auth_menu_open, |anchor| {
            anchor.child(auth_menu::render(request_id, auth, shell, window, cx))
        })
        .into_any_element();
    form_row(
        part("request-auth-type-row".to_owned()),
        "Type",
        vec![control],
        cx,
    )
    .into_any_element()
}

/// The open field in a well, `request-auth-input`.
fn input_field(input: &Entity<InputState>, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    part("request-auth-input".to_owned())
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .on_action(cx.listener(|this, action: &Enter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus: it
            // sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            Input::new(input)
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .text_size(tokens::GRID_TEXT)
                .font_family(family::MONO)
                .text_color(colors.strong),
        )
}

/// The Add to segments, `auth-location-header` and `auth-location-query`.
fn location_control(request_id: &str, auth: &Auth, cx: &mut Context<Shell>) -> AnyElement {
    let current = match auth {
        Auth::ApiKey { location, .. } => Some(*location),
        _ => None,
    };
    let segments: Vec<_> = [
        ("header", "Header", ApiKeyLocation::Header, IconName::List),
        ("query", "Query", ApiKeyLocation::Query, IconName::Hash),
    ]
    .into_iter()
    .map(|(id, label, location, icon_name)| {
        let request = request_id.to_owned();
        segment(
            format!("auth-location-{id}"),
            label,
            icon_name,
            None,
            current == Some(location),
            cx,
        )
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.set_api_key_location(&request, location, window, cx);
        }))
    })
    .collect();
    track("request-auth-location", segments, cx).into_any_element()
}

/// The row of `field`, `request-auth-field-<label>`, with its value in the well
/// `request-auth-well-<label>`.
fn field_row(
    request_id: &str,
    inspection: &Inspection,
    auth: &Auth,
    field: &AuthField,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let id = field.element_id();
    let well_id = id.replace("request-auth-field-", "request-auth-well-");
    if field.label == "Add to" {
        let control = location_control(request_id, auth, cx);
        return form_row(
            labeled_row(id, format!("{}: {}", field.label, field.value)),
            &field.label,
            vec![control],
            cx,
        )
        .into_any_element();
    }
    let edit = edit_of(auth, &field.label);
    if let Edit::Text { secret } = edit
        && shell.auth_edit_of(request_id, secret)
        && let Some(input) = shell.cell_edit.as_ref().map(|edit| edit.input.clone())
    {
        let input = input_field(&input, cx);
        let control = well(well_id, cx).w_full().child(input).into_any_element();
        return form_row(
            labeled_row(id, format!("{}: ", field.label)),
            &field.label,
            vec![control],
            cx,
        )
        .into_any_element();
    }
    let (shown, value) = if field.masked {
        let value = part(format!("{well_id}-text"))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(space::SM)
            .child(MASK)
            .child(secret_badge(bolt_legacy::colors(cx)));
        (MASK.to_owned(), value)
    } else {
        let value = part(format!("{well_id}-text"))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .children(spans::render(inspection, &field.value, cx));
        (field.value.clone(), value)
    };
    let control = well(well_id, cx).w_full().child(value).into_any_element();
    let row = form_row(
        labeled_row(id, format!("{}: {shown}", field.label)),
        &field.label,
        vec![control],
        cx,
    );
    match edit {
        Edit::Text { secret } => {
            let request = request_id.to_owned();
            row.cursor_text()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.start_auth_edit(&request, secret, window, cx);
                }))
                .into_any_element()
        }
        Edit::None => row.into_any_element(),
    }
}

/// The pane of `auth` of the request `request_id`, the draft's when the request has one,
/// else the saved file's.
pub(super) fn render(
    request_id: &str,
    inspection: &Inspection,
    auth: &Auth,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let line = bolt_legacy::colors(cx).line;
    let mut rows = vec![source_row(request_id, auth, shell, cx)];
    if !matches!(auth, Auth::Inherit) {
        rows.push(type_row(request_id, auth, shell, window, cx));
    }
    for field in auth_fields(auth, |text| inspection.references_secret(text)) {
        rows.push(field_row(request_id, inspection, auth, &field, shell, cx));
    }
    let form = part("request-auth-form".to_owned())
        .flex()
        .flex_col()
        .pl(space::LG)
        .pr(space::LG)
        .py(border::THICK)
        .border_t(border::THIN)
        .border_color(line)
        .children(rows)
        .into_any_element();
    let note = matches!(auth, Auth::Other { .. }).then(|| {
        code::empty(
            "request-auth-unsupported".to_owned(),
            "Not supported yet",
            cx,
        )
    });
    std::iter::once(form).chain(note).collect()
}
