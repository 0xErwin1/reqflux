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

use bolt::chamfer::Chamfer;
use bolt::tokens::{Colors, border, cut, family, icon, space, text};
use bolt::{Icon, IconName};
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, ParentElement, Pixels, Role, Stateful, Styled,
    Toggled, div, prelude::*,
};
use gpui_component::input::{Enter, Escape, Input};
use reqflux_core::model::Variable;
use reqflux_engine::{SecretKey, SecretScope};

use super::rows::{labeled_row, part};
use super::settings_view::{ButtonTone, button};
use super::variables_model::{Column, current_keys, empty_count, is_empty, undeclared};
use super::{Shell, region, text as text_role};
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
        .pb(space::XL)
        .flex()
        .flex_col()
        .child(heading)
        .child(
            region("variables-table")
                .role(Role::Table)
                .flex_none()
                .flex()
                .flex_col()
                .child(header(colors))
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
    let tint = bolt::brand(cx).accent_tint;
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
            .gap(space::HALF)
            .text_size(tokens::NOTICE_TEXT)
            .text_color(warning)
            .whitespace_nowrap()
            .child(
                Icon::new(IconName::TriangleAlert)
                    .size(icon::CHIP)
                    .color(warning),
            )
            .child(text)
    });
    let reset = resettable.then(|| {
        button(
            "variables-reset-current",
            Some(IconName::RotateCcw),
            "Reset current values",
            ButtonTone::Neutral,
            tokens::VARIABLES_HEADING_ACTION,
            cx,
        )
        .ml(space::SM)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.reset_current_values(scope, window, cx);
        }))
    });
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .pt(tokens::VARIABLES_HEADING_TOP)
        .pb(space::HALF)
        .mb(space::HALF)
        .border_b(border::THIN)
        .border_color(line)
        .child(Icon::new(IconName::Variable).size(icon::ITEM).color(tint))
        .child(
            text_role::role(
                labeled_row("variables-title".to_owned(), title.clone()),
                text::LABEL,
                cx,
            )
            .child(title),
        )
        .child(div().flex_1())
        .children(empty_label)
        .children(reset)
}

fn header(colors: &Colors) -> Div {
    let muted = colors.muted;
    let hint = move |text: &'static str| {
        div()
            .ml(space::HALF)
            .text_size(tokens::SMALL_MONO_TEXT)
            .font_weight(FontWeight::NORMAL)
            .text_color(muted)
            .child(text)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(tokens::GRID_HEADER)
        .bg(colors.background)
        .font_family(family::MONO)
        .text_size(tokens::GRID_HEADER_TEXT)
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
        .font_family(family::INTERFACE)
        .text_size(tokens::NOTICE_TEXT)
        .text_color(muted)
        .gap(space::HALF)
        .child(Icon::new(IconName::Lock).size(icon::SM).color(muted))
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
            cell.font_family(family::INTERFACE)
                .text_size(tokens::NOTICE_TEXT)
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
    .font_family(family::INTERFACE)
    .text_size(tokens::NOTICE_TEXT)
    .text_color(if empty { warning } else { muted })
    .child(description_text.clone())
    .into_any_element();
    let delete = delete_control(scope, n, &variable.key, cx);
    let colors = bolt::colors(cx);
    let number_color = if empty { colors.warning } else { colors.line_2 };
    row(format!("variables-row-{n}"), variable.key.clone(), colors)
        .text_color(colors.foreground)
        .when(empty, |row| row.bg(tokens::unresolved_row_wash(colors)))
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
        .pl(space::XS)
        .pr(space::XS)
        .bg(tokens::wash(warning, tokens::VARIABLES_EMPTY_TAG_WASH))
        .border_b(border::THIN)
        .border_color(warning)
        .text_color(warning)
        .child("empty")
}

fn undeclared_heading(cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let tint = bolt::brand(cx).accent_tint;
    let description = "Current values set by scripts with no declared variable. Not sent to \
                       the file.";
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .pt(tokens::VARIABLES_SECOND_HEADING_TOP)
        .pb(space::HALF)
        .mb(space::HALF)
        .border_b(border::THIN)
        .border_color(colors.line)
        .child(Icon::new(IconName::Info).size(icon::ITEM).color(tint))
        .child(
            text_role::role(
                labeled_row(
                    "variables-undeclared-title".to_owned(),
                    "ONLY ON THIS MACHINE".to_owned(),
                ),
                text::LABEL,
                cx,
            )
            .child("ONLY ON THIS MACHINE"),
        )
        .child(
            text_role::role(div().ml(space::SM), text::CAPTION, cx)
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
    let declare = button(
        &format!("variables-declare-{n}"),
        Some(IconName::ArrowUp),
        "Declare",
        ButtonTone::Neutral,
        tokens::VARIABLES_ROW_ACTION,
        cx,
    )
    .on_click(cx.listener(move |this, _event, window, cx| {
        this.declare_undeclared(scope, n, window, cx);
    }));
    let muted = bolt::colors(cx).muted;
    let discard = part(format!("variables-discard-{n}"))
        .role(Role::Button)
        .aria_label(format!("Discard the current value of {key}"))
        .cursor_pointer()
        .flex_none()
        .w(tokens::VARIABLES_DISCARD_WIDTH)
        .h(tokens::VARIABLES_ROW_ACTION)
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(IconName::Close).size(icon::CHIP).color(muted))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.discard_undeclared(scope, n, window, cx);
        }));
    let colors = bolt::colors(cx);
    row(format!("variables-undeclared-{n}"), key.to_owned(), colors)
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
            .font_family(family::INTERFACE)
            .text_size(tokens::NOTICE_TEXT)
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
                .gap(space::HALF)
                .pl(space::SM)
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
            .gap(space::SM)
            .child(Icon::new(IconName::Lock).size(icon::CHIP).color(muted))
            .child(prefix)
            .child(
                div()
                    .font_family(family::MONO)
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
            .gap(space::SM)
            .child(Icon::new(IconName::Info).size(icon::CHIP).color(muted))
            .child(text)
    });
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(space::SM)
        .mt(space::LG)
        .text_size(tokens::NOTICE_TEXT)
        .text_color(muted)
        .children(keyring)
        .children(declare)
}

fn row(id: String, label: String, colors: &Colors) -> Stateful<Div> {
    labeled_row(id, label)
        .role(Role::Row)
        .flex_none()
        .flex()
        .items_center()
        .h(tokens::GRID_ROW)
        .border_b(border::THIN)
        .border_color(colors.row_divider)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
}

fn number(n: usize, color: Hsla) -> Div {
    div()
        .w(tokens::VARIABLES_NUMBER_COLUMN)
        .flex_none()
        .pr(space::MD)
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
                Input::new(&edit.input)
                    .id("variables-cell-input")
                    .appearance(false)
                    .bordered(false)
                    .flex_1()
                    .min_w_0()
                    .text_size(tokens::GRID_TEXT)
                    .font_family(family::MONO)
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
        .pl(space::MD)
        .pr(space::MD)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap();
    if width > Pixels::ZERO {
        cell.w(width)
    } else {
        cell
    }
}

/// The enable box `variables-enabled-<n>`: a cut box, filled with the accent and checked
/// when enabled.
fn enable_box(scope: SecretScope, n: usize, variable: &Variable, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let brand = bolt::brand(cx);
    let chamfer = if variable.enabled {
        Chamfer::new(cut::KEYCAP).fill(brand.accent)
    } else {
        Chamfer::new(cut::KEYCAP).edge(colors.line_2, border::FOCUS_RING)
    };
    let on_accent = brand.on_accent;
    div()
        .flex_none()
        .w(tokens::VARIABLES_ON_COLUMN)
        .pl(tokens::VARIABLES_NARROW_PADDING)
        .pr(tokens::VARIABLES_NARROW_PADDING)
        .child(
            part(format!("variables-enabled-{n}"))
                .role(Role::CheckBox)
                .aria_label(format!("Enable {}", variable.key))
                .aria_toggled(toggled(variable.enabled))
                .cursor_pointer()
                .relative()
                .w(tokens::VARIABLES_CHECKBOX)
                .h(tokens::VARIABLES_CHECKBOX)
                .flex()
                .items_center()
                .justify_center()
                .child(chamfer)
                .when(variable.enabled, |this| {
                    this.child(Icon::new(IconName::Check).size(icon::SM).color(on_accent))
                })
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.toggle_variable_enabled(scope, n, window, cx);
                })),
        )
}

/// The `stored in` toggle `variables-secret-<n>`: the `file` pill, or the `KEYRING` badge
/// while the variable is secret, as `variables-stored-<n>`. A click moves the value between
/// them.
fn stored_toggle(
    scope: SecretScope,
    n: usize,
    variable: &Variable,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let (label, icon_name, mono) = if variable.secret {
        ("KEYRING", IconName::KeyRound, false)
    } else {
        ("file", IconName::File, true)
    };
    let pill = labeled_row(format!("variables-stored-{n}"), label.to_owned())
        .relative()
        .flex()
        .items_center()
        .gap(space::XS)
        .h(tokens::VARIABLES_PILL)
        .pl(tokens::VARIABLES_PILL_PADDING)
        .pr(tokens::VARIABLES_PILL_PADDING)
        .font_family(family::INTERFACE)
        .text_size(if mono {
            tokens::VARIABLES_PILL_TEXT
        } else {
            tokens::BADGE_TEXT
        })
        .font_weight(if mono {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::BOLD
        })
        .text_color(colors.muted)
        .child(Chamfer::new(cut::KEYCAP).fill(colors.raised))
        .child(
            Icon::new(icon_name)
                .size(tokens::VARIABLES_PILL_ICON)
                .color(colors.muted),
        )
        .child(label);
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
        .pl(space::MD)
        .child(pill)
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.toggle_variable_secret(scope, n, window, cx);
        }))
}

fn delete_control(
    scope: SecretScope,
    n: usize,
    key: &str,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let muted = bolt::colors(cx).muted;
    part(format!("variables-delete-{n}"))
        .role(Role::Button)
        .aria_label(format!("Delete {key}"))
        .cursor_pointer()
        .flex_none()
        .pl(space::MD)
        .pr(space::MD)
        .child(Icon::new(IconName::Close).size(icon::CHIP).color(muted))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.delete_variable(scope, n, window, cx);
        }))
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
        .h(tokens::GRID_ROW)
        .font_family(family::MONO)
        .text_size(tokens::GRID_TEXT)
        .text_color(ghost)
        .hover(|style| style.bg(colors.hover))
        .child(
            div()
                .flex_none()
                .w(tokens::VARIABLES_NUMBER_COLUMN)
                .flex()
                .justify_end()
                .child(Icon::new(IconName::Plus).size(icon::SM).color(ghost)),
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
        .pl(space::MD)
        .pr(space::MD)
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
        .pl(space::MD)
        .pr(space::MD)
        .overflow_hidden()
        .whitespace_nowrap()
}
