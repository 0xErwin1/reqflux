//! The `Toggle theme` command (light-theme S4): cycles Dark, Light and System and saves the
//! choice in the user's config file (user-config-mcp-policy S2). The interface size commands
//! (bolt-gaps S2) step or reset `font_size` the same way.

use std::path::PathBuf;

use gpui::{Context, Window};
use reqflux_engine::config;

use super::Shell;
use crate::theme::{ConfigReport, save_mode};

impl Shell {
    /// Sets the config file `Toggle theme` writes to and what went wrong reading it at
    /// startup, for the Problems panel. Without a path the choice is not saved; tests pass
    /// their own file so no test touches the user's config.
    pub(crate) fn set_config(&mut self, path: Option<PathBuf>, report: ConfigReport) {
        // The config file decides the layout mode the window starts in (layouts S7).
        self.apply_config_layout(path.as_deref());
        self.config_path = path;
        self.config_report = report;
        // The config file decides `[network]`, so an engine opened before it read the file
        // (the one at startup) is connected once it is known.
        self.connect_workspace();
    }

    /// Switches to the mode after the installed one and saves it; a save that fails shows in
    /// an error toast.
    pub(super) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = crate::theme::next(crate::theme::mode(cx));
        crate::theme::set_mode(mode, window.appearance(), cx);
        if let Err(error) = save_mode(self.config_path.as_deref(), mode) {
            super::toasts::failure("Cannot save the theme", error.to_string(), cx);
        }
        cx.notify();
    }

    /// Zoom: steps the interface size by `step` pixels within the config file's bounds, or
    /// resets it to the default without one, and saves it as `font_size`. A change that
    /// leaves the size where it was (a step at a limit, a reset at the default) saves
    /// nothing; a save that fails shows in an error toast and the size still applies.
    pub(super) fn change_interface_size(&mut self, step: Option<f32>, cx: &mut Context<Self>) {
        let current = bolt::fonts::current(cx).ui_size;
        let size = step.map_or(config::DEFAULT_FONT_SIZE, |step| {
            config::clamp_font_size(current + step)
        });
        if size == current {
            return;
        }
        crate::theme::set_font_size(size, cx);
        if let Some(path) = self.config_path.clone() {
            if let Err(error) = config::set_font_size(&path, size) {
                super::toasts::failure("Cannot save the interface size", error.to_string(), cx);
            }
            // The App > General select shows the saved size.
            if self.settings.app.controls.is_some() {
                self.read_app_config(cx);
            }
        }
        cx.notify();
    }
}
