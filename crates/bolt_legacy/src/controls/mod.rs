//! The shell controls: icon button, keycap, field, rail button and status chip.
//!
//! Each control is cut at 45 degrees with [`crate::chamfer::Chamfer`], takes its sizes from
//! [`crate::tokens`] and its colors from the installed roles, and carries an element id and
//! an accessible label, so UI automation finds it by id and reads it by label.

mod field;
mod hover;
mod icon_button;
mod keycap;
mod rail_button;
mod status_chip;

#[cfg(test)]
mod render_tests;

pub use field::Field;
pub use icon_button::{IconButton, IconButtonColors, IconButtonVariant, icon_button_colors};
pub use keycap::Keycap;
pub use rail_button::{RailButton, RailButtonColors, badge_text, rail_button_colors};
pub use status_chip::StatusChip;
