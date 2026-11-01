//! `StatusChip`, a status bar entry.

use gpui::prelude::*;
use gpui::{App, Hsla, Role, SharedString, Window, div};

use super::hover::named;
use crate::chamfer::Chamfer;
use crate::icon::{Icon, IconName};
use crate::theme::colors;
use crate::tokens::{chrome, cut, family, icon, status_chip};

/// A `chrome::STATUS_CHIP` high chip on the panel fill, cut `cut::CONTROL`: an optional
/// `icon::CHIP` icon and text in the muted role.
#[derive(IntoElement, Clone, Debug)]
pub struct StatusChip {
    id: SharedString,
    text: SharedString,
    label: Option<SharedString>,
    icon: Option<IconName>,
    icon_color: Option<Hsla>,
}

impl StatusChip {
    /// A chip with element id `id` showing `text`, which is also its accessible label unless
    /// [`label`](Self::label) replaces it.
    pub fn new(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            label: None,
            icon: None,
            icon_color: None,
        }
    }

    /// The accessible label, when it should differ from the text.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// An icon before the text, in the muted role unless [`icon_color`](Self::icon_color)
    /// sets one.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The icon color. Take it from a Bolt role, such as a semantic one for a state.
    pub fn icon_color(mut self, color: impl Into<Hsla>) -> Self {
        self.icon_color = Some(color.into());
        self
    }

    fn accessible_label(&self) -> SharedString {
        self.label.clone().unwrap_or_else(|| self.text.clone())
    }
}

impl RenderOnce for StatusChip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let icon_color = self.icon_color.unwrap_or(colors.muted);
        named(&self.id, Role::Status, self.accessible_label())
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(status_chip::GAP)
            .h(chrome::STATUS_CHIP)
            .pl(status_chip::PADDING_X)
            .pr(status_chip::PADDING_X)
            .font_family(family::INTERFACE)
            .text_size(status_chip::FONT)
            .text_color(colors.muted)
            .whitespace_nowrap()
            .child(Chamfer::new(cut::CONTROL).fill(colors.panel))
            .when_some(self.icon, |chip, name| {
                chip.child(Icon::new(name).size(icon::CHIP).color(icon_color))
            })
            .child(div().child(self.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_defaults_to_the_text() {
        let chip = StatusChip::new("warnings", "2 warnings");
        assert_eq!(chip.accessible_label(), "2 warnings");
        assert_eq!(
            chip.label("Load warnings: 2").accessible_label(),
            "Load warnings: 2"
        );
    }
}
