//! Color roles of the dark and light palettes and the wash strengths.

use gpui::Hsla;

/// The opaque color `0xRRGGBB`.
pub fn hex(rgb: u32) -> Hsla {
    Hsla::from(gpui::rgb(rgb))
}

/// The color `0xRRGGBB` at `alpha`, from 0 (transparent) to 1 (opaque).
pub fn hex_alpha(rgb: u32, alpha: f32) -> Hsla {
    Hsla {
        a: alpha,
        ..hex(rgb)
    }
}

/// Opacity of the accent-tint washes, applied by [`crate::Brand::washes`].
pub mod accent_wash {
    /// A selected table or list row.
    pub const SELECTED_ROW: f32 = 0.07;
    /// The selected tree item or menu entry.
    pub const SELECTED_ITEM: f32 = 0.12;
    /// Selected text in inputs and editors.
    pub const TEXT_SELECTION: f32 = 0.25;
    /// A drop target while dragging.
    pub const DROP_TARGET: f32 = 0.10;
    /// The active navigation entry: the activity rail button of the view on screen.
    pub const ACTIVE_NAV: f32 = 0.16;
}

/// The neutral, wash and semantic color roles of one palette.
///
/// [`Colors::dark`] is the design of record; [`Colors::light`] fills the same roles for a light
/// ground, keeping every text role and semantic color at least 4.5:1 on `panel`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    /// The window ground the islands sit on.
    pub desk: Hsla,
    /// Inset fills below the panel level.
    pub background: Hsla,
    /// Island and panel fill.
    pub panel: Hsla,
    /// Raised controls and secondary fills.
    pub raised: Hsla,
    /// Default border.
    pub line: Hsla,
    /// Stronger border, input edges.
    pub line_2: Hsla,
    /// Divider between data rows.
    pub row_divider: Hsla,
    /// Titles, selected items and data values.
    pub strong: Hsla,
    /// Body text.
    pub foreground: Hsla,
    /// Metadata, labels, counts and descriptions.
    pub muted: Hsla,
    /// Hover wash over any surface.
    pub hover: Hsla,
    /// Wash of every other data row.
    pub alternating_row: Hsla,
    /// The 1 px edge along an island outline.
    pub island_edge: Hsla,
    /// The dimmed desk behind a modal.
    pub scrim: Hsla,
    /// Success state.
    pub success: Hsla,
    /// Informational state and links.
    pub info: Hsla,
    /// Warning state.
    pub warning: Hsla,
    /// Errors and destructive actions.
    pub danger: Hsla,
    /// Text drawn on a solid success, info, warning or danger fill.
    pub on_semantic: Hsla,
}

impl Colors {
    /// The dark palette.
    pub fn dark() -> Self {
        Self {
            desk: hex(0x050507),
            background: hex(0x09090B),
            panel: hex(0x100F13),
            raised: hex(0x1A181E),
            line: hex(0x232128),
            line_2: hex(0x37333D),
            row_divider: hex(0x18161B),
            strong: hex(0xF7F4F7),
            foreground: hex(0xC6C3CC),
            muted: hex(0x8E8996),
            hover: hex_alpha(0xFFFFFF, 0.04),
            alternating_row: hex_alpha(0xFFFFFF, 0.012),
            island_edge: hex_alpha(0xFFFFFF, 0.05),
            scrim: hex_alpha(0x050507, 0.62),
            success: hex(0x7BE0A0),
            info: hex(0x6EA8FF),
            warning: hex(0xFFC23D),
            danger: hex(0xFF6B5E),
            on_semantic: hex(0x09090B),
        }
    }

    /// The light palette: near-white grounds with the desk a step below the panels, lines
    /// darker than the panels, dark text, black washes and deeper semantic colors.
    pub fn light() -> Self {
        Self {
            desk: hex(0xEDECF0),
            background: hex(0xF4F3F6),
            panel: hex(0xFCFBFD),
            raised: hex(0xEFEDF2),
            line: hex(0xE0DEE5),
            line_2: hex(0xC9C5D0),
            row_divider: hex(0xEEECF1),
            strong: hex(0x17151B),
            foreground: hex(0x3A3740),
            muted: hex(0x6B6673),
            hover: hex_alpha(0x000000, 0.04),
            alternating_row: hex_alpha(0x000000, 0.02),
            island_edge: hex_alpha(0x000000, 0.06),
            scrim: hex_alpha(0x17151B, 0.32),
            success: hex(0x1C7A45),
            info: hex(0x1F5FCC),
            warning: hex(0x8A5A00),
            danger: hex(0xC4352B),
            on_semantic: hex(0xFFFFFF),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::rgba_of;

    #[test]
    fn neutrals_match_the_design_system() {
        let colors = Colors::dark();
        assert_eq!(rgba_of(colors.desk), (0x050507, 1.0));
        assert_eq!(rgba_of(colors.background), (0x09090B, 1.0));
        assert_eq!(rgba_of(colors.panel), (0x100F13, 1.0));
        assert_eq!(rgba_of(colors.raised), (0x1A181E, 1.0));
        assert_eq!(rgba_of(colors.line), (0x232128, 1.0));
        assert_eq!(rgba_of(colors.line_2), (0x37333D, 1.0));
        assert_eq!(rgba_of(colors.row_divider), (0x18161B, 1.0));
        assert_eq!(rgba_of(colors.strong), (0xF7F4F7, 1.0));
        assert_eq!(rgba_of(colors.foreground), (0xC6C3CC, 1.0));
        assert_eq!(rgba_of(colors.muted), (0x8E8996, 1.0));
    }

    #[test]
    fn washes_match_the_design_system() {
        let colors = Colors::dark();
        assert_eq!(rgba_of(colors.hover), (0xFFFFFF, 0.04));
        assert_eq!(rgba_of(colors.alternating_row), (0xFFFFFF, 0.012));
        assert_eq!(rgba_of(colors.island_edge), (0xFFFFFF, 0.05));
        assert_eq!(rgba_of(colors.scrim), (0x050507, 0.62));
        assert_eq!(accent_wash::SELECTED_ROW, 0.07);
        assert_eq!(accent_wash::SELECTED_ITEM, 0.12);
        assert_eq!(accent_wash::TEXT_SELECTION, 0.25);
        assert_eq!(accent_wash::DROP_TARGET, 0.10);
        assert_eq!(accent_wash::ACTIVE_NAV, 0.16);
    }

    #[test]
    fn semantic_colors_match_the_design_system() {
        let colors = Colors::dark();
        assert_eq!(rgba_of(colors.success), (0x7BE0A0, 1.0));
        assert_eq!(rgba_of(colors.info), (0x6EA8FF, 1.0));
        assert_eq!(rgba_of(colors.warning), (0xFFC23D, 1.0));
        assert_eq!(rgba_of(colors.danger), (0xFF6B5E, 1.0));
    }

    /// The WCAG 2 contrast ratio between two opaque colors.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let luminance = |color: Hsla| {
            let rgba = gpui::Rgba::from(color);
            let linear = |c: f32| {
                if c <= 0.039_28 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b)
        };
        let (high, low) = {
            let (x, y) = (luminance(a), luminance(b));
            if x > y { (x, y) } else { (y, x) }
        };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn light_neutrals_are_near_white_grounds_with_darker_lines_and_dark_text() {
        let colors = Colors::light();
        assert_eq!(rgba_of(colors.desk), (0xEDECF0, 1.0));
        assert_eq!(rgba_of(colors.background), (0xF4F3F6, 1.0));
        assert_eq!(rgba_of(colors.panel), (0xFCFBFD, 1.0));
        assert_eq!(rgba_of(colors.raised), (0xEFEDF2, 1.0));
        assert_eq!(rgba_of(colors.line), (0xE0DEE5, 1.0));
        assert_eq!(rgba_of(colors.line_2), (0xC9C5D0, 1.0));
        assert_eq!(rgba_of(colors.row_divider), (0xEEECF1, 1.0));
        assert_eq!(rgba_of(colors.strong), (0x17151B, 1.0));
        assert_eq!(rgba_of(colors.foreground), (0x3A3740, 1.0));
        assert_eq!(rgba_of(colors.muted), (0x6B6673, 1.0));
        assert!(colors.line.l < colors.panel.l && colors.line_2.l < colors.line.l);
        assert!(colors.strong.l < colors.foreground.l && colors.foreground.l < colors.muted.l);
    }

    #[test]
    fn light_washes_darken_and_semantics_are_deeper() {
        let colors = Colors::light();
        assert_eq!(rgba_of(colors.hover), (0x000000, 0.04));
        assert_eq!(rgba_of(colors.alternating_row), (0x000000, 0.02));
        assert_eq!(rgba_of(colors.island_edge), (0x000000, 0.06));
        assert_eq!(rgba_of(colors.scrim), (0x17151B, 0.32));
        assert_eq!(rgba_of(colors.success), (0x1C7A45, 1.0));
        assert_eq!(rgba_of(colors.info), (0x1F5FCC, 1.0));
        assert_eq!(rgba_of(colors.warning), (0x8A5A00, 1.0));
        assert_eq!(rgba_of(colors.danger), (0xC4352B, 1.0));
        assert_eq!(rgba_of(colors.on_semantic), (0xFFFFFF, 1.0));
    }

    #[test]
    fn text_on_the_panel_meets_wcag_aa_in_both_palettes() {
        for (name, colors) in [("dark", Colors::dark()), ("light", Colors::light())] {
            for (role, color) in [
                ("strong", colors.strong),
                ("foreground", colors.foreground),
                ("muted", colors.muted),
                ("success", colors.success),
                ("info", colors.info),
                ("warning", colors.warning),
                ("danger", colors.danger),
            ] {
                let ratio = contrast(color, colors.panel);
                assert!(ratio >= 4.5, "{name} {role} on panel: {ratio:.2}");
            }
        }
    }

    #[test]
    fn on_semantic_text_reads_on_every_light_semantic_fill() {
        let colors = Colors::light();
        for fill in [colors.success, colors.info, colors.warning, colors.danger] {
            assert!(contrast(colors.on_semantic, fill) >= 4.5);
        }
    }
}
