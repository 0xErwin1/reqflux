//! The audit view's state (app-pending S2): where the MCP audit log is, what was read from it
//! and the toolbar's filters. The view ([`super::audit_view`]) is read-only: it never writes
//! the log. The log is read when the view opens and on Refresh, through
//! `reqflux_engine::audit::read_log`, which skips a line that does not parse and reports its
//! number; the view says so and never shows the line.

use std::path::PathBuf;

use bolt::controls::{DropdownItem, InputEvent, InputState, MultiSelect, MultiSelectChanged};
use gpui::{AppContext as _, Context, Entity, SharedString, Subscription, Window};
use gpui_kit_assets::IconName;
use reqflux_engine::audit::{AuditLog, read_log};

use super::Shell;
use super::audit_model::{CLASSES, Filters, OUTCOMES, Range};
use super::request::ui;
use super::sidebar_history::Panel;

/// The placeholder of the search field.
pub(crate) const SEARCH_PLACEHOLDER: &str = "Search tool, target, URL, reason";
/// Why a log without a location cannot be read.
const NO_LOCATION: &str = "neither XDG_STATE_HOME nor HOME is set";

/// The audit view's state.
#[derive(Default)]
pub(crate) struct AuditView {
    /// The audit log; `None` when neither `XDG_STATE_HOME` nor `HOME` is set, and in tests
    /// until they set it, so no test reads the user's log.
    pub(crate) path: Option<PathBuf>,
    /// What the last read found, or why the file could not be read.
    pub(crate) read: Option<Result<AuditLog, String>>,
    /// The toolbar's filters.
    pub(crate) filters: Filters,
    /// The index in the log of the expanded entry.
    pub(crate) expanded: Option<usize>,
    /// The toolbar's controls, made the first time the view opens.
    pub(crate) controls: Option<Controls>,
}

/// The toolbar's stateful controls.
pub(crate) struct Controls {
    pub(crate) search: Entity<InputState>,
    pub(crate) class: Entity<MultiSelect>,
    pub(crate) outcome: Entity<MultiSelect>,
    _subscriptions: Vec<Subscription>,
}

impl AuditView {
    /// The view over the user's audit log.
    pub(crate) fn new() -> Self {
        Self {
            path: default_path(),
            ..Self::default()
        }
    }

    /// The entries of the last read; empty when it failed or none happened.
    pub(crate) fn log(&self) -> Option<&AuditLog> {
        self.read.as_ref().and_then(|read| read.as_ref().ok())
    }
}

#[cfg(not(test))]
fn default_path() -> Option<PathBuf> {
    reqflux_engine::audit::default_path().ok()
}

#[cfg(test)]
fn default_path() -> Option<PathBuf> {
    None
}

impl Shell {
    /// Whether the audit view holds the document island.
    pub(super) fn audit_open(&self, cx: &gpui::App) -> bool {
        self.sidebar.read(cx).panel == Panel::Audit
    }

    /// Opens the audit view and reads the log.
    pub(super) fn open_audit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audit.controls.is_none() {
            self.audit.controls = Some(controls(window, cx));
        }
        self.read_audit();
        self.show_panel(Panel::Audit, cx);
        self.restore_focus(window, cx);
    }

    /// Reads the log again, as the view's Refresh does.
    pub(super) fn refresh_audit(&mut self, cx: &mut Context<Self>) {
        self.read_audit();
        cx.notify();
    }

    /// Leaves the audit view for the collection tree, when it is open: a request opened or
    /// activated takes the document island back.
    pub(super) fn leave_audit(&mut self, cx: &mut Context<Self>) {
        if self.audit_open(cx) {
            self.show_panel(Panel::Collections, cx);
        }
    }

    fn read_audit(&mut self) {
        self.audit.expanded = None;
        self.audit.read = Some(match &self.audit.path {
            Some(path) => read_log(path).map_err(|error| {
                log::warn!("cannot read the audit log {}: {error}", path.display());
                error.to_string()
            }),
            None => Err(NO_LOCATION.to_owned()),
        });
    }

    /// Keeps the entries of the classes whose values the Class multi-select holds; none keeps
    /// every class.
    pub(super) fn select_audit_classes(&mut self, values: &[SharedString], cx: &mut Context<Self>) {
        self.audit.filters.class = CLASSES
            .iter()
            .filter(|(_, class)| values.iter().any(|value| value == class))
            .map(|&(_, class)| class)
            .collect();
        cx.notify();
    }

    /// Keeps the entries of the decisions whose values the Outcome multi-select holds; none
    /// keeps every decision.
    pub(super) fn select_audit_outcomes(
        &mut self,
        values: &[SharedString],
        cx: &mut Context<Self>,
    ) {
        self.audit.filters.outcome = OUTCOMES
            .iter()
            .filter(|(_, value, _)| values.iter().any(|selected| selected == value))
            .map(|&(_, _, decision)| decision)
            .collect();
        cx.notify();
    }

    /// Shows the entries of `range`.
    pub(super) fn select_audit_range(&mut self, range: Range, cx: &mut Context<Self>) {
        self.audit.filters.range = range;
        cx.notify();
    }

    /// Expands the entry `index` of the log, or folds it when it is the expanded one.
    pub(super) fn toggle_audit_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.audit.expanded = (self.audit.expanded != Some(index)).then_some(index);
        cx.notify();
    }

    /// Puts every filter back to its default and empties the search field.
    pub(super) fn clear_audit_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.audit.filters = Filters::default();
        if let Some(controls) = &self.audit.controls {
            controls
                .search
                .update(cx, |search, cx| search.set_value("", window, cx));
            for select in [&controls.class, &controls.outcome] {
                select.update(cx, |select, cx| select.set_selected_values(&[], cx));
            }
        }
        cx.notify();
    }
}

/// The search field and the Class and Outcome multi-selects, subscribed to the shell.
fn controls(window: &mut Window, cx: &mut Context<Shell>) -> Controls {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
    let class = cx.new(|cx| {
        let mut select = MultiSelect::new("audit-class")
            .summary("Class")
            .leading_icon(ui::icon(IconName::SlidersHorizontal));
        select.set_items(items(CLASSES.iter().copied()), cx);
        select
    });
    let outcome = cx.new(|cx| {
        let mut select = MultiSelect::new("audit-outcome").summary("Outcome");
        select.set_items(
            items(OUTCOMES.iter().map(|&(label, value, _)| (label, value))),
            cx,
        );
        select
    });
    let subscriptions = vec![
        cx.subscribe(&search, |this, search, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.audit.filters.query = search.read(cx).value().to_string();
                cx.notify();
            }
        }),
        cx.subscribe(&class, |this, _, event: &MultiSelectChanged, cx| {
            this.select_audit_classes(&event.selected_values, cx);
        }),
        cx.subscribe(&outcome, |this, _, event: &MultiSelectChanged, cx| {
            this.select_audit_outcomes(&event.selected_values, cx);
        }),
    ];
    Controls {
        search,
        class,
        outcome,
        _subscriptions: subscriptions,
    }
}

fn items<'a>(entries: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<DropdownItem> {
    entries
        .map(|(label, value)| DropdownItem::with_value(label, value))
        .collect()
}
