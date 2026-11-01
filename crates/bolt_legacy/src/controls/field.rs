//! `Field`, a single-line field showing placeholder text.

use gpui::prelude::*;
use gpui::{App, Div, Refineable, Role, SharedString, StyleRefinement, Window, div};

use super::hover::named;
use super::keycap::Keycap;
use crate::chamfer::Chamfer;
use crate::icon::{Icon, IconName};
use crate::theme::colors;
use crate::tokens::{border, chrome, cut, family, field, icon};

/// A `chrome::CONTROL` high field on the `background` well, cut `cut::CONTROL`, with a 1 px
/// `line` edge: an optional leading icon, the placeholder in the muted role, and an
/// optional trailing keycap.
///
/// It only shows its placeholder; text editing comes with the input that replaces it. Size
/// its width like a `div`.
#[derive(IntoElement)]
pub struct Field {
    base: Div,
    id: SharedString,
    placeholder: SharedString,
    label: Option<SharedString>,
    leading_icon: Option<IconName>,
    trailing_keycap: Option<SharedString>,
}

impl Field {
    /// A field with element id `id` showing `placeholder`, which is also its accessible label
    /// unless [`label`](Self::label) replaces it.
    pub fn new(id: impl Into<SharedString>, placeholder: impl Into<SharedString>) -> Self {
        Self {
            base: div(),
            id: id.into(),
            placeholder: placeholder.into(),
            label: None,
            leading_icon: None,
            trailing_keycap: None,
        }
    }

    /// The accessible label, when it should differ from the placeholder.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// An `icon::MD` icon before the placeholder.
    pub fn leading_icon(mut self, icon: IconName) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// A keycap printing `key` at the end, with element id `<id>-keycap`.
    pub fn trailing_keycap(mut self, key: impl Into<SharedString>) -> Self {
        self.trailing_keycap = Some(key.into());
        self
    }

    fn accessible_label(&self) -> SharedString {
        self.label
            .clone()
            .unwrap_or_else(|| self.placeholder.clone())
    }

    fn keycap_id(id: &SharedString) -> SharedString {
        format!("{id}-keycap").into()
    }
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("id", &self.id)
            .field("placeholder", &self.placeholder)
            .field("label", &self.label)
            .field("leading_icon", &self.leading_icon)
            .field("trailing_keycap", &self.trailing_keycap)
            .finish_non_exhaustive()
    }
}

impl Styled for Field {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Field {
    fn render(mut self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let label = self.accessible_label();
        let keycap_id = Self::keycap_id(&self.id);
        let sizing = self.base.style().clone();

        let mut root = named(&self.id, Role::TextInput, label)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(field::GAP)
            .h(chrome::CONTROL)
            .pl(field::PADDING_X)
            .pr(field::PADDING_X)
            .font_family(family::INTERFACE)
            .text_size(field::FONT)
            .child(
                Chamfer::new(cut::CONTROL)
                    .fill(colors.background)
                    .edge(colors.line, border::THIN),
            )
            .when_some(self.leading_icon, |root, name| {
                root.child(Icon::new(name).size(icon::MD).color(colors.muted))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(colors.muted)
                    .child(self.placeholder),
            )
            .when_some(self.trailing_keycap, |root, key| {
                root.child(Keycap::new(keycap_id, key))
            });
        root.style().refine(&sizing);
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_defaults_to_the_placeholder() {
        let field = Field::new("filter", "Filter requests");
        assert_eq!(field.accessible_label(), "Filter requests");
        let field = field.label("Filter the tree");
        assert_eq!(field.accessible_label(), "Filter the tree");
    }

    #[test]
    fn the_trailing_keycap_takes_an_id_under_the_field() {
        assert_eq!(Field::keycap_id(&"filter".into()), "filter-keycap");
    }
}
