//! What the Problems chip and panel show: the config error, when the config file did not
//! load, then every warning, the config file's before the workspace load's.

use std::path::Path;

use gpui::{Context, Window};

use super::Shell;
use crate::workspace::OpenedWorkspace;

/// The title of the config error in the Problems panel.
pub(super) const CONFIG_ERROR_TITLE: &str = "Config not loaded · defaults in use";

/// One warning: the file it concerns, when its text names it, and what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Problem {
    pub(super) location: Option<String>,
    pub(super) message: String,
}

impl Problem {
    /// `text` split at its `<path>: ` prefix, when it starts with `path`; the location is
    /// `path` relative to `root` when it lies inside it.
    pub(super) fn at(text: &str, path: &Path, root: Option<&Path>) -> Self {
        let prefix = format!("{}: ", path.display());
        match text.strip_prefix(&prefix) {
            Some(message) => {
                let shown = root
                    .and_then(|root| path.strip_prefix(root).ok())
                    .unwrap_or(path);
                Self {
                    location: Some(shown.display().to_string()),
                    message: message.to_owned(),
                }
            }
            None => Self {
                location: None,
                message: text.to_owned(),
            },
        }
    }
}

/// The problems of the window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Problems {
    /// Why the config file was not used; never quotes the file.
    pub(super) error: Option<String>,
    pub(super) warnings: Vec<Problem>,
}

impl Problems {
    pub(super) fn is_empty(&self) -> bool {
        self.error.is_none() && self.warnings.is_empty()
    }

    pub(super) fn has_error(&self) -> bool {
        self.error.is_some()
    }

    fn error_count(&self) -> usize {
        usize::from(self.has_error())
    }

    /// `1 error · 3 warnings`, leaving out a part that is zero.
    pub(super) fn summary(&self) -> String {
        let errors = self.error_count();
        let warnings = self.warnings.len();
        [(errors, "error"), (warnings, "warning")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, noun)| count(n, noun))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The chip's accessible label.
    pub(super) fn chip_label(&self) -> String {
        format!(
            "Problems: {}, {}. Show the problems panel",
            count(self.error_count(), "error"),
            count(self.warnings.len(), "warning")
        )
    }
}

/// `n` and `noun`, plural unless `n` is one.
pub(super) fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

impl Shell {
    /// The config report and the workspace load warnings as problems.
    pub(super) fn problems(&self) -> Problems {
        let report = &self.config_report;
        let mut warnings =
            Vec::with_capacity(report.warnings.len() + self.workspace.warning_count());
        warnings.extend(report.warnings.iter().map(|text| match &self.config_path {
            Some(path) => Problem::at(text, path, None),
            None => Problem {
                location: None,
                message: text.clone(),
            },
        }));
        if let OpenedWorkspace::Loaded(engine) = &self.workspace {
            let root = Some(engine.root());
            warnings.extend(
                engine
                    .warnings()
                    .iter()
                    .map(|warning| Problem::at(&warning.to_string(), &warning.path, root)),
            );
        }
        Problems {
            error: report.error.clone(),
            warnings,
        }
    }

    /// Opens the Problems panel and focuses it, or closes it when open.
    pub(super) fn toggle_problems(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.problems_open = !self.problems_open;
        if self.problems_open {
            self.problems_focus.focus(window, cx);
        } else {
            self.restore_focus(window, cx);
        }
        cx.notify();
    }

    /// Closes the Problems panel and gives the focus back.
    pub(super) fn close_problems(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.problems_open = false;
        self.restore_focus(window, cx);
    }
}
