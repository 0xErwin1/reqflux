//! The results grid, `runner-grid`: DBFlux's result grid from `bolt` (`render_rows` over a
//! `RowSource`), one row per step and then the requests the run has not reached. A failed
//! step draws the grid's error row, the request in flight its saving row, and the selected
//! step the active row. A click on a finished row selects it.

use std::sync::Arc;

use bolt::components::grid::{GridCellCoord, GridColors, GridHost, render_rows};
use bolt::data::{
    CellAlignment, CellTone, CellView, ColumnKind, ColumnView, RowVisualState, StaticRows,
};
use bolt::tokens::{Borders, GridMetrics};
use gpui::{
    AnyElement, App, Context, Div, ParentElement, Pixels, Stateful, Styled, div, prelude::*,
    uniform_list,
};
use gpui_kit_assets::IconName;

use super::RunTab;
use super::model::{GridRow, RowStatus, Tone, cells};
use crate::shell::request::ui;
use crate::shell::{Shell, region};
use crate::tokens::RUNNER_COLUMNS;

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
    let source = source(&rows, run);
    let widths: Vec<f32> = RUNNER_COLUMNS
        .iter()
        .map(|(_, width)| f32::from(*width))
        .collect();
    let total_width = RUNNER_COLUMNS
        .iter()
        .fold(GridMetrics::ROW_NUMBER_WIDTH, |total, (_, width)| {
            total + *width
        });
    let total = f32::from(total_width);
    let count = rows.len();
    let shell = cx.entity().downgrade();
    let null_color = bolt::colors(cx).muted;
    let list = uniform_list(
        "runner-grid-rows",
        count,
        move |range, _window, cx: &mut App| {
            let steps = steps.clone();
            let shell = shell.clone();
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
                &source,
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
    region("runner-grid")
        .role(gpui::Role::Grid)
        .aria_label("Run results")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .font_family(bolt::fonts::editor_family(cx))
        .child(header(total_width, cx))
        .child(list)
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

/// The grid tone of a cell.
// bolt gap: `CellTone` has no success, warning or danger text; status and tests draw in the
// ordinary color and the row state carries the failure.
fn cell_tone(tone: Tone) -> CellTone {
    match tone {
        Tone::Muted => CellTone::Muted,
        Tone::Normal | Tone::Accent | Tone::Success | Tone::Warning | Tone::Danger => {
            CellTone::Normal
        }
    }
}

/// The header row, `runner-grid-header`: the row number gutter, then the column names.
// bolt gap: the grid has no header row renderer; DBFlux's data table draws its own.
fn header(total: Pixels, cx: &App) -> AnyElement {
    let colors = bolt::colors(cx);
    region("runner-grid-header")
        .flex_none()
        .flex()
        .w(total)
        .h(GridMetrics::HEADER_HEIGHT)
        .border_b(Borders::THIN)
        .border_color(colors.line_2)
        .text_size(GridMetrics::FONT)
        .text_color(colors.muted)
        .child(div().flex_none().w(GridMetrics::ROW_NUMBER_WIDTH))
        .children(RUNNER_COLUMNS.iter().map(|(name, width)| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .w(*width)
                .pl(GridMetrics::CELL_PADDING_X)
                .pr(GridMetrics::CELL_PADDING_X)
                .child(*name)
        }))
        .into_any_element()
}
