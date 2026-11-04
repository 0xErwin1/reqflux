//! The resizable column widths of the request grids. A width belongs to the kind of grid, not
//! to a request, so every request's Params grid shares one key width, every Headers grid
//! another, and so on. They live in memory for the session; `bolt` holds no widths, only the
//! drag and the clamp.

use bolt::components::grid::{ColumnResize, apply_column_resize};
use gpui::{Context, Pixels};

use super::Shell;
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
}

/// The index of the key column in a [`GridWidths`] resize.
pub(crate) const KEY: usize = 0;
/// The index of the description column in a [`GridWidths`] resize.
pub(crate) const DESCRIPTION: usize = 1;

/// The widths of the resizable columns of a grid, key then description; the value column
/// fills the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GridWidths([f32; 2]);

impl Default for GridWidths {
    fn default() -> Self {
        Self([
            tokens::GRID_CELL_KEY_COLUMN.into(),
            tokens::GRID_DESCRIPTION_COLUMN.into(),
        ])
    }
}

impl GridWidths {
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

    /// Takes the requested width, clamped, and leaves the other column as it was.
    fn apply(&mut self, resize: ColumnResize) -> bool {
        apply_column_resize(&mut self.0, resize)
    }
}

impl Shell {
    /// The widths of the grids of `kind`.
    pub(super) fn grid_widths(&self, kind: GridKind) -> GridWidths {
        self.grid_widths.get(&kind).copied().unwrap_or_default()
    }

    /// Stores the width a drag asked for in the grids of `kind` and redraws.
    pub(super) fn resize_grid_column(
        &mut self,
        kind: GridKind,
        resize: ColumnResize,
        cx: &mut Context<Self>,
    ) {
        let widths = self.grid_widths.entry(kind).or_default();
        if widths.apply(resize) {
            cx.notify();
        }
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
    fn a_resize_changes_one_column_and_clamps() {
        let mut widths = GridWidths::default();
        let description = widths.get(DESCRIPTION);
        assert!(widths.apply(ColumnResize {
            column: KEY,
            width: 1.0,
        }));
        assert_eq!(widths.get(KEY), bolt::components::grid::MIN_COLUMN_WIDTH);
        assert_eq!(widths.get(DESCRIPTION), description);
    }
}
