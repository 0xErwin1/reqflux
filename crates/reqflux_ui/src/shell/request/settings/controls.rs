//! The controls of the Settings pane, as artboard request-panes/01 frame C draws them: a
//! row (label column, control, hint), the Default / On / Off radio group on the sunken track
//! of the Body pane's mode control, the state badge and the Reset link.

use bolt::chamfer::Chamfer;
use bolt::tokens::{border, chrome, cut, family, field, space, text};
use gpui::{
    AnyElement, App, Div, FontWeight, Hsla, ParentElement, Role, SharedString, Stateful, Styled,
    div, prelude::*,
};

use crate::shell::rows::part;
use crate::shell::text as text_role;
use crate::tokens;

/// The state a row's badge shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Badge {
    /// The request uses the default.
    Default,
    /// The request sets its own value.
    Overridden,
    /// The request sets a value that weakens the send, such as TLS verification off.
    Warning,
}

/// The row `id` labeled `label`, with `control` and `hint` under it, reported as
/// `<label>: <shown>`.
pub(super) fn row(
    id: String,
    label: &str,
    shown: &str,
    control: impl IntoElement,
    hint: AnyElement,
    cx: &App,
) -> Stateful<Div> {
    part(id)
        .role(Role::Group)
        .aria_label(format!("{label}: {shown}"))
        .flex()
        .items_start()
        .gap(space::LG)
        .py(space::HALF)
        .child(
            text_role::role(div(), text::BODY, cx)
                .w(tokens::PANE_LABEL_COLUMN)
                .flex_none()
                .pt(space::HALF)
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::HALF)
                .child(control)
                .child(hint),
        )
}

/// The hint `id` under a row's control, in `color`.
pub(super) fn hint(id: String, text: String, color: Hsla) -> AnyElement {
    part(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .text_size(tokens::NOTICE_TEXT)
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The radio group `id`, named `label`, around `choices`.
pub(super) fn radio_group(
    id: String,
    label: &'static str,
    choices: impl IntoIterator<Item = Stateful<Div>>,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    part(id)
        .role(Role::RadioGroup)
        .aria_label(label)
        .relative()
        .flex_none()
        .flex()
        .p(border::THICK)
        .child(
            Chamfer::new(cut::CONTROL)
                .fill(colors.background)
                .edge(colors.line, border::THIN),
        )
        .children(choices)
}

/// A choice `id` named `label` in a radio group, raised when `selected`, with an optional
/// muted `note` after its name.
pub(super) fn choice(
    id: String,
    label: &'static str,
    note: Option<&'static str>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let color = if selected {
        colors.strong
    } else {
        colors.muted
    };
    text_role::role(part(id), text::BODY_SMALL, cx)
        .role(Role::RadioButton)
        .aria_label(label)
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::HALF)
        .h(chrome::STATUS_CHIP)
        .pl(field::PADDING_X)
        .pr(field::PADDING_X)
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(color)
        .when(selected, |choice| {
            choice.child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
        })
        .child(label)
        .children(note.map(|note| {
            div()
                .font_family(family::MONO)
                .text_size(tokens::SMALL_MONO_TEXT)
                .text_color(colors.muted)
                .child(note)
        }))
}

/// The badge `id` showing `badge`.
pub(super) fn badge(id: String, badge: Badge, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let accent = bolt::brand(cx).accent_tint;
    let (label, fill, color) = match badge {
        Badge::Default => ("Default", colors.raised, colors.muted),
        Badge::Overridden => (
            "Overridden",
            tokens::wash(accent, tokens::VARIABLE_WASH),
            accent,
        ),
        Badge::Warning => (
            "Overridden",
            tokens::wash(colors.warning, tokens::VARIABLE_WASH),
            colors.warning,
        ),
    };
    part(id)
        .role(Role::Label)
        .aria_label(label)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .h(tokens::VARIABLES_PILL)
        .pl(space::HALF)
        .pr(space::HALF)
        .font_family(family::INTERFACE)
        .text_size(tokens::BADGE_TEXT)
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(Chamfer::new(cut::KEYCAP).fill(fill))
        .child(label.to_uppercase())
}

/// The link button `id` reading `label`.
pub(super) fn link(id: String, label: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
    let label = label.into();
    part(id)
        .role(Role::Button)
        .aria_label(label.clone())
        .flex_none()
        .text_size(tokens::NOTICE_TEXT)
        .text_color(bolt::brand(cx).accent_tint)
        .cursor_pointer()
        .child(label)
}
