//! The `Toggle theme` command (light-theme S4): cycles Dark, Light and System and saves the
//! choice in the user's config file (user-config-mcp-policy S2).

use std::path::PathBuf;

use gpui::{Context, Window};

use super::Shell;
use crate::theme::{ConfigReport, save_mode};

impl Shell {
    /// Sets the config file `Toggle theme` writes to and what went wrong reading it at
    /// startup, for the Problems panel. Without a path the choice is not saved; tests pass
    /// their own file so no test touches the user's config.
    pub(crate) fn set_config(&mut self, path: Option<PathBuf>, report: ConfigReport) {
        self.config_path = path;
        self.config_report = report;
    }

    /// Switches to the mode after the installed one and saves it.
    pub(super) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = crate::theme::next(crate::theme::mode(cx));
        crate::theme::set_mode(mode, window.appearance(), cx);
        save_mode(self.config_path.as_deref(), mode);
        cx.notify();
    }
}
