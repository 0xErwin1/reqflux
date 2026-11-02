//! The run document without GPUI: what the run is over, its setup, the steps the engine
//! reported, the grid rows and every text the document shows.

use std::time::Duration;

use reqflux_engine::{
    CurrentValues, RunEnd, RunEvent, RunItem, RunSummary, SecretScope, StepResult,
};

use crate::shell::documents::locate;
use crate::shell::request::elapsed_text;
use crate::shell::request::size_text;
use crate::shell::tree_model::{Node, NodeKind};

/// Whether the run is over a whole collection or one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetKind {
    Collection,
    Folder,
}

impl TargetKind {
    /// The hint beside the target in the setup.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::Collection => "whole collection",
            Self::Folder => "folder",
        }
    }
}

/// A request the run will send, as the grid and the order list show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanRow {
    pub(crate) id: String,
    pub(crate) name: String,
    /// The uppercase HTTP method.
    pub(crate) method: String,
    /// The folder the request sits in, else its collection.
    pub(crate) folder: String,
}

/// Where the run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The setup form, before the first run.
    Setup,
    Running,
    Finished,
}

/// The state of a grid row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowStatus {
    /// The step with this number finished.
    Done(usize),
    /// The request in flight.
    Sending,
    /// Not reached yet.
    Queued,
    /// The run ended before reaching it.
    Skipped,
}

/// One row of the results grid: a step or a request the run has not reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GridRow {
    pub(crate) plan: PlanRow,
    pub(crate) status: RowStatus,
}

/// The tone of a verdict or a status text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// The ordinary text color.
    Normal,
    Accent,
    Success,
    Warning,
    Danger,
    Muted,
}

/// The run document: its target, setup and what the engine reported.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunModel {
    /// The id of the collection or folder.
    pub(crate) target: String,
    pub(crate) name: String,
    pub(crate) kind: TargetKind,
    /// The requests in tree order, as the engine plans them.
    pub(crate) plan: Vec<PlanRow>,
    /// The environment slug the run uses; `None` is No environment.
    pub(crate) environment: Option<String>,
    /// Its name, for the header.
    pub(crate) environment_name: Option<String>,
    /// Stop on first failure.
    pub(crate) bail: bool,
    pub(crate) phase: Phase,
    /// One result per finished step, in order.
    pub(crate) results: Vec<StepResult>,
    /// The step in flight, with its request.
    pub(crate) sending: Option<(usize, String)>,
    /// The plan index of the last request the run started.
    pub(crate) reached: Option<usize>,
    /// How the run ended, once it did.
    pub(crate) end: Option<RunEnd>,
    /// Why the run did not start.
    pub(crate) error: Option<String>,
    /// How long the run took or has taken.
    pub(crate) elapsed: Duration,
    /// The selected step, whose result shows under the grid.
    pub(crate) selected: Option<usize>,
}

impl RunModel {
    /// The setup of a run over the collection or folder `target` among `roots`, with the
    /// engine's run order `items`. `None` when `target` is not a collection or folder.
    pub(crate) fn new(
        roots: &[Node],
        target: &str,
        items: &[RunItem],
        environment: Option<(String, String)>,
    ) -> Option<Self> {
        let node = crate::shell::tree_model::find(roots, target)?;
        let kind = match node.kind {
            NodeKind::Collection => TargetKind::Collection,
            NodeKind::Folder => TargetKind::Folder,
            NodeKind::Request { .. } => return None,
        };
        let plan = items
            .iter()
            .map(|item| {
                let location = locate(roots, &item.id);
                PlanRow {
                    id: item.id.clone(),
                    name: item.name.clone(),
                    method: location
                        .as_ref()
                        .map(|location| location.method.clone())
                        .unwrap_or_default(),
                    folder: location
                        .map(|location| {
                            location
                                .folders
                                .last()
                                .cloned()
                                .unwrap_or(location.collection)
                        })
                        .unwrap_or_default(),
                }
            })
            .collect();
        let (environment, environment_name) = environment.unzip();
        Some(Self {
            target: target.to_owned(),
            name: node.name.clone(),
            kind,
            plan,
            environment,
            environment_name,
            bail: false,
            phase: Phase::Setup,
            results: Vec::new(),
            sending: None,
            reached: None,
            end: None,
            error: None,
            elapsed: Duration::ZERO,
            selected: None,
        })
    }

    /// Clears what the last run reported and marks the run started.
    pub(crate) fn start(&mut self) {
        self.phase = Phase::Running;
        self.results.clear();
        self.sending = None;
        self.reached = None;
        self.end = None;
        self.error = None;
        self.elapsed = Duration::ZERO;
        self.selected = None;
    }

    /// Applies one event of the run.
    pub(crate) fn apply(&mut self, event: RunEvent) {
        match event {
            RunEvent::Started { step, item } => {
                self.reached = self.plan.iter().position(|row| row.id == item.id);
                self.sending = Some((step, item.id));
            }
            RunEvent::Finished(result) => {
                self.sending = None;
                self.results.push(result);
            }
        }
    }

    /// Ends the run with what the engine returned.
    pub(crate) fn finish(&mut self, outcome: Result<&RunSummary, String>) {
        self.phase = Phase::Finished;
        self.sending = None;
        match outcome {
            Ok(summary) => {
                self.results.clone_from(&summary.results);
                self.end = Some(summary.end.clone());
                self.elapsed = Duration::from_millis(summary.elapsed_ms);
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// The grid rows: every finished step, the request in flight, then the requests after
    /// the last one reached, queued while the run goes on and skipped once it ended.
    pub(crate) fn rows(&self) -> Vec<GridRow> {
        let mut rows: Vec<GridRow> = self
            .results
            .iter()
            .map(|result| GridRow {
                plan: self.plan_row(&result.item),
                status: RowStatus::Done(result.step),
            })
            .collect();
        if let Some((_, id)) = &self.sending
            && let Some(plan) = self.plan.iter().find(|row| &row.id == id)
        {
            rows.push(GridRow {
                plan: plan.clone(),
                status: RowStatus::Sending,
            });
        }
        let completed = matches!(self.end, Some(RunEnd::Completed));
        let rest = self
            .plan
            .iter()
            .skip(self.reached.map_or(0, |index| index + 1));
        let status = if self.phase == Phase::Finished {
            RowStatus::Skipped
        } else {
            RowStatus::Queued
        };
        if !completed {
            rows.extend(rest.map(|plan| GridRow {
                plan: plan.clone(),
                status,
            }));
        }
        rows
    }

    /// The plan row of `item`, or one built from it when the plan does not hold it.
    fn plan_row(&self, item: &RunItem) -> PlanRow {
        self.plan
            .iter()
            .find(|row| row.id == item.id)
            .cloned()
            .unwrap_or_else(|| PlanRow {
                id: item.id.clone(),
                name: item.name.clone(),
                method: String::new(),
                folder: String::new(),
            })
    }

    /// The result of step `step`.
    pub(crate) fn result(&self, step: usize) -> Option<&StepResult> {
        self.results.iter().find(|result| result.step == step)
    }

    /// How many steps ran the request `id`.
    pub(crate) fn sends_of(&self, id: &str) -> usize {
        self.results
            .iter()
            .filter(|result| result.item.id == id)
            .count()
    }

    /// How many tests passed and failed so far.
    pub(crate) fn test_counts(&self) -> (usize, usize) {
        let tests = self.results.iter().flat_map(|result| result.tests.iter());
        tests.fold((0, 0), |(passed, failed), test| {
            if test.passed {
                (passed + 1, failed)
            } else {
                (passed, failed + 1)
            }
        })
    }

    /// Whether every step got a response and passed its tests, and the run completed or
    /// a script stopped it.
    pub(crate) fn passed(&self) -> bool {
        self.error.is_none()
            && matches!(self.end, Some(RunEnd::Completed | RunEnd::Stopped))
            && self.results.iter().all(|result| !result.failed())
    }

    /// The header title.
    pub(crate) fn title(&self) -> String {
        let name = &self.name;
        match self.phase {
            Phase::Setup => format!("Run {name}"),
            Phase::Running => format!("Running {name}"),
            Phase::Finished if self.error.is_some() => format!("{name} did not run"),
            Phase::Finished if matches!(self.end, Some(RunEnd::Cancelled)) => {
                format!("{name} stopped")
            }
            Phase::Finished if self.passed() => format!("{name} passed"),
            Phase::Finished => format!("{name} failed"),
        }
    }

    /// The badge beside the title and its tone; none in the setup.
    pub(crate) fn badge(&self) -> Option<(&'static str, Tone)> {
        match self.phase {
            Phase::Setup => None,
            Phase::Running => Some(("RUNNING", Tone::Accent)),
            Phase::Finished => Some(match &self.end {
                _ if self.error.is_some() => ("FAILED", Tone::Danger),
                Some(RunEnd::Cancelled) => ("CANCELLED", Tone::Warning),
                Some(RunEnd::Bailed) => ("STOPPED", Tone::Danger),
                _ if self.passed() => ("PASSED", Tone::Success),
                _ => ("FAILED", Tone::Danger),
            }),
        }
    }

    /// The line under the title: the environment, the failure policy, and how the run
    /// ended when it did not run to the end.
    pub(crate) fn subline(&self) -> Vec<String> {
        let mut parts = vec![
            self.environment_name
                .clone()
                .unwrap_or_else(|| "No environment".to_owned()),
        ];
        let skipped = self
            .rows()
            .iter()
            .filter(|row| row.status == RowStatus::Skipped)
            .count();
        let at = self.results.len();
        match (&self.error, &self.end) {
            (Some(error), _) => parts.push(error.clone()),
            (None, Some(RunEnd::Bailed)) => {
                parts.push(format!("stopped on first failure at request {at}"));
            }
            (None, Some(RunEnd::Cancelled)) => parts.push(format!("stopped at request {at}")),
            (None, Some(RunEnd::Stopped)) => {
                parts.push("a script ended the run".to_owned());
            }
            (None, Some(RunEnd::StepLimit(limit))) => {
                parts.push(format!("ended after {limit} sends, the step limit"));
            }
            (None, Some(RunEnd::UnknownNext(name))) => {
                parts.push(format!(
                    "pm.setNextRequest named {name}, which the run does not hold"
                ));
            }
            (None, Some(RunEnd::Completed) | None) => parts.push(
                if self.bail {
                    "stops on first failure"
                } else {
                    "continues after a failure"
                }
                .to_owned(),
            ),
        }
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }
        parts
    }

    /// The totals at the right of the header: requests, passed and failed tests, time.
    pub(crate) fn totals(&self) -> [(String, Tone); 4] {
        let total = self.plan.len();
        let done = self.results.len();
        let (passed, failed) = self.test_counts();
        let requests = if self.phase == Phase::Finished {
            format!("{done} of {total} requests")
        } else {
            format!("{done} of {total}")
        };
        let failed_tone = if failed > 0 {
            Tone::Danger
        } else {
            Tone::Muted
        };
        [
            (requests, Tone::Muted),
            (format!("{passed} tests passed"), Tone::Success),
            (format!("{failed} failed"), failed_tone),
            (format!("{:.1} s", self.elapsed.as_secs_f64()), Tone::Muted),
        ]
    }

    /// The label of the run's tab chip.
    pub(crate) fn tab_label(&self) -> String {
        format!("{} run", self.name)
    }
}

/// The cells of `row` after its number: method, request, folder, status, time, size and
/// tests, each with its tone.
pub(crate) fn cells(row: &GridRow, model: &RunModel) -> [(String, Tone); 7] {
    const NONE: &str = "\u{2014}";
    let plan = &row.plan;
    let head = |muted: bool| {
        let tone = if muted { Tone::Muted } else { Tone::Normal };
        [
            (plan.method.clone(), tone),
            (plan.name.clone(), tone),
            (plan.folder.clone(), Tone::Muted),
        ]
    };
    let pending = |status: &str| {
        let [method, name, folder] = head(true);
        [
            method,
            name,
            folder,
            (status.to_owned(), Tone::Muted),
            (NONE.to_owned(), Tone::Muted),
            (NONE.to_owned(), Tone::Muted),
            (NONE.to_owned(), Tone::Muted),
        ]
    };
    match row.status {
        RowStatus::Queued => pending("Queued"),
        RowStatus::Skipped => pending("Skipped"),
        RowStatus::Sending => {
            let [method, name, folder] = head(false);
            [
                method,
                name,
                folder,
                ("Sending\u{2026}".to_owned(), Tone::Accent),
                (NONE.to_owned(), Tone::Muted),
                (NONE.to_owned(), Tone::Muted),
                (NONE.to_owned(), Tone::Muted),
            ]
        }
        RowStatus::Done(step) => {
            let [method, name, folder] = head(false);
            let Some(result) = model.result(step) else {
                return pending("");
            };
            let status = match (result.status, &result.error) {
                (Some(code), _) => (
                    status_text(code, result.reason.as_deref()),
                    status_tone(code),
                ),
                (None, Some(_)) => ("Failed".to_owned(), Tone::Danger),
                (None, None) => (NONE.to_owned(), Tone::Muted),
            };
            let size = if result.status.is_some() {
                size_text(result.size)
            } else {
                NONE.to_owned()
            };
            [
                method,
                name,
                folder,
                status,
                (
                    elapsed_text(Duration::from_millis(result.elapsed_ms)),
                    Tone::Normal,
                ),
                (size, Tone::Normal),
                tests_text(result),
            ]
        }
    }
}

/// `200 OK`, or the code alone without a reason.
fn status_text(code: u16, reason: Option<&str>) -> String {
    match reason {
        Some(reason) if !reason.is_empty() => format!("{code} {reason}"),
        Some(_) | None => code.to_string(),
    }
}

/// 2xx success, 4xx warning, 5xx danger, the rest muted.
fn status_tone(code: u16) -> Tone {
    match code {
        200..=299 => Tone::Success,
        400..=499 => Tone::Warning,
        500..=599 => Tone::Danger,
        _ => Tone::Muted,
    }
}

/// `3 passed`, `2 failed · 1 passed`, or a dash without tests.
pub(crate) fn tests_text(result: &StepResult) -> (String, Tone) {
    let passed = result.tests.iter().filter(|test| test.passed).count();
    let failed = result.tests.len() - passed;
    match (passed, failed) {
        (0, 0) => ("\u{2014}".to_owned(), Tone::Muted),
        (passed, 0) => (format!("{passed} passed"), Tone::Success),
        (0, failed) => (format!("{failed} failed"), Tone::Danger),
        (passed, failed) => (
            format!("{failed} failed \u{b7} {passed} passed"),
            Tone::Danger,
        ),
    }
}

/// A current value the run changed: set to a value, or cleared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) scope: SecretScope,
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

/// What the run changed between the current values it `started` with and those it
/// `ended` with, in scope and key order: what to persist after it, as a single send
/// persists its script writes.
pub(crate) fn changes(started: &CurrentValues, ended: &CurrentValues) -> Vec<Change> {
    let mut changes: Vec<Change> = ended
        .iter()
        .filter(|(scope, key, value)| started.get(*scope, key) != Some(*value))
        .map(|(scope, key, value)| Change {
            scope,
            key: key.to_owned(),
            value: Some(value.to_owned()),
        })
        .collect();
    changes.extend(
        started
            .iter()
            .filter(|(scope, key, _)| ended.get(*scope, key).is_none())
            .map(|(scope, key, _)| Change {
                scope,
                key: key.to_owned(),
                value: None,
            }),
    );
    changes
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
