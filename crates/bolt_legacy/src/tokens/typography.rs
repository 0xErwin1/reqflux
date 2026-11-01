//! Font families and the eight text roles.

use gpui::{FontWeight, Pixels, px};

/// Font family names, as registered by the bundled fonts.
pub mod family {
    /// Interface face: labels, buttons, menus, body copy, tabs and tree rows.
    pub const INTERFACE: &str = "Archivo";
    /// Display face: uppercase section labels. Never body text.
    pub const DISPLAY: &str = "Archivo Expanded";
    /// Data face: data, URLs, code, keys and key hints.
    pub const MONO: &str = "JetBrains Mono";
}

/// Base interface text size.
pub const BASE_SIZE: Pixels = px(13.0);

/// The text level a role uses unless a screen overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTone {
    /// [`crate::tokens::Colors::strong`].
    Strong,
    /// [`crate::tokens::Colors::foreground`].
    Body,
    /// [`crate::tokens::Colors::muted`].
    Muted,
}

/// One typography role.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRole {
    /// Font family, one of [`family`].
    pub family: &'static str,
    /// Font size.
    pub size: Pixels,
    /// Font weight.
    pub weight: FontWeight,
    /// Letter spacing in em.
    pub tracking_em: f32,
    /// Whether the text is set in capitals.
    pub uppercase: bool,
    /// Default text level.
    pub tone: TextTone,
}

impl TextRole {
    const fn plain(family: &'static str, size: f32, weight: FontWeight, tone: TextTone) -> Self {
        Self {
            family,
            size: px(size),
            weight,
            tracking_em: 0.0,
            uppercase: false,
            tone,
        }
    }
}

/// The eight text roles of Bolt.
pub mod text {
    use super::{TextRole, TextTone, family};
    use gpui::{FontWeight, px};

    /// Screen and document titles.
    pub const TITLE: TextRole =
        TextRole::plain(family::INTERFACE, 20.0, FontWeight::BOLD, TextTone::Strong);
    /// Section headings.
    pub const HEADING: TextRole =
        TextRole::plain(family::INTERFACE, 18.0, FontWeight::BOLD, TextTone::Strong);
    /// Body copy.
    pub const BODY: TextRole =
        TextRole::plain(family::INTERFACE, 13.0, FontWeight::MEDIUM, TextTone::Body);
    /// Secondary body copy.
    pub const BODY_SMALL: TextRole =
        TextRole::plain(family::INTERFACE, 12.0, FontWeight::MEDIUM, TextTone::Body);
    /// Uppercase section labels.
    pub const LABEL: TextRole = TextRole {
        family: family::DISPLAY,
        size: px(11.0),
        weight: FontWeight::EXTRA_BOLD,
        tracking_em: 0.14,
        uppercase: true,
        tone: TextTone::Muted,
    };
    /// Captions and metadata.
    pub const CAPTION: TextRole =
        TextRole::plain(family::INTERFACE, 12.0, FontWeight::MEDIUM, TextTone::Muted);
    /// Data, URLs and code.
    pub const CODE: TextRole =
        TextRole::plain(family::MONO, 13.0, FontWeight::MEDIUM, TextTone::Body);
    /// Keyboard hints.
    pub const KEY_HINT: TextRole =
        TextRole::plain(family::MONO, 12.0, FontWeight::MEDIUM, TextTone::Muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_match_the_design_system() {
        assert_eq!(family::INTERFACE, "Archivo");
        assert_eq!(family::DISPLAY, "Archivo Expanded");
        assert_eq!(family::MONO, "JetBrains Mono");
        assert_eq!(BASE_SIZE, px(13.0));
    }

    #[test]
    fn roles_match_the_design_system() {
        let roles = [
            (text::TITLE, family::INTERFACE, 20.0, 700.0),
            (text::HEADING, family::INTERFACE, 18.0, 700.0),
            (text::BODY, family::INTERFACE, 13.0, 500.0),
            (text::BODY_SMALL, family::INTERFACE, 12.0, 500.0),
            (text::LABEL, family::DISPLAY, 11.0, 800.0),
            (text::CAPTION, family::INTERFACE, 12.0, 500.0),
            (text::CODE, family::MONO, 13.0, 500.0),
            (text::KEY_HINT, family::MONO, 12.0, 500.0),
        ];
        for (role, family, size, weight) in roles {
            assert_eq!(role.family, family, "{role:?}");
            assert_eq!(role.size, px(size), "{role:?}");
            assert_eq!(role.weight, FontWeight(weight), "{role:?}");
            assert_eq!(role.uppercase, role == text::LABEL, "{role:?}");
        }
        assert_eq!(text::LABEL.tracking_em, 0.14);
        assert_eq!(text::TITLE.tone, TextTone::Strong);
        assert_eq!(text::LABEL.tone, TextTone::Muted);
        assert_eq!(text::CODE.tone, TextTone::Body);
    }
}
