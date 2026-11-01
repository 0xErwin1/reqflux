//! `IconButton`, a cut button that shows only an icon.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, ClickEvent, Hsla, Role, SharedString, Window, transparent_black};

use super::hover::{GROUP, hover_wash, named};
use crate::chamfer::Chamfer;
use crate::icon::{Icon, IconName};
use crate::theme::colors;
use crate::tokens::{Colors, chrome, cut, icon, icon_button};

/// How an icon button sits on its surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    /// No fill until hovered, muted icon: toolbars and headers.
    #[default]
    Ghost,
    /// On the raised fill with a body icon: the primary icon action of a header.
    Raised,
}

/// Fills and icon color of an icon button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconButtonColors {
    /// Fill at rest.
    pub rest: Hsla,
    /// Wash over the rest fill under the pointer.
    pub hover: Hsla,
    /// Icon color.
    pub icon: Hsla,
}

/// The colors of `variant` in `palette`.
pub fn icon_button_colors(variant: IconButtonVariant, palette: &Colors) -> IconButtonColors {
    match variant {
        IconButtonVariant::Ghost => IconButtonColors {
            rest: transparent_black(),
            hover: palette.hover,
            icon: palette.muted,
        },
        IconButtonVariant::Raised => IconButtonColors {
            rest: palette.raised,
            hover: palette.hover,
            icon: palette.foreground,
        },
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// An icon-only button, `icon_button::WIDTH` by `chrome::CONTROL`, cut `cut::CONTROL`, with
/// an `icon::MD` icon. UI automation finds it by its element id and reads its label.
#[derive(IntoElement)]
pub struct IconButton {
    id: SharedString,
    icon: IconName,
    label: SharedString,
    variant: IconButtonVariant,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    /// A ghost button with element id `id` showing `icon`, named `label`.
    pub fn new(
        id: impl Into<SharedString>,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            variant: IconButtonVariant::Ghost,
            on_click: None,
        }
    }

    /// Sets the variant.
    pub fn variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Puts the button on the raised fill.
    pub fn raised(self) -> Self {
        self.variant(IconButtonVariant::Raised)
    }

    /// Called when the button is clicked.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl std::fmt::Debug for IconButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconButton")
            .field("id", &self.id)
            .field("icon", &self.icon)
            .field("label", &self.label)
            .field("variant", &self.variant)
            .finish_non_exhaustive()
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = icon_button_colors(self.variant, colors(cx));
        named(&self.id, Role::Button, self.label)
            .group(GROUP)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(icon_button::WIDTH)
            .h(chrome::CONTROL)
            .cursor_pointer()
            .child(Chamfer::new(cut::CONTROL).fill(colors.rest))
            .child(hover_wash(cut::CONTROL, colors.hover))
            .child(Icon::new(self.icon).size(icon::MD).color(colors.icon))
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::Colors;

    #[test]
    fn ghost_is_transparent_with_a_muted_icon() {
        let palette = Colors::dark();
        let colors = icon_button_colors(IconButtonVariant::Ghost, &palette);
        assert_eq!(colors.rest.a, 0.0);
        assert_eq!(colors.icon, palette.muted);
        assert_eq!(colors.hover, palette.hover);
    }

    #[test]
    fn raised_sits_on_the_raised_fill_with_a_body_icon() {
        let palette = Colors::dark();
        let colors = icon_button_colors(IconButtonVariant::Raised, &palette);
        assert_eq!(colors.rest, palette.raised);
        assert_eq!(colors.icon, palette.foreground);
        assert_eq!(colors.hover, palette.hover);
    }

    #[test]
    fn the_default_variant_is_ghost() {
        let button = IconButton::new("save", IconName::Save, "Save");
        assert_eq!(button.variant, IconButtonVariant::Ghost);
        assert_eq!(button.raised().variant, IconButtonVariant::Raised);
    }
}
