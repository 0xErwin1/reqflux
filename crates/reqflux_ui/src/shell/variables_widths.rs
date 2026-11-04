//! The resizable columns of the variables grid: key, initial value, current value and
//! where the value is stored. The number, on and delete columns are fixed and the
//! description fills the rest. The widths are the table store's (`table_widths`), shared by
//! the declared grid and the undeclared rows under it, so their columns stay aligned.

use gpui::{App, Div, Pixels, Stateful, Styled};

use super::rows::part;
use super::table_widths;
use crate::tokens;
use bolt::tokens::GridMetrics;

/// The id of the variables table in the table store.
const TABLE: &str = "variables";

/// A column of the variables grid that takes a drag.
#[derive(Clone, Copy)]
pub(super) enum Resizable {
    Key,
    Value,
    Current,
    Stored,
}

impl Resizable {
    /// The column's index in the table store.
    fn index(self) -> usize {
        match self {
            Self::Key => 0,
            Self::Value => 1,
            Self::Current => 2,
            Self::Stored => 3,
        }
    }

    /// The width the column has until it is dragged.
    fn default_width(self) -> Pixels {
        match self {
            Self::Key => tokens::VARIABLES_KEY_COLUMN,
            Self::Value => tokens::VARIABLES_VALUE_COLUMN,
            Self::Current => tokens::VARIABLES_CURRENT_COLUMN,
            Self::Stored => tokens::VARIABLES_STORED_COLUMN,
        }
    }

    /// The column's width now.
    pub(super) fn width(self, cx: &App) -> Pixels {
        table_widths::width(TABLE, self.index(), self.default_width(), cx)
    }

    /// The header cell `variables-header-<name>` of the column, `name` wide as it is now,
    /// with the resize handle on its right edge.
    pub(super) fn header_cell(self, name: &str, cx: &App) -> Stateful<Div> {
        let cell = part(format!("variables-header-{name}"))
            .w(self.width(cx))
            .flex_none()
            .pl(GridMetrics::CELL_PADDING_X)
            .pr(GridMetrics::CELL_PADDING_X)
            .overflow_hidden()
            .whitespace_nowrap();
        table_widths::with_handle(cell, TABLE, self.index(), self.default_width(), cx)
    }
}

/// `table`, the element that holds the whole variables grid, following a column drag.
pub(super) fn resizable<E: gpui::InteractiveElement>(table: E, cx: &App) -> E {
    table_widths::resizable(table, TABLE, cx)
}
