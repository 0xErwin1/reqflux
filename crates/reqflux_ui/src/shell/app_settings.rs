//! The App section of the settings page (app-pending S3, design/app-settings/02): what the
//! user's `config.toml` holds and the operations its controls run. Every change writes the
//! file at once through `reqflux_engine::config`'s format-preserving editors, then reads it
//! back; the theme and the interface font size also apply live. A file that cannot be read
//! or does not parse leaves the section read-only, so nothing overwrites it, until Reload
//! reads it again. Workspace overrides are only shown; "Open config.toml" opens the file
//! with the OS default handler.

use std::path::{Path, PathBuf};

use bolt::controls::{Dropdown, DropdownItem, DropdownSelectionChanged, InputState};
use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use reqflux_engine::config::{self, ConfigError, McpList, Rule, ThemeMode, UserConfig};
use reqflux_engine::policy::ToolClass;

use super::{AppSection, Section};
use crate::shell::Shell;
use crate::theme::{ConfigReport, palette_mode};

/// The sizes the font size select offers, in pixels: every whole size from
/// `MIN_FONT_SIZE` to `MAX_FONT_SIZE`, and `current` in its place when the file sets a size
/// between them.
pub(crate) fn font_sizes(current: f32) -> Vec<f32> {
    let mut sizes: Vec<f32> = (8_u8..=32).map(f32::from).collect();
    if !sizes.contains(&current) {
        let at = sizes.partition_point(|size| *size < current);
        sizes.insert(at, current);
    }
    sizes
}

/// The label of the font size `size`: `13 px`, `14.5 px`.
pub(crate) fn font_size_label(size: f32) -> String {
    format!("{size} px")
}

/// The App section's state.
#[derive(Default)]
pub(crate) struct AppSettings {
    /// What the last read of `config.toml` found; `None` before the section first opens.
    pub(crate) loaded: Option<Result<UserConfig, ConfigError>>,
    /// The section's stateful controls, made the first time it opens.
    pub(crate) controls: Option<Controls>,
}

/// The font size select and the host and environment fields.
pub(crate) struct Controls {
    pub(crate) font_size: Entity<Dropdown>,
    /// The size of each item of the select, in order.
    pub(crate) sizes: Vec<f32>,
    pub(crate) hosts: Entity<InputState>,
    pub(crate) environments: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl AppSettings {
    /// The parsed file, `None` while it cannot be read or does not parse.
    pub(crate) fn config(&self) -> Option<&UserConfig> {
        self.loaded.as_ref().and_then(|loaded| loaded.as_ref().ok())
    }

    /// Why the file is not used.
    pub(crate) fn error(&self) -> Option<&ConfigError> {
        self.loaded
            .as_ref()
            .and_then(|loaded| loaded.as_ref().err())
    }

    /// Whether the controls write: only over a file that parsed.
    pub(crate) fn editable(&self) -> bool {
        self.config().is_some()
    }

    #[cfg(test)]
    pub(crate) fn font_size_control(&self) -> Option<&Entity<Dropdown>> {
        self.controls.as_ref().map(|controls| &controls.font_size)
    }
}

impl Shell {
    /// The rail's Settings: opens the page at App > General and reads `config.toml`.
    pub(crate) fn open_app_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_app_controls(window, cx);
        self.read_app_config(cx);
        self.open_settings(Section::App(AppSection::General), cx);
    }

    /// Makes the section's controls the first time it shows.
    pub(crate) fn ensure_app_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.app.controls.is_none() {
            self.settings.app.controls = Some(controls(window, cx));
        }
    }

    /// Reload: reads `config.toml` again and applies its theme and font size, and refreshes
    /// what the Problems panel says about it.
    pub(crate) fn reload_app_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.read_app_config(cx);
        if let Some(loaded) = self.settings.app.config() {
            let mode = palette_mode(loaded.theme.unwrap_or_default());
            let size = loaded.interface_font_size();
            crate::theme::set_mode(mode, window.appearance(), cx);
            crate::theme::set_font_size(size, cx);
        }
        self.config_report = match self.config_path.as_deref() {
            Some(path) => match config::load_with_warnings(path) {
                Ok((_, warnings)) => ConfigReport {
                    error: None,
                    warnings,
                },
                Err(error) => ConfigReport {
                    error: Some(error.to_string()),
                    warnings: Vec::new(),
                },
            },
            None => ConfigReport::default(),
        };
        cx.notify();
    }

    /// Reads `config.toml` into the section and shows its font size in the select.
    fn read_app_config(&mut self, cx: &mut Context<Self>) {
        let loaded = match self.config_path.as_deref() {
            Some(path) => config::load(path),
            None => Err(ConfigError::NoConfigDirectory),
        };
        if let Err(error) = &loaded {
            log::warn!("{error}; the app settings are read-only");
        }
        let editable = loaded.is_ok();
        let size = loaded
            .as_ref()
            .map_or(config::DEFAULT_FONT_SIZE, UserConfig::interface_font_size);
        self.settings.app.loaded = Some(loaded);
        if let Some(controls) = &mut self.settings.app.controls {
            let sizes = font_sizes(size);
            let index = sizes.iter().position(|candidate| *candidate == size);
            let items = (sizes != controls.sizes).then(|| size_items(&sizes));
            controls.sizes = sizes;
            controls.font_size.update(cx, |dropdown, cx| {
                if let Some(items) = items {
                    dropdown.set_items(items, cx);
                }
                dropdown.set_selected_index(index, cx);
                dropdown.set_disabled(!editable, cx);
            });
        }
        cx.notify();
    }

    /// Runs `edit` on `config.toml` when the section is editable, then reads the file back;
    /// a failure shows in the page. Returns whether the edit ran and succeeded.
    fn edit_app_config<T>(
        &mut self,
        edit: impl FnOnce(&Path) -> Result<T, ConfigError>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = self
            .config_path
            .clone()
            .filter(|_| self.settings.app.editable())
        else {
            return false;
        };
        let outcome = edit(&path);
        self.read_app_config(cx);
        match outcome {
            Ok(_) => {
                self.show_variables_warning(None, cx);
                true
            }
            Err(error) => {
                self.show_variables_error(error.to_string(), cx);
                false
            }
        }
    }

    /// The Theme control: writes `theme` and switches to it.
    pub(crate) fn select_app_theme(
        &mut self,
        mode: ThemeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit_app_config(|path| config::set_theme(path, mode), cx) {
            crate::theme::set_mode(palette_mode(mode), window.appearance(), cx);
        }
    }

    /// The font size select: writes `font_size` and resizes the interface.
    pub(crate) fn select_app_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        if self.edit_app_config(|path| config::set_font_size(path, size), cx) {
            crate::theme::set_font_size(size, cx);
        }
    }

    /// A rule control: writes the `[mcp]` rule of `class`.
    pub(crate) fn select_app_rule(&mut self, class: ToolClass, rule: Rule, cx: &mut Context<Self>) {
        self.edit_app_config(|path| config::set_rule(path, class, rule), cx);
    }

    /// Add host / Add environment: appends the field's text to the list and empties the
    /// field.
    pub(crate) fn add_app_list_entry(
        &mut self,
        list: McpList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self
            .settings
            .app
            .controls
            .as_ref()
            .map(|controls| match list {
                McpList::Hosts => controls.hosts.clone(),
                McpList::Environments => controls.environments.clone(),
            })
        else {
            return;
        };
        let entry = field.read(cx).value().to_string();
        if entry.trim().is_empty() {
            return;
        }
        if self.edit_app_config(|path| config::add_list_entry(path, list, &entry), cx) {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    /// A row's Remove: drops `entry` from the list.
    pub(crate) fn remove_app_list_entry(
        &mut self,
        list: McpList,
        entry: &str,
        cx: &mut Context<Self>,
    ) {
        self.edit_app_config(|path| config::remove_list_entry(path, list, entry), cx);
    }

    /// "Open config.toml" and "Open file": opens the file with the OS default handler.
    pub(crate) fn open_config_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.config_path.clone() else {
            self.show_variables_error(ConfigError::NoConfigDirectory.to_string(), cx);
            return;
        };
        if let Err(error) = crate::open_file::open(&path, cx) {
            let message = format!("cannot open {}: {error}", path.display());
            self.show_variables_error(message, cx);
        }
    }

    /// The config file path, as the page shows it.
    pub(crate) fn config_file(&self) -> Option<&PathBuf> {
        self.config_path.as_ref()
    }
}

/// The font size select and the two list fields, subscribed to the shell.
fn controls(window: &mut Window, cx: &mut Context<Shell>) -> Controls {
    let sizes = font_sizes(config::DEFAULT_FONT_SIZE);
    let items = size_items(&sizes);
    let font_size = cx.new(|_| Dropdown::new("font-size").items(items));
    let hosts = cx.new(|cx| InputState::new(window, cx).placeholder("api.example.com"));
    let environments = cx.new(|cx| InputState::new(window, cx).placeholder("environment slug"));
    let subscriptions = vec![cx.subscribe_in(
        &font_size,
        window,
        |this, _, event: &DropdownSelectionChanged, _window, cx| {
            let size = this
                .settings
                .app
                .controls
                .as_ref()
                .and_then(|controls| controls.sizes.get(event.index).copied());
            if let Some(size) = size {
                this.select_app_font_size(size, cx);
            }
        },
    )];
    Controls {
        font_size,
        sizes,
        hosts,
        environments,
        _subscriptions: subscriptions,
    }
}

fn size_items(sizes: &[f32]) -> Vec<DropdownItem> {
    sizes
        .iter()
        .map(|size| DropdownItem::new(font_size_label(*size)))
        .collect()
}

#[cfg(test)]
#[path = "app_settings_tests.rs"]
mod tests;
