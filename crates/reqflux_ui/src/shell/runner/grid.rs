//! The results grid, `runner-grid`: DBFlux's result grid from `bolt` (`render_rows` over a
//! `RowSource`), one row per step and then the requests the run has not reached. A failed
//! step draws the grid's error row, the request in flight its saving row, and the selected
//! step the active row. A click on a finished row selects it.

use std::sync::Arc;

use bolt::components::grid::{
    GridCellCoord, GridColors, GridHost, HeaderResize, render_header, render_rows,
};
use bolt::data::{
    CellAlignment, CellTone, CellView, ColumnKind, ColumnView, RowVisualState, StaticRows,
};
use bolt::tokens::GridMetrics;
use gpui::{App, Context, Div, ParentElement, Pixels, Stateful, Styled, prelude::*, uniform_list};
use gpui_kit_assets::IconName;

use super::RunTab;
use super::model::{GridRow, RowStatus, Tone, cells};
use crate::shell::request::ui;
use crate::shell::table_widths;
use crate::shell::{Shell, region};
use crate::tokens::RUNNER_COLUMNS;

/// The id of the grid in the table store; a column is resized by its index in
/// `RUNNER_COLUMNS`, and every column has a width, since `bolt`'s grid has none that fills.
const TABLE: &str = "runner-grid";

/// The grid: its header row over the rows.
pub(super) fn render(run: &RunTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let rows = run.model.rows();
    let steps: Vec<Option<usize>> = rows
        .iter()
        .map(|row| match row.status {
            RowStatus::Done(step) => Some(step),
            RowStatus::Sending | RowStatus::Queued | RowStatus::Skipped => None,
        })
        .collect();
    let active = run
        .model
        .selected
        .and_then(|selected| steps.iter().position(|step| *step == Some(selected)))
        .map(|row| GridCellCoord::new(row, 1));
    let source = Arc::new(source(&rows, run));
    let defaults = RUNNER_COLUMNS.map(|(_, width)| width);
    let current: Vec<Pixels> = defaults
        .iter()
        .enumerate()
        .map(|(column, default)| table_widths::width(TABLE, column, *default, cx))
        .collect();
    let widths: Vec<f32> = current.iter().copied().map(f32::from).collect();
    let total_width = current
        .iter()
        .fold(GridMetrics::ROW_NUMBER_WIDTH, |total, width| total + *width);
    let total = f32::from(total_width);
    let count = rows.len();
    let shell = cx.entity().downgrade();
    let null_color = bolt::colors(cx).muted;
    let colors = GridColors::of(cx.global::<gpui_component::theme::Theme>());
    let (drag, hover) = table_widths::header_resize(cx);
    let header = render_header(
        "runner-grid-header-cell",
        source.as_ref(),
        &widths,
        bolt::fonts::grid_header_height(cx),
        total,
        GridMetrics::FONT,
        GridMetrics::TYPE_FONT,
        &colors,
        Some(HeaderResize { drag: &drag, hover }),
    );
    let rows_source = Arc::clone(&source);
    let list = uniform_list(
        "runner-grid-rows",
        count,
        move |range, _window, cx: &mut App| {
            let steps = steps.clone();
            let shell = shell.clone();
            let rows_source = Arc::clone(&rows_source);
            let on_click = Arc::new(
                move |coord: GridCellCoord,
                      _event: &gpui::ClickEvent,
                      _window: &mut gpui::Window,
                      cx: &mut App| {
                    let Some(Some(step)) = steps.get(coord.row).copied() else {
                        return;
                    };
                    if let Err(error) =
                        shell.update(cx, |shell, cx| shell.select_run_step(step, cx))
                    {
                        log::debug!("the run grid outlived its shell: {error}");
                    }
                },
            );
            let host = GridHost {
                column_widths: &widths,
                active,
                editing: None,
                is_selected: &|_| false,
                reference_row: false,
                on_cell_click: on_click,
                on_cell_context: Arc::new(|_, _, _, _| {}),
                braces_icon: ui::icon(IconName::Braces),
                chevron_icon: ui::icon(IconName::ChevronRight),
                reference_icon: ui::icon(IconName::Crosshair),
            };
            render_rows(
                range,
                rows_source.as_ref(),
                &host,
                None,
                total,
                GridMetrics::FONT,
                GridMetrics::ROW_HEIGHT,
                GridMetrics::HEADER_ICON,
                null_color,
                &GridColors::of(cx.global::<gpui_component::theme::Theme>()),
            )
        },
    )
    .flex_1()
    .min_h_0();
    // The header and the rows scroll sideways together on the one handle the table store keeps,
    // so a column stays over its own cells at every offset.
    let overflow = table_widths::overflow("runner-grid-scroll".to_owned(), total_width, cx)
        .header(region("runner-grid-header").flex_none().child(header))
        .rows(list)
        .flex_1()
        .min_h_0()
        .w_full();
    table_widths::resizable(region("runner-grid"), TABLE, cx)
        .role(gpui::Role::Grid)
        .aria_label("Run results")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .child(overflow)
}

/// The grid's rows as `bolt`'s row source: a failed step is an error row and the request
/// in flight a saving row.
fn source(rows: &[GridRow], run: &RunTab) -> StaticRows {
    let mut source = StaticRows::new();
    for (name, width) in RUNNER_COLUMNS {
        source.push_column(ColumnView::new(name, ColumnKind::Text).with_fixed_width(width));
    }
    for row in rows {
        let state = match row.status {
            RowStatus::Done(step)
                if run.model.result(step).is_some_and(|result| result.failed()) =>
            {
                RowVisualState::Error
            }
            RowStatus::Sending => RowVisualState::Saving,
            RowStatus::Done(_) | RowStatus::Queued | RowStatus::Skipped => RowVisualState::Clean,
        };
        let cells = cells(row, &run.model)
            .into_iter()
            .map(|(text, tone)| CellView::new(text, cell_tone(tone), CellAlignment::Left))
            .collect();
        source.push_row(cells, state);
    }
    source
}

/// The grid tone of a cell: status and test outcomes draw in the grid's status colors; the
/// request in flight has no accent tone in the grid and draws in the ordinary color.
fn cell_tone(tone: Tone) -> CellTone {
    match tone {
        Tone::Muted => CellTone::Muted,
        Tone::Success => CellTone::Success,
        Tone::Warning => CellTone::Warning,
        Tone::Danger => CellTone::Danger,
        Tone::Normal | Tone::Accent => CellTone::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tones_draw_in_the_grid_status_colors() {
        assert_eq!(cell_tone(Tone::Success), CellTone::Success);
        assert_eq!(cell_tone(Tone::Warning), CellTone::Warning);
        assert_eq!(cell_tone(Tone::Danger), CellTone::Danger);
        assert_eq!(cell_tone(Tone::Muted), CellTone::Muted);
        assert_eq!(cell_tone(Tone::Normal), CellTone::Normal);
        assert_eq!(cell_tone(Tone::Accent), CellTone::Normal);
    }
}
