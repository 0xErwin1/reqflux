//! Spacing, borders, cut corners, chrome sizes and motion.
//!
//! Sizes are logical pixels at the base density.

/// The spacing scale.
pub mod space {
    use gpui::{Pixels, px};

    /// 4 px.
    pub const XS: Pixels = px(4.0);
    /// 6 px, the half step between [`XS`] and [`SM`].
    pub const HALF: Pixels = px(6.0);
    /// 8 px.
    pub const SM: Pixels = px(8.0);
    /// 12 px.
    pub const MD: Pixels = px(12.0);
    /// 16 px.
    pub const LG: Pixels = px(16.0);
    /// 24 px.
    pub const XL: Pixels = px(24.0);
}

/// Border widths.
pub mod border {
    use gpui::{Pixels, px};

    /// Default border and island edge.
    pub const THIN: Pixels = px(1.0);
    /// Emphasis edges: the active tab and the selected tree row bar.
    pub const THICK: Pixels = px(2.0);
    /// The focus ring.
    pub const FOCUS_RING: Pixels = px(1.5);
}

/// The 45 degree corner cut of each surface kind. Data (rows, cells, code) stays square.
pub mod cut {
    use gpui::{Pixels, px};

    /// Keycaps.
    pub const KEYCAP: Pixels = px(4.0);
    /// Buttons and small controls.
    pub const CONTROL: Pixels = px(6.0);
    /// Text inputs.
    pub const INPUT: Pixels = px(8.0);
    /// Large controls.
    pub const LARGE_CONTROL: Pixels = px(10.0);
    /// Menus, popovers and tooltips.
    pub const OVERLAY: Pixels = px(12.0);
    /// Cards and islands.
    pub const CARD: Pixels = px(14.0);
    /// Modals.
    pub const MODAL: Pixels = px(18.0);
    /// Corner radius everywhere: Bolt shapes are cut, never rounded.
    pub const RADIUS: Pixels = px(0.0);
}

/// Sizes of the window chrome and controls.
pub mod chrome {
    use gpui::{Pixels, px};

    /// Title bar height, on the desk.
    pub const TITLE_BAR: Pixels = px(44.0);
    /// Document tab row height.
    pub const DOC_TAB_ROW: Pixels = px(46.0);
    /// Document tab height inside its row.
    pub const DOC_TAB: Pixels = px(30.0);
    /// Panel header height.
    pub const PANEL_HEADER: Pixels = px(40.0);
    /// Status bar height, on the desk.
    pub const STATUS_BAR: Pixels = px(38.0);
    /// Status bar chip height.
    pub const STATUS_CHIP: Pixels = px(26.0);
    /// Tree row height.
    pub const TREE_ROW: Pixels = px(26.0);
    /// Indent per tree level.
    pub const TREE_INDENT: Pixels = px(14.0);
    /// Data grid row height.
    pub const GRID_ROW: Pixels = px(31.0);
    /// Default control height.
    pub const CONTROL: Pixels = px(30.0);
    /// Inline control height.
    pub const CONTROL_INLINE: Pixels = px(24.0);
    /// Large control height.
    pub const CONTROL_LARGE: Pixels = px(44.0);
    /// Desk between islands and at the side edges of the window.
    pub const ISLAND_GAP: Pixels = px(8.0);
    /// Sidebar island width.
    pub const SIDEBAR_WIDTH: Pixels = px(290.0);
    /// Activity rail width, on the desk at the left of the sidebar island.
    pub const RAIL_WIDTH: Pixels = px(46.0);
}

/// Icon metrics.
pub mod icon {
    use gpui::{Pixels, px};

    /// Small icons: tree disclosure chevrons, inline indicators.
    pub const SM: Pixels = px(12.0);
    /// Status chip icons.
    pub const CHIP: Pixels = px(13.0);
    /// Default icons: icon buttons, field icons.
    pub const MD: Pixels = px(14.0);
    /// Tree item icons.
    pub const ITEM: Pixels = px(15.0);
    /// Emphasized title bar icons.
    pub const LG: Pixels = px(16.0);
    /// Activity rail icons.
    pub const RAIL: Pixels = px(19.0);
    /// Stroke width of line icons.
    pub const STROKE: Pixels = px(1.5);
}

/// Motion timing.
pub mod motion {
    use std::time::Duration;

    /// Duration of every transition, on the `cubic-bezier(.2, .8, .2, 1)` curve.
    pub const FAST: Duration = Duration::from_millis(150);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn spacing_and_borders_match_the_design_system() {
        let scale = [
            space::XS,
            space::HALF,
            space::SM,
            space::MD,
            space::LG,
            space::XL,
        ];
        assert_eq!(
            scale,
            [px(4.0), px(6.0), px(8.0), px(12.0), px(16.0), px(24.0)]
        );
        assert_eq!(border::THIN, px(1.0));
        assert_eq!(border::THICK, px(2.0));
        assert_eq!(border::FOCUS_RING, px(1.5));
    }

    #[test]
    fn cuts_match_the_design_system() {
        let cuts = [
            cut::KEYCAP,
            cut::CONTROL,
            cut::INPUT,
            cut::LARGE_CONTROL,
            cut::OVERLAY,
            cut::CARD,
            cut::MODAL,
        ];
        assert_eq!(
            cuts,
            [
                px(4.0),
                px(6.0),
                px(8.0),
                px(10.0),
                px(12.0),
                px(14.0),
                px(18.0)
            ]
        );
        assert_eq!(cut::RADIUS, px(0.0));
    }

    #[test]
    fn chrome_matches_the_design_system() {
        assert_eq!(chrome::TITLE_BAR, px(44.0));
        assert_eq!(chrome::DOC_TAB_ROW, px(46.0));
        assert_eq!(chrome::DOC_TAB, px(30.0));
        assert_eq!(chrome::PANEL_HEADER, px(40.0));
        assert_eq!(chrome::STATUS_BAR, px(38.0));
        assert_eq!(chrome::STATUS_CHIP, px(26.0));
        assert_eq!(chrome::TREE_ROW, px(26.0));
        assert_eq!(chrome::TREE_INDENT, px(14.0));
        assert_eq!(chrome::CONTROL, px(30.0));
        assert_eq!(chrome::CONTROL_INLINE, px(24.0));
        assert_eq!(chrome::CONTROL_LARGE, px(44.0));
        assert_eq!(chrome::ISLAND_GAP, px(8.0));
        assert_eq!(chrome::SIDEBAR_WIDTH, px(290.0));
        assert_eq!(chrome::RAIL_WIDTH, px(46.0));
        assert_eq!(motion::FAST.as_millis(), 150);
    }

    #[test]
    fn icon_sizes_match_the_design_system() {
        let sizes = [
            icon::SM,
            icon::CHIP,
            icon::MD,
            icon::ITEM,
            icon::LG,
            icon::RAIL,
        ];
        assert_eq!(
            sizes,
            [px(12.0), px(13.0), px(14.0), px(15.0), px(16.0), px(19.0)]
        );
    }
}
