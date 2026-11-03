//! The Auth pane, as artboard 05 draws it: a label/control row per setting. Source is a
//! segmented control between the parent's auth and the request's own; Type is a select that
//! opens the type menu; every field sits in a field well. Masked values show only `••••••••`
//! with the SECRET badge, on screen and in the automation label. Username, Key and the
//! secrets open a field; Add to is a segmented control. Digest, AWS SigV4 and OAuth 2.0
//! fields open a field too, Algorithm and Grant type are selects, and OAuth 2.0 adds the
//! Token row ([`token_view`]). While Get new token runs the form is locked. Every edit lands
//! in the request's draft. Other auth types stay read-only.

mod token_model;
mod token_view;

use bolt::composites::field_row as bolt_field_row;
use bolt::controls::{GpuiInput, InputEnter, InputEscape, InputState};
use bolt::primitives::{SegmentedItem, TextVariant};
use bolt::tokens::{Borders, Fields, ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, Entity, ParentElement, SharedString, Stateful, Styled, Window,
    div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_core::model::{ApiKeyLocation, Auth, TypedAuth};
use reqflux_engine::Inspection;

use super::body::controls::{select, well};
use super::grid::secret;
use super::grid_model::MASK;
use super::pane_model::{AuthField, AuthMenu, FieldEdit, auth_fields, auth_type};
use super::segmented::control;
use super::{auth_menu, code, spans, ui, variable_popover_view};
use crate::shell::Shell;
use crate::shell::documents::locate;
use crate::shell::rows::{labeled_row, part};
use crate::tokens;

/// A form row: `bolt`'s field row of `label` over `content` and the hints under it.
fn form_row(row: Stateful<Div>, label: &str, content: Vec<AnyElement>, cx: &App) -> Stateful<Div> {
    let control = div()
        .min_w_0()
        .flex()
        .flex_col()
        .items_start()
        .gap(Spacing::XXS)
        .children(content);
    row.py(ModalMetrics::FORM_ROW_PADDING_Y)
        .child(bolt_field_row(label.to_owned(), control, cx))
}

/// A muted hint `text` under a control, element `id`.
fn hint(id: &str, text_value: String, cx: &App) -> AnyElement {
    ui::role(
        labeled_row(id.to_owned(), text_value.clone()),
        TextVariant::Caption,
        cx,
    )
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
        .map(|(id, label, icon_name, _)| {
            SegmentedItem::new(id, label)
                .element_id(format!("auth-source-{id}"))
                .icon(ui::icon(icon_name))
        })
        .collect();
    let request = request_id.to_owned();
    let on_select = cx.listener(move |this, id: &SharedString, window, cx| {
        this.set_auth_source(&request, id.as_ref() == "inherit", window, cx);
    });
    let active = if inherits { "inherit" } else { "request" };
    let mut content = vec![
        control(
            "request-auth-source",
            None,
            true,
            segments,
            active,
            on_select,
        )
        .into_any_element(),
    ];
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

/// Whether the menu `menu` is the open one.
fn menu_open(shell: &Shell, menu: AuthMenu) -> bool {
    shell.auth_menu_open && shell.auth_pane.menu == menu
}

/// The Type row: the select `request-auth-type` and, when open, the type menu under it.
fn type_row(
    request_id: &str,
    auth: &Auth,
    locked: bool,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let trigger = select(
        "request-auth-type".to_owned(),
        Some(IconName::Key),
        auth_type(auth).to_owned(),
    );
    let trigger = if locked {
        trigger.disabled(true)
    } else {
        trigger.on_click(cx.listener(|this, _event, window, cx| {
            this.toggle_auth_menu(window, cx);
        }))
    };
    let control = div()
        .relative()
        .child(div().w(tokens::AUTH_MENU_WIDTH).child(trigger))
        .when(menu_open(shell, AuthMenu::Type), |anchor| {
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
    let colors = bolt::colors(cx);
    part("request-auth-input".to_owned())
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            // Ctrl+Enter maps to the input's `secondary-enter` while the field has focus: it
            // sends instead of committing.
            if action.secondary {
                this.send_active(window, cx);
            } else {
                this.commit_cell_edit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            this.discard_cell_edit(window, cx);
        }))
        .child(
            GpuiInput::new(input)
                .appearance(false)
                .bordered(false)
                .p_0()
                .h_full()
                .text_size(Fields::TEXT)
                .font_family(bolt::fonts::editor_family(cx))
                .text_color(colors.strong),
        )
}

/// The Add to segments, `auth-location-header` and `auth-location-query`.
fn location_control(request_id: &str, auth: &Auth, cx: &mut Context<Shell>) -> AnyElement {
    let current = match auth {
        Auth::ApiKey { location, .. } => Some(*location),
        _ => None,
    };
    let locations = [
        ("header", "Header", ApiKeyLocation::Header, IconName::List),
        ("query", "Query", ApiKeyLocation::Query, IconName::Hash),
    ];
    let segments: Vec<_> = locations
        .into_iter()
        .map(|(id, label, _, icon_name)| {
            SegmentedItem::new(id, label)
                .element_id(format!("auth-location-{id}"))
                .icon(ui::icon(icon_name))
        })
        .collect();
    let active = locations
        .into_iter()
        .find(|(.., location, _)| current == Some(*location))
        .map_or("", |(id, ..)| id);
    let request = request_id.to_owned();
    let on_select = cx.listener(move |this, id: &SharedString, window, cx| {
        if let Some((.., location, _)) = locations
            .into_iter()
            .find(|(location_id, ..)| *location_id == id.as_ref())
        {
            this.set_api_key_location(&request, location, window, cx);
        }
    });
    control(
        "request-auth-location",
        None,
        true,
        segments,
        active,
        on_select,
    )
    .into_any_element()
}

/// The select row of `field`, `request-auth-field-<label>`, with the select
/// `request-auth-<algorithm|grant>` and, when open, its menu under it.
fn select_row(
    request_id: &str,
    auth: &Auth,
    field: &AuthField,
    locked: bool,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let (menu, name) = match field.edit {
        FieldEdit::Select(AuthMenu::Algorithm) => (AuthMenu::Algorithm, "algorithm"),
        _ => (AuthMenu::Grant, "grant"),
    };
    let trigger = select(format!("request-auth-{name}"), None, field.value.clone());
    let trigger = if locked {
        trigger.disabled(true)
    } else {
        trigger.on_click(cx.listener(move |this, _event, window, cx| {
            this.toggle_auth_select(menu, window, cx);
        }))
    };
    let open = match (menu_open(shell, menu), auth.typed()) {
        (true, Some(Ok(TypedAuth::Digest(digest)))) => Some(auth_menu::algorithm(
            request_id,
            digest.algorithm,
            shell,
            window,
            cx,
        )),
        (true, Some(Ok(TypedAuth::OAuth2(oauth)))) => Some(auth_menu::grant(
            request_id,
            oauth.grant_type,
            shell,
            window,
            cx,
        )),
        _ => None,
    };
    let control = div()
        .relative()
        .child(div().w(tokens::AUTH_MENU_WIDTH).child(trigger))
        .children(open)
        .into_any_element();
    let mut content = vec![control];
    content.extend(
        field
            .hint
            .map(|text| hint(&format!("{}-hint", field.element_id()), text.to_owned(), cx)),
    );
    form_row(
        labeled_row(
            field.element_id(),
            format!("{}: {}", field.label, field.value),
        ),
        &field.label,
        content,
        cx,
    )
    .into_any_element()
}

/// Whether the open field is the one `edit` opens for the request `request_id`.
fn editing(shell: &Shell, request_id: &str, auth: &Auth, edit: FieldEdit) -> bool {
    match (auth, edit) {
        (Auth::Other { .. }, FieldEdit::Text(key) | FieldEdit::Secret(key)) => {
            shell.typed_edit_of(request_id, key)
        }
        (_, FieldEdit::Text(_)) => shell.auth_edit_of(request_id, false),
        (_, FieldEdit::Secret(_)) => shell.auth_edit_of(request_id, true),
        _ => false,
    }
}

/// The row of `field`, `request-auth-field-<label>`, with its value in the well
/// `request-auth-well-<label>`.
#[allow(
    clippy::too_many_arguments,
    reason = "one row of the pane, drawn from its parts"
)]
fn field_row(
    request_id: &str,
    inspection: &Inspection,
    auth: &Auth,
    field: &AuthField,
    locked: bool,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let id = field.element_id();
    let well_id = id.replace("request-auth-field-", "request-auth-well-");
    let hints: Vec<AnyElement> = field
        .hint
        .map(|text| hint(&format!("{id}-hint"), text.to_owned(), cx))
        .into_iter()
        .collect();
    match field.edit {
        FieldEdit::Location => {
            let control = location_control(request_id, auth, cx);
            return form_row(
                labeled_row(id, format!("{}: {}", field.label, field.value)),
                &field.label,
                vec![control],
                cx,
            )
            .into_any_element();
        }
        FieldEdit::Select(_) => {
            return select_row(request_id, auth, field, locked, shell, window, cx);
        }
        FieldEdit::Text(_) | FieldEdit::Secret(_) | FieldEdit::None => {}
    }
    if editing(shell, request_id, auth, field.edit)
        && let Some(input) = shell.cell_edit.as_ref().map(|edit| edit.input.clone())
    {
        let text = input_field(&input, cx);
        let control = variable_popover_view::editing_keys(
            well(well_id.clone(), cx).w_full(),
            request_id,
            &well_id,
            &input,
            shell,
            cx,
        )
        .child(text)
        .into_any_element();
        return form_row(
            labeled_row(id, format!("{}: ", field.label)),
            &field.label,
            std::iter::once(control).chain(hints).collect(),
            cx,
        )
        .into_any_element();
    }
    let placeholder = field.placeholder.filter(|_| field.value.is_empty());
    let (shown, value) = if let Some(placeholder) = placeholder {
        let value = part(format!("{well_id}-text"))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Spacing::SM)
            .child(ui::role(div(), TextVariant::Caption, cx).child(placeholder))
            .children(field.masked.then(secret));
        (placeholder.to_owned(), value)
    } else if field.masked {
        let value = part(format!("{well_id}-text"))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Spacing::SM)
            .child(MASK)
            .child(secret());
        (MASK.to_owned(), value)
    } else {
        let value = part(format!("{well_id}-text"))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .children(spans::render_marked(
                inspection,
                &field.value,
                request_id,
                &well_id,
                shell,
                cx,
            ));
        (field.value.clone(), value)
    };
    let control = well(well_id, cx).w_full().child(value).into_any_element();
    let row = form_row(
        labeled_row(id, format!("{}: {shown}", field.label)),
        &field.label,
        std::iter::once(control).chain(hints).collect(),
        cx,
    );
    let (key, secret) = match field.edit {
        FieldEdit::Text(key) => (key, false),
        FieldEdit::Secret(key) => (key, true),
        _ => return row.into_any_element(),
    };
    if locked {
        return row.opacity(LOCKED_OPACITY).into_any_element();
    }
    let request = request_id.to_owned();
    let typed = matches!(auth, Auth::Other { .. });
    row.cursor_text()
        .on_click(cx.listener(move |this, _event, window, cx| {
            if typed {
                this.start_typed_auth_edit(&request, key, secret, window, cx);
            } else {
                this.start_auth_edit(&request, secret, window, cx);
            }
        }))
        .into_any_element()
}

/// The opacity of a field row while Get new token locks the form.
// bolt gap: no disabled or locked state for a form field row.
const LOCKED_OPACITY: f32 = 0.5;

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
    let line = bolt::colors(cx).line;
    let typed = auth.typed();
    let locked = matches!(&typed, Some(Ok(TypedAuth::OAuth2(_))))
        && shell.token_fetching(request_id).is_some();
    let mut rows = vec![source_row(request_id, auth, shell, cx)];
    if !matches!(auth, Auth::Inherit) {
        rows.push(type_row(request_id, auth, locked, shell, window, cx));
    }
    for field in auth_fields(auth, |text| inspection.references_secret(text)) {
        rows.push(field_row(
            request_id, inspection, auth, &field, locked, shell, window, cx,
        ));
    }
    if let Some(Ok(TypedAuth::OAuth2(oauth))) = &typed {
        rows.push(token_view::render(request_id, oauth, shell, cx));
    }
    let form = part("request-auth-form".to_owned())
        .flex()
        .flex_col()
        .pl(Spacing::LG)
        .pr(Spacing::LG)
        .py(Borders::MEDIUM)
        .border_t(Borders::THIN)
        .border_color(line)
        .children(rows)
        .into_any_element();
    let note = match (auth, &typed) {
        (Auth::Other { .. }, None) => Some(code::empty(
            "request-auth-unsupported".to_owned(),
            IconName::Lock,
            "Not supported yet",
        )),
        (_, Some(Err(_))) => Some(code::empty(
            "request-auth-invalid".to_owned(),
            IconName::Lock,
            "These fields are not valid for this auth type",
        )),
        _ => None,
    };
    std::iter::once(form).chain(note).collect()
}
