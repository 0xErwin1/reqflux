//! `RailButton`, a view button of the activity rail.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, ClickEvent, FontWeight, Hsla, Role, SharedString, Window, div, transparent_black};

use super::hover::{GROUP, hover_wash, named};
use crate::brand::Brand;
use crate::chamfer::Chamfer;
use crate::icon::{Icon, IconName};
use crate::theme::{brand, colors};
use crate::tokens::{Colors, accent_wash, cut, family, icon, rail};

/// Fills and icon color of a rail button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailButtonColors {
    /// Fill at rest.
    pub rest: Hsla,
    /// Wash over the rest fill under the pointer.
    pub hover: Hsla,
    /// Icon color.
    pub icon: Hsla,
}

/// The colors of a rail button: the active one on the accent tint at
/// [`accent_wash::ACTIVE_NAV`] with a tint icon, the others transparent with a muted icon.
pub fn rail_button_colors(active: bool, palette: &Colors, brand: &Brand) -> RailButtonColors {
    if active {
        RailButtonColors {
            rest: Hsla {
                a: accent_wash::ACTIVE_NAV,
                ..brand.accent_tint
            },
            hover: palette.hover,
            icon: brand.accent_tint,
        }
    } else {
        RailButtonColors {
            rest: transparent_black(),
            hover: palette.hover,
            icon: palette.muted,
        }
    }
}

/// The text of a count badge: nothing for no count or zero, the count up to
/// `rail::BADGE_MAX`, and the maximum and a plus above it.
pub fn badge_text(count: Option<usize>) -> Option<SharedString> {
    match count {
        None | Some(0) => None,
        Some(count) if count > rail::BADGE_MAX => Some(format!("{}+", rail::BADGE_MAX).into()),
        Some(count) => Some(count.to_string().into()),
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A `rail::BUTTON` square button of the activity rail, cut `cut::CONTROL`, with an
/// `icon::RAIL` icon and an optional count badge in the accent at its top-right corner.
///
/// UI automation finds it by its element id and reads its label; the badge has element id
/// `<id>-badge` and the count as its label.
#[derive(IntoElement)]
pub struct RailButton {
    id: SharedString,
    icon: IconName,
    label: SharedString,
    active: bool,
    badge: Option<usize>,
    on_click: Option<ClickHandler>,
}

impl RailButton {
    /// An idle button with element id `id` showing `icon`, named `label`.
    pub fn new(
        id: impl Into<SharedString>,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            active: false,
            badge: None,
            on_click: None,
        }
    }

    /// Whether the view this button opens is on screen.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// The count on the badge; no count or zero shows no badge.
    pub fn badge(mut self, count: Option<usize>) -> Self {
        self.badge = count;
        self
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

impl std::fmt::Debug for RailButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RailButton")
            .field("id", &self.id)
            .field("icon", &self.icon)
            .field("label", &self.label)
            .field("active", &self.active)
            .field("badge", &self.badge)
            .finish_non_exhaustive()
    }
}

impl RenderOnce for RailButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = colors(cx);
        let brand = brand(cx);
        let colors = rail_button_colors(self.active, palette, brand);
        let badge = badge_text(self.badge).map(|text| {
            let badge_id: SharedString = format!("{}-badge", self.id).into();
            named(&badge_id, Role::Label, text.clone())
                .absolute()
                .top(rail::BADGE_INSET)
                .right(rail::BADGE_INSET)
                .pl(rail::BADGE_PADDING_X)
                .pr(rail::BADGE_PADDING_X)
                .font_family(family::INTERFACE)
                .text_size(rail::BADGE_FONT)
                .line_height(rail::BADGE_LINE_HEIGHT)
                .font_weight(FontWeight::BOLD)
                .text_color(brand.on_accent)
                .child(Chamfer::new(cut::KEYCAP).fill(brand.accent))
                .child(div().relative().child(text))
        });

        named(&self.id, Role::Button, self.label)
            .group(GROUP)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(rail::BUTTON)
            .cursor_pointer()
            .child(Chamfer::new(cut::CONTROL).fill(colors.rest))
            .child(hover_wash(cut::CONTROL, colors.hover))
            .child(Icon::new(self.icon).size(icon::RAIL).color(colors.icon))
            .children(badge)
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Brand;
    use crate::test_support::rgba_of;
    use crate::tokens::{Colors, hex};

    fn brand() -> Brand {
        Brand {
            name: "Product".into(),
            accent: hex(0x2B45B8),
            accent_hover: hex(0x3451C8),
            accent_pressed: hex(0x1F3389),
            accent_tint: hex(0x9DB1FF),
            on_accent: hex(0xFFFFFF),
        }
    }

    #[test]
    fn the_active_button_sits_on_the_tint_wash_with_a_tint_icon() {
        let palette = Colors::dark();
        let colors = rail_button_colors(true, &palette, &brand());
        assert_eq!(rgba_of(colors.rest), (0x9DB1FF, 0.16));
        assert_eq!(rgba_of(colors.icon), (0x9DB1FF, 1.0));
        assert_eq!(colors.hover, palette.hover);
    }

    #[test]
    fn an_idle_button_is_transparent_with_a_muted_icon() {
        let palette = Colors::dark();
        let colors = rail_button_colors(false, &palette, &brand());
        assert_eq!(colors.rest.a, 0.0);
        assert_eq!(colors.icon, palette.muted);
        assert_eq!(colors.hover, palette.hover);
    }

    #[test]
    fn the_badge_shows_only_a_positive_count_and_caps_it() {
        assert_eq!(badge_text(None), None);
        assert_eq!(badge_text(Some(0)), None);
        assert_eq!(badge_text(Some(1)).as_deref(), Some("1"));
        assert_eq!(badge_text(Some(99)).as_deref(), Some("99"));
        assert_eq!(badge_text(Some(100)).as_deref(), Some("99+"));
    }
}
