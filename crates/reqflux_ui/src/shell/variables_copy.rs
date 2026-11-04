//! Copying variables into an environment from the Globals and collection grids
//! (feedback-nov3 S13, design/variables/04-copy-to-environment.html).
//!
//! Each row has a selection box (`variables-select-<n>`) and the header one
//! (`variables-select-all`). `Copy to environment…` (`variables-copy`, enabled while a row is
//! selected) opens `variables-copy-dialog`: an environment select (`variables-copy-env`) over
//! the environments of the workspace, defaulting to the active one, and the policy for keys
//! the environment already has: skipped, or replaced with `variables-copy-overwrite` ticked.
//! A workspace with no environment offers `variables-copy-new-env` instead.
//!
//! The copy is the engine's [`Engine::copy_variables`]: initial values, descriptions and
//! enabled flags move, and a secret moves through the keyring (a keyring failure skips that
//! key). The result shows as the page's `variables-warning` line. The shell holds keys, never
//! a value.

use std::collections::BTreeSet;

use bolt::controls::{Button, Checkbox, Dropdown, DropdownItem, DropdownSelectionChanged};
use bolt::modals::{Modal, ModalFocus, modal_field};
use bolt::primitives::TextVariant;
use bolt::tokens::{ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, AppContext as _, Context, Div, Entity, IntoElement, ParentElement, Point,
    Role, SharedString, Styled, Subscription, Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::{CopyPolicy, CopyReport, Engine, SecretScope, SkipReason};

use crate::shell::request::ui;
use crate::shell::variables_model::{ScopeEntry, environments, scope_name};
use crate::shell::{Shell, region, text_region};
use crate::tokens;
use crate::workspace::OpenedWorkspace;

/// What the grid selection and the open dialog hold.
pub(crate) struct CopyState {
    /// The scope the selection belongs to; another scope's page starts with none selected.
    owner: Option<SecretScope>,
    /// The keys selected in `owner`.
    selected: BTreeSet<String>,
    dialog: Option<Dialog>,
    focus: ModalFocus,
}

/// The open `Copy to environment…` dialog.
struct Dialog {
    from: SecretScope,
    keys: Vec<String>,
    /// The environments the select lists, in its order.
    choices: Vec<ScopeEntry>,
    target: Option<usize>,
    overwrite: bool,
    select: Entity<Dropdown>,
    _selection: Subscription,
}

impl CopyState {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            owner: None,
            selected: BTreeSet::new(),
            dialog: None,
            focus: ModalFocus::new(cx),
        }
    }

    /// Forgets the selection and the dialog, as when the workspace changes.
    pub(crate) fn reset(&mut self) {
        self.owner = None;
        self.selected.clear();
        self.dialog = None;
    }
}

/// The user-facing `3 variables`.
fn variables(count: usize) -> String {
    match count {
        1 => "1 variable".to_owned(),
        count => format!("{count} variables"),
    }
}

/// The line that reports a copy into the environment `name`.
fn report_text(report: &CopyReport, name: &str) -> String {
    let mut text = format!("Copied {} to {name}", variables(report.copied.len()));
    for (reason, what) in [
        (SkipReason::Exists, "already there"),
        (
            SkipReason::Keyring,
            "whose keyring value could not be moved",
        ),
        (SkipReason::Missing, "no longer declared"),
    ] {
        let keys: Vec<&str> = report
            .skipped
            .iter()
            .filter(|skipped| skipped.reason == reason)
            .map(|skipped| skipped.key.as_str())
            .collect();
        if !keys.is_empty() {
            text.push_str(&format!(
                "; skipped {} {what}: {}",
                keys.len(),
                keys.join(", ")
            ));
        }
    }
    text.push('.');
    text
}

impl Shell {
    /// The keys of the declared variables of `scope` that are selected, in grid order.
    fn selected_variable_keys(&self, scope: SecretScope) -> Vec<String> {
        if self.settings.copy.owner != Some(scope) {
            return Vec::new();
        }
        self.variables_of(scope)
            .into_iter()
            .map(|variable| variable.key)
            .filter(|key| self.settings.copy.selected.contains(key))
            .collect()
    }

    /// The selection box of a row.
    fn toggle_variable_selected(&mut self, scope: SecretScope, key: &str, cx: &mut Context<Self>) {
        let copy = &mut self.settings.copy;
        if copy.owner != Some(scope) {
            copy.owner = Some(scope);
            copy.selected.clear();
        }
        if !copy.selected.remove(key) {
            copy.selected.insert(key.to_owned());
        }
        cx.notify();
    }

    /// The header box: selects every row, or none when every row is selected.
    fn toggle_all_variables_selected(&mut self, scope: SecretScope, cx: &mut Context<Self>) {
        let keys: Vec<String> = self
            .variables_of(scope)
            .into_iter()
            .map(|variable| variable.key)
            .collect();
        let all = !keys.is_empty() && self.selected_variable_keys(scope).len() == keys.len();
        let copy = &mut self.settings.copy;
        copy.owner = Some(scope);
        copy.selected = if all {
            BTreeSet::new()
        } else {
            keys.into_iter().collect()
        };
        cx.notify();
    }

    fn workspace_environments(&self, from: SecretScope) -> Vec<ScopeEntry> {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return Vec::new();
        };
        environments(engine.workspace())
            .into_iter()
            .filter(|entry| SecretScope::Environment(entry.id) != from)
            .collect()
    }

    /// `Copy to environment…`: opens the dialog over the selected keys of `scope`. Does
    /// nothing while no row is selected.
    pub(in crate::shell) fn open_copy_dialog(
        &mut self,
        scope: SecretScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(window, cx);
        let keys = self.selected_variable_keys(scope);
        if keys.is_empty() {
            return;
        }
        let choices = self.workspace_environments(scope);
        let target = self
            .active_environment
            .as_ref()
            .and_then(|slug| choices.iter().position(|entry| entry.slug == *slug))
            .or_else(|| (!choices.is_empty()).then_some(0));
        let select = cx.new(|_| {
            Dropdown::new("variables-copy-env")
                .items(environment_items(&choices))
                .selected_index(target)
                .placeholder("No environments")
                .aria_label("Environment to copy into")
        });
        let selection = cx.subscribe_in(
            &select,
            window,
            |this, _, event: &DropdownSelectionChanged, _window, cx| {
                if let Some(dialog) = this.settings.copy.dialog.as_mut() {
                    dialog.target = Some(event.index);
                    cx.notify();
                }
            },
        );
        self.settings.copy.dialog = Some(Dialog {
            from: scope,
            keys,
            choices,
            target,
            overwrite: false,
            select,
            _selection: selection,
        });
        self.settings.copy.focus.focus(None, window, cx);
        cx.notify();
    }

    /// Cancel, Escape or the close button: closes the dialog and changes nothing.
    pub(in crate::shell) fn close_copy_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.copy.dialog.take().is_some() {
            self.settings.copy.focus.restore(cx);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn toggle_copy_overwrite(&mut self, overwrite: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = self.settings.copy.dialog.as_mut() {
            dialog.overwrite = overwrite;
            cx.notify();
        }
    }

    /// `New environment` in the dialog of a workspace with none: creates one and selects it.
    fn create_copy_environment(&mut self, cx: &mut Context<Self>) {
        let outcome = self.with_engine(|engine| {
            engine
                .create_environment()
                .map(Some)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok((created, _)) => {
                self.environments_changed(cx);
                let Some(from) = self.settings.copy.dialog.as_ref().map(|dialog| dialog.from)
                else {
                    return;
                };
                let choices = self.workspace_environments(from);
                let target = created.and_then(|id| choices.iter().position(|e| e.id == id));
                if let Some(dialog) = self.settings.copy.dialog.as_mut() {
                    dialog.select.update(cx, |select, cx| {
                        select.set_items(environment_items(&choices), cx);
                        select.set_selected_index(target, cx);
                    });
                    dialog.choices = choices;
                    dialog.target = target;
                }
                cx.notify();
            }
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    /// The dialog's primary button: copies the keys into the chosen environment, reports
    /// what moved and closes the dialog. Does nothing without an environment.
    pub(in crate::shell) fn confirm_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.settings.copy.dialog.as_ref() else {
            return;
        };
        let Some(target) = dialog
            .target
            .and_then(|index| dialog.choices.get(index))
            .cloned()
        else {
            return;
        };
        let (from, keys) = (dialog.from, dialog.keys.clone());
        let policy = if dialog.overwrite {
            CopyPolicy::Overwrite
        } else {
            CopyPolicy::Skip
        };
        let mut report = None;
        let outcome = self.with_engine(|engine: &mut Engine| {
            report = Some(
                engine
                    .copy_variables(from, &keys, SecretScope::Environment(target.id), policy)
                    .map_err(|error| error.to_string())?,
            );
            Ok(None)
        });
        self.close_copy_dialog(window, cx);
        match (outcome, report) {
            (Ok(_), Some(report)) => {
                self.settings.copy.selected.clear();
                self.environments_changed(cx);
                self.show_variables_warning(Some(report_text(&report, &target.name)), cx);
                self.inspect_open_requests(cx);
            }
            (Ok(_), None) => {}
            (Err(error), _) => self.show_variables_error(error, cx),
        }
    }
}

fn environment_items(choices: &[ScopeEntry]) -> Vec<DropdownItem> {
    choices
        .iter()
        .map(|entry| DropdownItem::new(entry.name.clone()))
        .collect()
}

/// The selection box `variables-select-<n>` of the row of `key`.
pub(in crate::shell) fn select_box(
    shell: &Shell,
    scope: SecretScope,
    n: usize,
    key: &str,
    cx: &mut Context<Shell>,
) -> Div {
    let checked =
        shell.settings.copy.owner == Some(scope) && shell.settings.copy.selected.contains(key);
    let key = key.to_owned();
    column(
        Checkbox::new(gpui::ElementId::Name(
            format!("variables-select-{n}").into(),
        ))
        .checked(checked)
        .aria_label(format!("Select {key}"))
        .on_click(cx.listener(move |this, _checked, _window, cx| {
            this.toggle_variable_selected(scope, &key, cx);
        })),
    )
}

/// The header box `variables-select-all`.
pub(in crate::shell) fn select_all_box(
    shell: &Shell,
    scope: SecretScope,
    cx: &mut Context<Shell>,
) -> Div {
    let count = shell.variables_of(scope).len();
    let checked = count > 0 && shell.selected_variable_keys(scope).len() == count;
    column(
        Checkbox::new("variables-select-all")
            .checked(checked)
            .aria_label("Select every variable")
            .on_click(cx.listener(move |this, _checked, _window, cx| {
                this.toggle_all_variables_selected(scope, cx);
            })),
    )
}

/// The width the selection column takes in the grid's rows.
pub(in crate::shell) fn spacer() -> Div {
    div().flex_none().w(tokens::VARIABLES_ON_COLUMN)
}

fn column(child: impl IntoElement) -> Div {
    spacer()
        .pl(tokens::VARIABLES_NARROW_PADDING)
        .pr(tokens::VARIABLES_NARROW_PADDING)
        .child(child)
}

/// `Copy to environment…`, `variables-copy`, for the grids that can copy (Globals and a
/// collection); `None` on an environment's own page. Disabled while no row is selected.
pub(in crate::shell) fn copy_button(
    shell: &Shell,
    scope: SecretScope,
    cx: &mut Context<Shell>,
) -> Option<Button> {
    if matches!(scope, SecretScope::Environment(_)) {
        return None;
    }
    let selected = shell.selected_variable_keys(scope).len();
    let label = if selected == 0 {
        "Copy to environment…".to_owned()
    } else {
        format!("Copy {selected} to environment…")
    };
    Some(
        Button::new("variables-copy", label)
            .secondary()
            .inline()
            .icon(ui::icon(IconName::Copy))
            .disabled(selected == 0)
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.open_copy_dialog(scope, window, cx);
            })),
    )
}

/// The dialog, `variables-copy-dialog`, over the window while it is open.
pub(in crate::shell) fn dialog(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let dialog = shell.settings.copy.dialog.as_ref()?;
    let workspace = match &shell.workspace {
        OpenedWorkspace::Loaded(engine) => Some(engine.workspace()),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
    };
    let source = workspace
        .and_then(|workspace| scope_name(workspace, dialog.from))
        .unwrap_or_default();
    let secrets = workspace.map_or(0, |_| {
        shell
            .variables_of(dialog.from)
            .iter()
            .filter(|variable| variable.secret && dialog.keys.contains(&variable.key))
            .count()
    });
    let intro = format!(
        "Copy {} from {} into an environment of this workspace: {}.",
        variables(dialog.keys.len()),
        if source.is_empty() {
            "Globals"
        } else {
            &source
        },
        dialog.keys.join(", ")
    );
    let environment: AnyElement = if dialog.choices.is_empty() {
        div()
            .flex()
            .flex_col()
            .gap(Spacing::SM)
            .child(caption(
                "variables-copy-none",
                "This workspace has no environments yet.",
                cx,
            ))
            .child(
                Button::new("variables-copy-new-env", "New environment")
                    .secondary()
                    .inline()
                    .icon(ui::icon(IconName::Plus))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.create_copy_environment(cx);
                    })),
            )
            .into_any_element()
    } else {
        dialog.select.clone().into_any_element()
    };
    let mut body = div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::BODY_GAP)
        .child(caption("variables-copy-intro", intro, cx))
        .child(modal_field("Environment", environment, cx))
        .child(
            Checkbox::new("variables-copy-overwrite")
                .label("Overwrite keys the environment already has")
                .checked(dialog.overwrite)
                .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                    this.toggle_copy_overwrite(*checked, cx);
                })),
        )
        .child(caption(
            "variables-copy-policy",
            if dialog.overwrite {
                "A key the environment already has takes the value, description and storage \
                 of the one you copy."
            } else {
                "A key the environment already has is skipped and keeps its value."
            },
            cx,
        ));
    if secrets > 0 {
        body = body.child(caption(
            "variables-copy-secrets",
            "Secrets copy their keyring value into the environment's own keyring entry, \
             never into a file. A secret whose keyring value cannot be read is skipped.",
            cx,
        ));
    }
    let footer = div()
        .flex()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            Button::new("variables-copy-cancel", "Cancel")
                .ghost()
                .kbd("Esc")
                .on_click(
                    cx.listener(|this, _event, window, cx| this.close_copy_dialog(window, cx)),
                ),
        )
        .child(
            Button::new("variables-copy-confirm", "Copy")
                .primary()
                .icon(ui::icon(IconName::Copy))
                .disabled(dialog.target.is_none())
                .on_click(cx.listener(|this, _event, window, cx| this.confirm_copy(window, cx))),
        );
    let on_close = cx.entity().downgrade();
    let modal = Modal::new("Copy to environment")
        .id("variables-copy-backdrop")
        .icon(ui::icon(IconName::Copy))
        .width(tokens::EXPORT_DIALOG_WIDTH)
        .focus_handle(shell.settings.copy.focus.handle())
        .body(
            region("variables-copy-dialog")
                .role(Role::Dialog)
                .aria_label("Copy to environment")
                .child(body),
        )
        .footer(footer)
        .on_close(move |window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_copy_dialog(window, cx)) {
                log::warn!("variables copy dialog close: {error}");
            }
        });
    let viewport = window.viewport_size();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                deferred(
                    anchored().position(Point::default()).child(
                        div()
                            .relative()
                            .w(viewport.width)
                            .h(viewport.height)
                            .child(modal),
                    ),
                )
                .with_priority(3),
            )
            .into_any_element(),
    )
}

fn caption(id: &'static str, text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let text = text.into();
    ui::role(text_region(id), TextVariant::Caption, cx)
        .aria_label(text.clone())
        .child(text)
}

#[cfg(test)]
#[path = "variables_copy_tests.rs"]
mod tests;
