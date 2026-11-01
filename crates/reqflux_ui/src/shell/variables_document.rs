//! The variables column of the workspace settings page (variables-editor S6; design-catch-up
//! T7, design/variables/01-settings-page.html): the variables of one scope as a grid, then
//! the current values of keys the scope does not declare, then the notes.
//!
//! The heading `variables-title` carries the count of empty variables (`variables-empty`) and
//! `Reset current values` (`variables-reset-current`). Each row `variables-row-<n>` has its
//! number, enable box, key, initial value (in the file), current value (on this machine),
//! where the value is stored (`file` or `KEYRING`, a toggle), description and delete control;
//! the ghost row `variables-add` appends one. A secret has no initial value in the file and
//! shows its current value as `••••••••`. The undeclared rows `variables-undeclared-<n>`
//! show a key a script set, its editable current value, `Declare` and a discard control.
//!
//! It draws with DBFlux's `bolt`: the grid sizes of `GridMetrics` (header 40, rows 31),
//! `Checkbox`, `Badge`, `Button`, and the section labels of `HeaderMetrics`.

use bolt::Colors;
use bolt::controls::{Button, Checkbox, GpuiInput, InputEnter as Enter, InputEscape as Escape};
use bolt::primitives::{Badge, BadgeTone, Icon, TextVariant};
use bolt::tokens::{Borders, FontSizes, GridMetrics, HeaderMetrics, ResultMetrics, Spacing};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, ParentElement, Pixels, Role, Stateful, Styled,
    Toggled, div, prelude::*,
};
use reqflux_core::model::Variable;
use reqflux_engine::{SecretKey, SecretScope};

use super::request::ui;
use super::rows::{labeled_row, part};
use super::variables_model::{Column, current_keys, empty_count, is_empty, undeclared};
use super::{Shell, region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// What a secret's current value shows.
const MASK: &str = "••••••••";

/// What a secret's initial value shows.
const NOT_IN_FILE: &str = "not in the file";

/// The variables column of `scope`, named `name`, whose file is `file`.
pub(super) fn render(
    shell: &Shell,
    scope: SecretScope,
    name: &str,
    file: &str,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let variables = shell.variables_of(scope);
    let extra = undeclared(&shell.current, scope, &variables);
    let rows: Vec<_> = variables
        .iter()
        .enumerate()
        .map(|(index, variable)| declared_row(shell, scope, index + 1, variable, cx))
        .collect();
    let extra_rows: Vec<_> = extra
        .iter()
        .enumerate()
        .map(|(index, (key, value))| undeclared_row(shell, scope, index + 1, key, value, cx))
        .collect();
    let title = match scope {
        SecretScope::Global => "GLOBAL VARIABLES".to_owned(),
        SecretScope::Collection(_) | SecretScope::Environment(_) => {
            format!("VARIABLES IN {}", name.to_uppercase())
        }
    };
    let heading = heading(shell, scope, &variables, title, cx);
    let add = add_row(scope, cx);
    let colors = bolt::colors(cx);
    let mut column = region("variables-document")
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        .pl(tokens::VARIABLES_PANEL_INSET)
        .pr(tokens::VARIABLES_PANEL_INSET)
        .pb(Spacing::XL)
        .flex()
        .flex_col()
        .child(heading)
        .child(
            region("variables-table")
                .role(Role::Table)
                .flex_none()
                .flex()
                .flex_col()
                .child(header(&colors, cx))
                .children(rows)
                .child(add),
        );
    if !extra_rows.is_empty() {
        column = column.child(undeclared_heading(cx)).child(
            region("variables-undeclared")
                .role(Role::Table)
                .flex_none()
                .flex()
                .flex_col()
                .children(extra_rows),
        );
    }
    column
        .child(notes(shell, scope, &variables, !extra.is_empty(), file, cx))
        .into_any_element()
}

/// The section heading: the variable icon and `title`, the count of empty variables and
/// `Reset current values` while a declared variable holds a current value.
fn heading(
    shell: &Shell,
    scope: SecretScope,
    variables: &[Variable],
    title: String,
    cx: &mut Context<Shell>,
) -> Div {
    let empty = empty_count(&shell.current, scope, variables);
    let resettable = !current_keys(&shell.current, scope, variables).is_empty();
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let warning = colors.warning;
    let line = colors.line;
    let empty_label = (empty > 0).then(|| {
        let text = if empty == 1 {
            "1 empty variable".to_owned()
        } else {
            format!("{empty} empty variables")
        };
        labeled_row("variables-empty".to_owned(), text.clone())
            .flex()
            .items_center()
            .gap(ResultMetrics::FOOTER_ITEM_GAP)
            .text_size(FontSizes::XS)
            .text_color(warning)
            .whitespace_nowrap()
            .child(
                Icon::new(ui::icon(IconName::TriangleAlert))
                    .size(ResultMetrics::FOOTER_ICON)
                    .color(warning),
            )
            .child(text)
    });
    let reset = resettable.then(|| {
        Button::new("variables-reset-current", "Reset current values")
            .secondary()
            .inline()
            .icon(ui::icon(IconName::RotateCcw))
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.reset_current_values(scope, window, cx);
            }))
    });
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .pt(HeaderMetrics::LABEL_PADDING_TOP)
        .pb(HeaderMetrics::LABEL_PADDING_BOTTOM)
        .mb(HeaderMetrics::LABEL_PADDING_BOTTOM)
        .border_b(Borders::THIN)
        .border_color(line)
        .child(
            Icon::new(ui::icon(IconName::Variable))
                .size(HeaderMetrics::LABEL_ICON)
                .color(tint),
        )
        .child(
            ui::role(
                labeled_row("variables-title".to_owned(), title.clone()),
                TextVariant::Label,
                cx,
            )
            .text_size(HeaderMetrics::LABEL_FONT)
            .child(title),
        )
        .child(div().flex_1())
        .children(empty_label)
        .children(reset)
}

fn header(colors: &Colors, cx: &Context<Shell>) -> Div {
    let muted = colors.muted;
    let hint = move |text: &'static str| {
        div()
            .ml(GridMetrics::HEADER_GAP)
            .text_size(GridMetrics::TYPE_FONT)
            .font_weight(FontWeight::NORMAL)
            .text_color(muted)
            .child(text)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(GridMetrics::HEADER_HEIGHT)
        .bg(colors.background)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.strong)
        .child(column(tokens::VARIABLES_NUMBER_COLUMN))
        .child(narrow(tokens::VARIABLES_ON_COLUMN).child("on"))
        .child(column(tokens::VARIABLES_KEY_COLUMN).child("key"))
        .child(
            column(tokens::VARIABLES_VALUE_COLUMN)
                .flex()
                .child("initial value")
                .child(hint("in file")),
        )
        .child(
            column(tokens::VARIABLES_CURRENT_COLUMN)
                .flex()
                .child("current")
                .child(hint("this machine")),
        )
        .child(column(tokens::VARIABLES_STORED_COLUMN).child("stored in"))
        .child(fill().child("description"))
}

fn declared_row(
    shell: &Shell,
    scope: SecretScope,
    n: usize,
    variable: &Variable,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let current_value = shell.current.get(scope, &variable.key).map(str::to_owned);
    let empty = is_empty(variable, current_value.as_deref());
    let colors = bolt::colors(cx);
    let ui_family = bolt::fonts::ui_family(cx);
    let (muted, strong, warning) = (colors.muted, colors.strong, colors.warning);
    let dim = |cell: Stateful<Div>| {
        if variable.enabled {
            cell
        } else {
            cell.opacity(tokens::VARIABLES_DISABLED_OPACITY)
        }
    };
    let enable = enable_box(scope, n, variable, cx);
    let key = dim(cell(
        shell,
        scope,
        Column::Key,
        n,
        tokens::VARIABLES_KEY_COLUMN,
        &variable.key,
        cx,
    ))
    .text_color(strong)
    .child(variable.key.clone())
    .into_any_element();
    let initial = variable.value.clone().unwrap_or_default();
    let value = if variable.secret {
        dim(static_cell(
            Column::Value.element_id(n),
            tokens::VARIABLES_VALUE_COLUMN,
            NOT_IN_FILE,
        ))
        .font_family(ui_family.clone())
        .text_size(FontSizes::XS)
        .text_color(muted)
        .gap(GridMetrics::HEADER_GAP)
        .child(
            Icon::new(ui::icon(IconName::Lock))
                .size(GridMetrics::HEADER_ICON)
                .color(muted),
        )
        .child(NOT_IN_FILE)
        .into_any_element()
    } else {
        let cell = dim(cell(
            shell,
            scope,
            Column::Value,
            n,
            tokens::VARIABLES_VALUE_COLUMN,
            &initial,
            cx,
        ));
        if initial.is_empty() && !shell.is_editing(scope, Column::Value, n) {
            cell.child(empty_tag(warning)).into_any_element()
        } else {
            cell.child(initial.clone()).into_any_element()
        }
    };
    let current = if variable.secret {
        dim(cell(
            shell,
            scope,
            Column::Current,
            n,
            tokens::VARIABLES_CURRENT_COLUMN,
            MASK,
            cx,
        ))
        .when(!shell.is_editing(scope, Column::Current, n), |cell| {
            cell.child(
                part(format!("variables-masked-{n}"))
                    .text_color(muted)
                    .child(MASK),
            )
        })
        .into_any_element()
    } else {
        let shown = current_value.clone().unwrap_or_default();
        let cell = dim(cell(
            shell,
            scope,
            Column::Current,
            n,
            tokens::VARIABLES_CURRENT_COLUMN,
            &shown,
            cx,
        ));
        if shell.is_editing(scope, Column::Current, n) {
            cell.into_any_element()
        } else if empty {
            cell.child(empty_tag(warning)).into_any_element()
        } else if current_value.is_none() {
            cell.font_family(ui_family.clone())
                .text_size(FontSizes::XS)
                .text_color(muted)
                .child("= initial")
                .into_any_element()
        } else {
            cell.child(shown).into_any_element()
        }
    };
    let stored = stored_toggle(scope, n, variable, cx);
    let description_text = variable.description.clone().unwrap_or_default();
    let description = cell(
        shell,
        scope,
        Column::Description,
        n,
        Pixels::ZERO,
        &description_text,
        cx,
    )
    .flex_1()
    .font_family(ui_family)
    .text_size(FontSizes::XS)
    .text_color(if empty { warning } else { muted })
    .child(description_text.clone())
    .into_any_element();
    let delete = delete_control(scope, n, &variable.key, cx);
    let number_color = if empty { colors.warning } else { colors.line_2 };
    row(
        format!("variables-row-{n}"),
        variable.key.clone(),
        &colors,
        cx,
    )
    .text_color(colors.foreground)
    .when(empty, |row| row.bg(tokens::unresolved_wash(colors.warning)))
    .child(number(n, number_color))
    .child(enable)
    .child(key)
    .child(value)
    .child(current)
    .child(stored)
    .child(description)
    .child(delete)
    .into_any_element()
}

/// The `empty` tag of a value that resolves to nothing.
fn empty_tag(warning: Hsla) -> Div {
    div()
        .pl(Spacing::XS)
        .pr(Spacing::XS)
        .bg(tokens::variable_wash(warning))
        .border_b(Borders::THIN)
        .border_color(warning)
        .text_color(warning)
        .child("empty")
}

fn undeclared_heading(cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let description = "Current values set by scripts with no declared variable. Not sent to \
                       the file.";
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .pt(HeaderMetrics::SECTION_PADDING_TOP)
        .pb(HeaderMetrics::LABEL_PADDING_BOTTOM)
        .mb(HeaderMetrics::LABEL_PADDING_BOTTOM)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            Icon::new(ui::icon(IconName::Info))
                .size(HeaderMetrics::LABEL_ICON)
                .color(tint),
        )
        .child(
            ui::role(
                labeled_row(
                    "variables-undeclared-title".to_owned(),
                    "ONLY ON THIS MACHINE".to_owned(),
                ),
                TextVariant::Label,
                cx,
            )
            .text_size(HeaderMetrics::LABEL_FONT)
            .child("ONLY ON THIS MACHINE"),
        )
        .child(
            ui::role(div().ml(Spacing::SM), TextVariant::Caption, cx)
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(description),
        )
}

fn undeclared_row(
    shell: &Shell,
    scope: SecretScope,
    n: usize,
    key: &str,
    value: &str,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let current = cell(
        shell,
        scope,
        Column::Undeclared,
        n,
        tokens::VARIABLES_CURRENT_COLUMN,
        value,
        cx,
    )
    .child(value.to_owned())
    .into_any_element();
    let declare = Button::new(format!("variables-declare-{n}"), "Declare")
        .secondary()
        .inline()
        .icon(ui::icon(IconName::ArrowUp))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.declare_undeclared(scope, n, window, cx);
        }));
    let discard = Button::new(
        format!("variables-discard-{n}"),
        format!("Discard the current value of {key}"),
    )
    .ghost()
    .inline()
    .icon(ui::icon(IconName::Close))
    .icon_only()
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.discard_undeclared(scope, n, window, cx);
    }));
    let colors = bolt::colors(cx);
    row(
        format!("variables-undeclared-{n}"),
        key.to_owned(),
        &colors,
        cx,
    )
    .text_color(colors.foreground)
    .child(
        div()
            .flex_none()
            .w(tokens::VARIABLES_NUMBER_COLUMN + tokens::VARIABLES_ON_COLUMN),
    )
    .child(
        static_cell(
            format!("variables-undeclared-key-{n}"),
            tokens::VARIABLES_KEY_COLUMN,
            key,
        )
        .text_color(colors.strong)
        .child(key.to_owned()),
    )
    .child(
        static_cell(
            format!("variables-undeclared-hint-{n}"),
            tokens::VARIABLES_VALUE_COLUMN,
            "not declared",
        )
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(FontSizes::XS)
        .text_color(colors.muted)
        .child("not declared"),
    )
    .child(current)
    .child(div().flex_none().w(tokens::VARIABLES_UNDECLARED_NOTE))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .justify_end()
            .gap(Spacing::XS)
            .pl(Spacing::SM)
            .pr(Spacing::SM)
            .child(declare)
            .child(discard),
    )
    .into_any_element()
}

/// The notes under the grids: where the first secret lives in the keyring, and what Declare
/// does while there are undeclared values.
fn notes(
    shell: &Shell,
    scope: SecretScope,
    variables: &[Variable],
    undeclared: bool,
    file: &str,
    cx: &mut Context<Shell>,
) -> Div {
    let colors = bolt::colors(cx);
    let (muted, foreground) = (colors.muted, colors.foreground);
    let workspace_id = match &shell.workspace {
        OpenedWorkspace::Loaded(engine) => Some(engine.workspace().manifest.id),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
    };
    let keyring = workspace_id
        .zip(variables.iter().find(|variable| variable.secret))
        .map(|(workspace_id, variable)| {
            let account = SecretKey {
                workspace_id,
                scope,
                key: variable.key.clone(),
            }
            .account();
            let prefix = format!(
                "Secrets never reach the file. Keyring entry for {}: ",
                variable.key
            );
            let entry = format!("reqflux · {account}");
            labeled_row(
                "variables-keyring-note".to_owned(),
                format!("{prefix}{entry}"),
            )
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(
                Icon::new(ui::icon(IconName::Lock))
                    .size(ResultMetrics::FOOTER_ICON)
                    .color(muted),
            )
            .child(prefix)
            .child(
                div()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_color(foreground)
                    .child(entry),
            )
        });
    let declare = undeclared.then(|| {
        let text = format!(
            "Declare keeps the key in {file} with an empty initial value; the current value \
             stays local."
        );
        labeled_row("variables-declare-note".to_owned(), text.clone())
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(
                Icon::new(ui::icon(IconName::Info))
                    .size(ResultMetrics::FOOTER_ICON)
                    .color(muted),
            )
            .child(text)
    });
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(Spacing::SM)
        .mt(Spacing::LG)
        .text_size(FontSizes::XS)
        .text_color(muted)
        .children(keyring)
        .children(declare)
}

fn row(id: String, label: String, colors: &Colors, cx: &Context<Shell>) -> Stateful<Div> {
    labeled_row(id, label)
        .role(Role::Row)
        .flex_none()
        .flex()
        .items_center()
        .h(GridMetrics::ROW_HEIGHT)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
}

fn number(n: usize, color: Hsla) -> Div {
    div()
        .w(tokens::VARIABLES_NUMBER_COLUMN)
        .flex_none()
        .pr(Spacing::SM)
        .text_right()
        .text_color(color)
        .child(n.to_string())
}

impl Shell {
    /// Whether the cell `column` of the 1-based row `n` of `scope` holds the open field.
    fn is_editing(&self, scope: SecretScope, column: Column, n: usize) -> bool {
        let owner = scope.path();
        self.cell_edit.as_ref().is_some_and(|edit| {
            edit.request_id == owner && edit.table == column.table() && edit.n == n
        })
    }
}

/// The cell `column` of the 1-based row `n`, labeled `label` for automation: the open field
/// when it is under edit, else a cell whose click opens it. A zero `width` leaves the width
/// to the caller.
fn cell(
    shell: &Shell,
    scope: SecretScope,
    column: Column,
    n: usize,
    width: Pixels,
    label: &str,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let base = static_cell(column.element_id(n), width, label);
    if shell.is_editing(scope, column, n)
        && let Some(edit) = shell.cell_edit.as_ref()
    {
        let foreground = bolt::colors(cx).foreground;
        // bolt gap: bolt's Input without its appearance keeps gpui-component's padding and
        // text size, so the open cell is gpui-component's frameless input.
        return base
            .on_action(cx.listener(|this, action: &Enter, window, cx| {
                if action.secondary {
                    this.send_active(window, cx);
                } else {
                    this.commit_cell_edit(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.discard_cell_edit(window, cx);
            }))
            .child(
                GpuiInput::new(&edit.input)
                    .id("variables-cell-input")
                    .appearance(false)
                    .bordered(false)
                    .p_0()
                    .flex_1()
                    .min_w_0()
                    .text_size(GridMetrics::FONT)
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_color(foreground),
            );
    }
    base.cursor_text()
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.start_variable_edit(scope, column, n, window, cx);
        }))
}

fn static_cell(id: String, width: Pixels, label: &str) -> Stateful<Div> {
    let cell = labeled_row(id, label.to_owned())
        .h_full()
        .flex()
        .items_center()
        .flex_none()
        .pl(GridMetrics::CELL_PADDING_X)
        .pr(GridMetrics::CELL_PADDING_X)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap();
    if width > Pixels::ZERO {
        cell.w(width)
    } else {
        cell
    }
}

/// The enable box `variables-enabled-<n>`, `bolt`'s checkbox, checked when enabled.
fn enable_box(scope: SecretScope, n: usize, variable: &Variable, cx: &mut Context<Shell>) -> Div {
    let shell = cx.entity().downgrade();
    div()
        .flex_none()
        .w(tokens::VARIABLES_ON_COLUMN)
        .pl(tokens::VARIABLES_NARROW_PADDING)
        .pr(tokens::VARIABLES_NARROW_PADDING)
        .child(
            Checkbox::new(gpui::ElementId::Name(
                format!("variables-enabled-{n}").into(),
            ))
            .checked(variable.enabled)
            .aria_label(format!("Enable {}", variable.key))
            .on_click(move |_checked, window, cx| {
                let toggled = shell.update(cx, |this, cx| {
                    this.toggle_variable_enabled(scope, n, window, cx);
                });
                if let Err(error) = toggled {
                    log::warn!("variable enable toggle: {error}");
                }
            }),
        )
}

/// The `stored in` toggle `variables-secret-<n>`: the `file` badge, or the `KEYRING` badge
/// while the variable is secret, as `variables-stored-<n>`. A click moves the value between
/// them.
// bolt gap: Badge takes no element id, so a labeled wrapper carries `variables-stored-<n>`.
fn stored_toggle(
    scope: SecretScope,
    n: usize,
    variable: &Variable,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let (label, icon_name) = if variable.secret {
        ("KEYRING", IconName::KeyRound)
    } else {
        ("file", IconName::File)
    };
    let pill = labeled_row(format!("variables-stored-{n}"), label.to_owned())
        .flex_none()
        .child(Badge::new(label, BadgeTone::Neutral).icon(ui::icon(icon_name)));
    part(format!("variables-secret-{n}"))
        .role(Role::CheckBox)
        .aria_label(format!("Keep {} in the keyring", variable.key))
        .aria_toggled(toggled(variable.secret))
        .cursor_pointer()
        .flex_none()
        .w(tokens::VARIABLES_STORED_COLUMN)
        .h_full()
        .flex()
        .items_center()
        .pl(GridMetrics::CELL_PADDING_X)
        .child(pill)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.toggle_variable_secret(scope, n, window, cx);
        }))
}

fn delete_control(scope: SecretScope, n: usize, key: &str, cx: &mut Context<Shell>) -> Div {
    div().flex_none().pl(Spacing::XS).pr(Spacing::XS).child(
        Button::new(format!("variables-delete-{n}"), format!("Delete {key}"))
            .ghost()
            .inline()
            .icon(ui::icon(IconName::Close))
            .icon_only()
            .tab_stop(false)
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.delete_variable(scope, n, window, cx);
            })),
    )
}

/// The ghost row `variables-add`: placeholders in the key and value columns; a click
/// appends a variable.
fn add_row(scope: SecretScope, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let ghost = colors.line_2;
    part("variables-add".to_owned())
        .role(Role::Button)
        .aria_label("Add a variable")
        .cursor_pointer()
        .flex_none()
        .flex()
        .items_center()
        .h(GridMetrics::ROW_HEIGHT)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(GridMetrics::FONT)
        .text_color(ghost)
        .hover(|style| style.bg(colors.hover))
        .child(
            div()
                .flex_none()
                .w(tokens::VARIABLES_NUMBER_COLUMN)
                .flex()
                .justify_end()
                .child(
                    Icon::new(ui::icon(IconName::Plus))
                        .size(GridMetrics::HEADER_ICON)
                        .color(ghost),
                ),
        )
        .child(div().flex_none().w(tokens::VARIABLES_ON_COLUMN))
        .child(column(tokens::VARIABLES_KEY_COLUMN).child("Variable"))
        .child(column(tokens::VARIABLES_VALUE_COLUMN).child("Initial value"))
        .child(column(tokens::VARIABLES_CURRENT_COLUMN).child("Current value"))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.add_variable(scope, window, cx);
        }))
}

fn toggled(on: bool) -> Toggled {
    if on { Toggled::True } else { Toggled::False }
}

fn column(width: Pixels) -> Div {
    div()
        .w(width)
        .flex_none()
        .pl(GridMetrics::CELL_PADDING_X)
        .pr(GridMetrics::CELL_PADDING_X)
        .overflow_hidden()
        .whitespace_nowrap()
}

fn narrow(width: Pixels) -> Div {
    div()
        .w(width)
        .flex_none()
        .pl(tokens::VARIABLES_NARROW_PADDING)
        .pr(tokens::VARIABLES_NARROW_PADDING)
}

fn fill() -> Div {
    div()
        .flex_1()
        .min_w_0()
        .pl(GridMetrics::CELL_PADDING_X)
        .pr(GridMetrics::CELL_PADDING_X)
        .overflow_hidden()
        .whitespace_nowrap()
}
