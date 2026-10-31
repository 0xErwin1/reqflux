//! The `Toggle theme` command (light-theme S4): cycles Dark, Light and System and saves the
//! choice in the user's config file (user-config-mcp-policy S2).

use std::path::PathBuf;

use gpui::{Context, Window};

use super::Shell;
use crate::theme::save_mode;

impl Shell {
    /// Sets the config file `Toggle theme` writes to and the warning to show when it did not
    /// parse at startup. Without a path the choice is not saved; tests pass their own file so
    /// no test touches the user's config.
    pub(crate) fn set_config(&mut self, path: Option<PathBuf>, warning: Option<String>) {
        self.config_path = path;
        self.config_warning = warning;
    }

    /// Switches to the mode after the installed one and saves it.
    pub(super) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = bolt::mode(cx).next();
        bolt::set_mode(mode, window.appearance(), cx);
        save_mode(self.config_path.as_deref(), mode);
        cx.notify();
    }
}
