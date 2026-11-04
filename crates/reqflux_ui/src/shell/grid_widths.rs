//! The resizable column widths of the request grids. A width belongs to the kind of grid, not
//! to a request, so every request's Params grid shares one key width, every Headers grid
//! another, and so on. They are kept by `table_widths`, the store every table uses.

use gpui::{App, Pixels};

use super::table_widths;
use crate::tokens;

/// The kind of request grid whose widths are shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GridKind {
    Params,
    Headers,
    Urlencoded,
    FormData,
}

impl GridKind {
    /// The kind of the grid of `table` whose entries are `entry`s: `params`, `headers`, or
    /// the `body` of `field`s (urlencoded) or `part`s (form data).
    pub(crate) fn of(table: &str, entry: &str) -> Self {
        match (table, entry) {
            ("headers", _) => Self::Headers,
            ("body", "part") => Self::FormData,
            ("body", _) => Self::Urlencoded,
            _ => Self::Params,
        }
    }

    /// The id the grid's widths go by in the shared table store.
    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Params => "request-params",
            Self::Headers => "request-headers",
            Self::Urlencoded => "request-urlencoded",
            Self::FormData => "request-form-data",
        }
    }
}

/// The index of the key column of a request grid in the table store.
pub(crate) const KEY: usize = 0;
/// The index of the description column of a request grid in the table store.
pub(crate) const DESCRIPTION: usize = 1;

/// The widths of the resizable columns of a request grid, key then description, as the table
/// store holds them; the value column fills the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GridWidths([f32; 2]);

impl GridWidths {
    /// The widths of the grids of `kind` as they are now.
    pub(crate) fn of(kind: GridKind, cx: &App) -> Self {
        let column = |column, default: Pixels| {
            f32::from(table_widths::width(kind.table(), column, default, cx))
        };
        Self([
            column(KEY, tokens::GRID_CELL_KEY_COLUMN),
            column(DESCRIPTION, tokens::GRID_DESCRIPTION_COLUMN),
        ])
    }

    /// The width of column `column` ([`KEY`] or [`DESCRIPTION`]) as a number.
    pub(crate) fn get(self, column: usize) -> f32 {
        self.0.get(column).copied().unwrap_or_default()
    }

    pub(crate) fn key(self) -> Pixels {
        Pixels::from(self.get(KEY))
    }

    pub(crate) fn description(self) -> Pixels {
        Pixels::from(self.get(DESCRIPTION))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_grid_of_a_request_has_its_own_kind() {
        assert_eq!(GridKind::of("params", "param"), GridKind::Params);
        assert_eq!(GridKind::of("headers", "header"), GridKind::Headers);
        assert_eq!(GridKind::of("body", "field"), GridKind::Urlencoded);
        assert_eq!(GridKind::of("body", "part"), GridKind::FormData);
    }

    #[test]
    fn each_kind_has_its_own_table_id() {
        let ids = [
            GridKind::Params,
            GridKind::Headers,
            GridKind::Urlencoded,
            GridKind::FormData,
        ]
        .map(GridKind::table);
        for (index, id) in ids.iter().enumerate() {
            assert!(!ids.iter().skip(index + 1).any(|other| other == id));
        }
    }
}
