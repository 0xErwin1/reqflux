//! The palette's sizes, as drawn in design/command-palette/02-scope-tabs.html; the values live
//! in `tokens.rs` with the other ReqFlux sizes.

use gpui::Pixels;

use crate::tokens;

/// The height of the query row.
pub(crate) const QUERY_ROW: Pixels = tokens::PALETTE_QUERY_ROW;

/// The height of the scope tab row.
pub(crate) const TAB_ROW: Pixels = tokens::PALETTE_TAB_ROW;

/// The height of a scope tab.
pub(crate) const TAB: Pixels = tokens::PALETTE_TAB;

/// The height of a row.
pub(crate) const ROW: Pixels = tokens::PALETTE_ROW;

/// The height of the key hint footer.
pub(crate) const FOOTER: Pixels = tokens::PALETTE_ROW;

/// The horizontal padding of the query row, the rows, the headers and the footer.
pub(crate) const INSET: Pixels = tokens::PALETTE_INSET;

/// The width of the method column of a request row.
pub(crate) const METHOD_COLUMN: Pixels = tokens::PALETTE_METHOD_COLUMN;

/// The size of the square that holds the icon of the empty and no-match states.
pub(crate) const EMPTY_ICON_BOX: Pixels = tokens::PALETTE_EMPTY_ICON_BOX;
