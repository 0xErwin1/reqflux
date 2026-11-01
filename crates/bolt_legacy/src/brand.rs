//! What a product passes to Bolt: its name and accent.

use gpui::{Hsla, SharedString};

use crate::palette::Palette;
use crate::tokens::accent_wash;

/// A product's identity: its name and its accent colors.
///
/// The accent fill is never text on the dark ground; accent text, icons, the caret and the
/// focus ring use [`Brand::accent_tint`]. On the light ground the accent itself reads as text,
/// so [`Brand::for_palette`] uses it as the tint there.
#[derive(Debug, Clone, PartialEq)]
pub struct Brand {
    /// The product name.
    pub name: SharedString,
    /// The accent fill: the primary action, the active tab edge, the logo.
    pub accent: Hsla,
    /// The accent fill under the pointer.
    pub accent_hover: Hsla,
    /// The accent fill while pressed.
    pub accent_pressed: Hsla,
    /// Accent text, the selection bar, the caret and the focus ring.
    pub accent_tint: Hsla,
    /// Text on an accent fill.
    pub on_accent: Hsla,
}

/// The washes derived from the accent tint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccentWashes {
    /// A selected table or list row.
    pub selected_row: Hsla,
    /// The selected tree item or menu entry.
    pub selected_item: Hsla,
    /// Selected text.
    pub text_selection: Hsla,
    /// A drop target while dragging.
    pub drop_target: Hsla,
}

impl Brand {
    /// This brand as it draws on `palette`: unchanged on dark; on light the tint is the accent,
    /// so accent text and the washes derived from it stay readable on the light ground.
    pub fn for_palette(&self, palette: Palette) -> Self {
        match palette {
            Palette::Dark => self.clone(),
            Palette::Light => Self {
                accent_tint: self.accent,
                ..self.clone()
            },
        }
    }

    /// The washes of this brand: the accent tint at each [`accent_wash`] opacity.
    pub fn washes(&self) -> AccentWashes {
        let tint = |alpha| Hsla {
            a: alpha,
            ..self.accent_tint
        };
        AccentWashes {
            selected_row: tint(accent_wash::SELECTED_ROW),
            selected_item: tint(accent_wash::SELECTED_ITEM),
            text_selection: tint(accent_wash::TEXT_SELECTION),
            drop_target: tint(accent_wash::DROP_TARGET),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Palette;
    use crate::test_support::rgba_of;
    use crate::tokens::hex;

    #[test]
    fn washes_are_the_tint_at_each_opacity() {
        let brand = Brand {
            name: "Product".into(),
            accent: hex(0x2B45B8),
            accent_hover: hex(0x3451C8),
            accent_pressed: hex(0x1F3389),
            accent_tint: hex(0x9DB1FF),
            on_accent: hex(0xFFFFFF),
        };
        let washes = brand.washes();
        assert_eq!(rgba_of(washes.selected_row), (0x9DB1FF, 0.07));
        assert_eq!(rgba_of(washes.selected_item), (0x9DB1FF, 0.12));
        assert_eq!(rgba_of(washes.text_selection), (0x9DB1FF, 0.25));
        assert_eq!(rgba_of(washes.drop_target), (0x9DB1FF, 0.10));
    }

    #[test]
    fn light_washes_use_the_accent_and_dark_washes_the_tint() {
        let brand = Brand {
            name: "Product".into(),
            accent: hex(0x2B45B8),
            accent_hover: hex(0x3451C8),
            accent_pressed: hex(0x1F3389),
            accent_tint: hex(0x9DB1FF),
            on_accent: hex(0xFFFFFF),
        };
        let light = brand.for_palette(Palette::Light);
        assert_eq!(rgba_of(light.accent_tint), (0x2B45B8, 1.0));
        assert_eq!(light.accent, brand.accent);
        let washes = light.washes();
        assert_eq!(rgba_of(washes.selected_row), (0x2B45B8, 0.07));
        assert_eq!(rgba_of(washes.selected_item), (0x2B45B8, 0.12));
        assert_eq!(rgba_of(washes.text_selection), (0x2B45B8, 0.25));
        assert_eq!(rgba_of(washes.drop_target), (0x2B45B8, 0.10));
        assert_eq!(brand.for_palette(Palette::Dark), brand);
    }
}
