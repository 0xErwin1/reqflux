//! The controls of the Settings pane, as artboard request-panes/01 frame C draws them: a
//! row (`bolt`'s field row: label column, control, hint), the Default / On / Off radio group
//! on the segmented track of the Body pane's mode control, the state badge and the Reset
//! button.

use bolt::composites::field_row;
use bolt::controls::Button;
use bolt::primitives::{Badge as BoltBadge, BadgeTone, SegmentedControl, SegmentedItem};
use bolt::tokens::{ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, Div, Hsla, ParentElement, Role, SharedString, Stateful, Styled, Window, div,
    prelude::*,
};

use crate::shell::request::segmented::control;
use crate::shell::rows::part;

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
    let column = div()
        .flex()
        .flex_col()
        .gap(Spacing::XXS)
        .child(control)
        .child(hint);
    part(id)
        .role(Role::Group)
        .aria_label(format!("{label}: {shown}"))
        .py(ModalMetrics::FORM_ROW_PADDING_Y)
        .child(field_row(label.to_owned(), column, cx))
}

/// The hint `id` under a row's control, in `color`.
pub(super) fn hint(id: String, text: String, color: Hsla, cx: &App) -> AnyElement {
    part(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .text_size(bolt::density::font_xs(cx))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The radio group `id`, named `label`, of `choices`, `active` raised; `on_select` gets the
/// choice clicked.
pub(super) fn radio_group(
    id: &str,
    label: &'static str,
    choices: Vec<SegmentedItem>,
    active: &'static str,
    on_select: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
) -> SegmentedControl {
    control(id, Some(label.into()), false, choices, active, on_select)
}

/// The choice `part`, element id `id`, named `label` in a radio group, with an optional
/// muted `note` after its name.
pub(super) fn choice(
    part: &'static str,
    id: String,
    label: &'static str,
    note: Option<&'static str>,
) -> SegmentedItem {
    let item = SegmentedItem::new(part, label).element_id(id);
    match note {
        Some(note) => item.count(note),
        None => item,
    }
}

/// The badge `id` showing `badge`.
pub(super) fn badge(id: String, badge: Badge) -> Stateful<Div> {
    let (label, tone) = match badge {
        Badge::Default => ("Default", BadgeTone::Neutral),
        Badge::Overridden => ("Overridden", BadgeTone::Accent),
        Badge::Warning => ("Overridden", BadgeTone::Warning),
    };
    part(id)
        .role(Role::Label)
        .aria_label(label)
        .flex_none()
        .child(BoltBadge::new(label.to_uppercase(), tone))
}

/// The ghost button `id` reading `label`; `on_click` runs on a click.
pub(super) fn link(
    id: String,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static,
) -> Button {
    Button::new(gpui::ElementId::Name(id.into()), label)
        .ghost()
        .inline()
        .on_click(on_click)
}
