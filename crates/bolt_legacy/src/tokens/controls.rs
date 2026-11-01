//! Metrics of the shell controls: icon button, keycap, field, rail button, status chip,
//! document tab and action strip.
//!
//! Heights shared with other controls live in [`super::chrome`]; cuts in [`super::cut`].

/// The icon-only button.
pub mod icon_button {
    use gpui::{Pixels, px};

    /// Width of an icon button on a [`crate::tokens::chrome::CONTROL`] height.
    pub const WIDTH: Pixels = px(32.0);
}

/// The keycap that prints a shortcut.
pub mod keycap {
    use gpui::{Pixels, px};

    /// Text size, in the mono family.
    pub const FONT: Pixels = px(10.5);
    /// Line height.
    pub const LINE_HEIGHT: Pixels = px(14.0);
    /// Horizontal padding.
    pub const PADDING_X: Pixels = px(6.0);
    /// Vertical padding.
    pub const PADDING_Y: Pixels = px(2.0);
}

/// The single-line field.
pub mod field {
    use gpui::{Pixels, px};

    /// Horizontal padding.
    pub const PADDING_X: Pixels = px(10.0);
    /// Gap between the leading icon, the text and the trailing keycap.
    pub const GAP: Pixels = px(8.0);
    /// Text size.
    pub const FONT: Pixels = px(12.5);
}

/// The activity rail button.
pub mod rail {
    use gpui::{Pixels, px};

    /// Side of the square button.
    pub const BUTTON: Pixels = px(38.0);
    /// Distance of the count badge from the button's top and right edges.
    pub const BADGE_INSET: Pixels = px(3.0);
    /// Horizontal padding of the count badge.
    pub const BADGE_PADDING_X: Pixels = px(3.0);
    /// Text size of the count badge.
    pub const BADGE_FONT: Pixels = px(9.0);
    /// Line height, and so height, of the count badge.
    pub const BADGE_LINE_HEIGHT: Pixels = px(13.0);
    /// The largest count a badge prints; larger counts read as this number and a plus.
    pub const BADGE_MAX: usize = 99;
}

/// The status bar chip.
pub mod status_chip {
    use gpui::{Pixels, px};

    /// Horizontal padding.
    pub const PADDING_X: Pixels = px(12.0);
    /// Gap between the icon and the text.
    pub const GAP: Pixels = px(8.0);
    /// Text size.
    pub const FONT: Pixels = px(12.0);
}

/// The document tab and the row that holds the tabs.
pub mod doc_tab {
    use gpui::{Pixels, px};

    /// Horizontal padding of the tab row.
    pub const ROW_PADDING_X: Pixels = px(10.0);
    /// Gap between tabs.
    pub const ROW_GAP: Pixels = px(4.0);
    /// Horizontal padding of a tab.
    pub const PADDING_X: Pixels = px(12.0);
    /// Gap between the method label, the name and the close button.
    pub const GAP: Pixels = px(8.0);
    /// Side of the close icon.
    pub const CLOSE_ICON: Pixels = px(13.0);
}

/// The vertical strip of document actions at the left of an editor.
pub mod action_strip {
    use gpui::{Pixels, px};

    /// Width of the strip.
    pub const WIDTH: Pixels = px(36.0);
    /// Padding above the first and below the last action.
    pub const PADDING_Y: Pixels = px(8.0);
    /// Gap between actions.
    pub const GAP: Pixels = px(4.0);
    /// Length of the separator line between action groups.
    pub const SEPARATOR: Pixels = px(16.0);
    /// Space above and below the separator, on top of [`GAP`].
    pub const SEPARATOR_MARGIN: Pixels = px(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn control_metrics_match_the_design_system() {
        assert_eq!(icon_button::WIDTH, px(32.0));
        assert_eq!(
            [
                keycap::FONT,
                keycap::LINE_HEIGHT,
                keycap::PADDING_X,
                keycap::PADDING_Y
            ],
            [px(10.5), px(14.0), px(6.0), px(2.0)]
        );
        assert_eq!(
            [field::PADDING_X, field::GAP, field::FONT],
            [px(10.0), px(8.0), px(12.5)]
        );
        assert_eq!(
            [
                rail::BUTTON,
                rail::BADGE_INSET,
                rail::BADGE_PADDING_X,
                rail::BADGE_FONT,
                rail::BADGE_LINE_HEIGHT
            ],
            [px(38.0), px(3.0), px(3.0), px(9.0), px(13.0)]
        );
        assert_eq!(rail::BADGE_MAX, 99);
        assert_eq!(
            [status_chip::PADDING_X, status_chip::GAP, status_chip::FONT],
            [px(12.0), px(8.0), px(12.0)]
        );
        assert_eq!(
            [
                doc_tab::ROW_PADDING_X,
                doc_tab::ROW_GAP,
                doc_tab::PADDING_X,
                doc_tab::GAP,
                doc_tab::CLOSE_ICON
            ],
            [px(10.0), px(4.0), px(12.0), px(8.0), px(13.0)]
        );
        assert_eq!(
            [
                action_strip::WIDTH,
                action_strip::PADDING_Y,
                action_strip::GAP,
                action_strip::SEPARATOR,
                action_strip::SEPARATOR_MARGIN
            ],
            [px(36.0), px(8.0), px(4.0), px(16.0), px(2.0)]
        );
    }
}
