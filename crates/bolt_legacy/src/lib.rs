//! ReqFlux's copy of Bolt, the design system it shares with DBFlux. Today's screens still draw
//! with it; they move to DBFlux's `bolt` crate one by one, and this crate is deleted after the
//! last one. Add nothing here.
//!
//! Bolt owns everything that does not depend on the product: color roles and washes,
//! typography roles, spacing, cut corners, chrome metrics, the theme mapping onto
//! gpui-component, and generic components. A product supplies only its brand (accent,
//! name, logo) and any extra tokens of its own.
//!
//! This crate must never name a product, a protocol or a product type, so it can move to
//! its own repository without changes. See "Design system" in ARCHITECTURE.md.
//!
//! A product calls [`install`] with its [`Brand`] and a [`PaletteMode`] before opening a
//! window, switches with [`set_mode`], and reads roles through [`colors`], [`brand`],
//! [`accent_washes`], [`palette`] and the constants in [`tokens`].

mod brand;
pub mod chamfer;
mod chevron;
pub mod controls;
pub mod fonts;
mod icon;
mod island;
mod palette;
mod theme;
pub mod tokens;

#[cfg(test)]
mod test_support;

pub use brand::{AccentWashes, Brand};
pub use chevron::{Chevron, ChevronDirection, chevron_points};
pub use icon::{Icon, IconName, assets};
pub use island::Island;
pub use palette::{Palette, PaletteMode};
pub use theme::{accent_washes, brand, colors, install, mode, palette, set_mode};
