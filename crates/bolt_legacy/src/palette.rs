//! Which palette draws: the user's [`PaletteMode`] and the [`Palette`] it resolves to.

use gpui::WindowAppearance;

use crate::tokens::Colors;

/// One of the two palettes, with the same color roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Palette {
    /// The dark palette, the design of record.
    Dark,
    /// The light palette.
    Light,
}

impl Palette {
    /// The color roles of this palette.
    pub fn colors(self) -> Colors {
        match self {
            Self::Dark => Colors::dark(),
            Self::Light => Colors::light(),
        }
    }

    /// Whether this is the dark palette.
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

/// The palette the user chose: one of the two, or whatever the window appearance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PaletteMode {
    /// Always the dark palette.
    #[default]
    Dark,
    /// Always the light palette.
    Light,
    /// The palette of the window appearance, following it when the OS changes it.
    System,
}

impl PaletteMode {
    /// The palette this mode draws in a window of `appearance`.
    pub fn resolve(self, appearance: WindowAppearance) -> Palette {
        match self {
            Self::Dark => Palette::Dark,
            Self::Light => Palette::Light,
            Self::System => match appearance {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette::Dark,
                WindowAppearance::Light | WindowAppearance::VibrantLight => Palette::Light,
            },
        }
    }

    /// The mode after this one in the cycle Dark, Light, System.
    pub fn next(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::System,
            Self::System => Self::Dark,
        }
    }
}
