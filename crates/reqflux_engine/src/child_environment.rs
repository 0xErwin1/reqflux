//! The environment a child process may inherit from ReqFlux.
//!
//! On Linux the GUI calls `gpui_linux::select_vulkan_driver` first, which re-executes the
//! process with `VK_LOADER_DRIVERS_SELECT` narrowed to the boot GPU's Vulkan driver and a marker
//! recording that choice. A child inherits both, so a program ReqFlux starts for the user (the
//! browser, a file's default handler) would be pinned to that driver too.
//!
//! The rule and the variable names mirror `gpui-pre-linux`'s `restore_child_environment`, which
//! owns them. It is repeated here because the engine must not depend on GPUI; the UI and the
//! engine both apply this copy.

use std::process::Command;

/// Records which Vulkan driver GPUI's startup selected, as the ICD manifest patterns it chose.
const SELECTION_VARIABLE: &str = "GPUI_VULKAN_DRIVER_SELECT";

/// The loader variable that selection narrows.
const LOADER_SELECTION_VARIABLE: &str = "VK_LOADER_DRIVERS_SELECT";

/// Marker value meaning no driver is selected, because every driver was tried instead.
const FALLBACK_VALUE: &str = "fallback";

/// Marker value that turns the selection off.
const OPT_OUT_VALUE: &str = "0";

/// Removes the Vulkan driver selection GPUI's startup may have set, so a child process starts
/// with the environment the user gave ReqFlux. Driver variables the user set are kept.
///
/// Call this on every `Command` that runs a program for the user.
pub fn restore_child_environment(command: &mut Command) {
    restore_child_environment_from(selection_marker().as_deref(), command);
}

/// Whether a child of this process would inherit a driver selection GPUI's startup made, so a
/// launcher that builds its own commands (the `open` crate) has to be bypassed.
pub(crate) fn inherits_driver_selection() -> bool {
    let mut command = Command::new("reqflux-child");
    restore_child_environment(&mut command);
    command.get_envs().next().is_some()
}

/// The selection marker as this process carries it.
fn selection_marker() -> Option<String> {
    std::env::var_os(SELECTION_VARIABLE).map(|value| value.to_string_lossy().into_owned())
}

/// Applies the rule for a given marker, so the removals can be asserted without changing this
/// process's environment.
fn restore_child_environment_from(marker: Option<&str>, command: &mut Command) {
    match marker.filter(|marker| !marker.is_empty()) {
        None | Some(OPT_OUT_VALUE) => {}
        Some(FALLBACK_VALUE) => {
            command.env_remove(SELECTION_VARIABLE);
        }
        Some(_) => {
            command.env_remove(LOADER_SELECTION_VARIABLE);
            command.env_remove(SELECTION_VARIABLE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every environment update `command` records, sorted, as `(name, value)` pairs.
    fn updates_for(marker: Option<&str>) -> Vec<(String, Option<String>)> {
        let mut command = Command::new("reqflux-child");
        restore_child_environment_from(marker, &mut command);
        let mut updates: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        updates.sort();
        updates
    }

    #[test]
    fn a_child_of_a_process_that_selected_a_driver_drops_the_selection_and_the_marker() {
        assert_eq!(
            updates_for(Some("*intel_icd*,*intel_hasvk_icd*")),
            vec![
                (SELECTION_VARIABLE.to_owned(), None),
                (LOADER_SELECTION_VARIABLE.to_owned(), None),
            ]
        );
    }

    #[test]
    fn a_child_of_a_fallback_process_drops_only_the_marker() {
        assert_eq!(
            updates_for(Some(FALLBACK_VALUE)),
            vec![(SELECTION_VARIABLE.to_owned(), None)]
        );
    }

    #[test]
    fn a_process_without_a_selection_leaves_the_child_environment_alone() {
        for marker in [None, Some(OPT_OUT_VALUE), Some("")] {
            assert_eq!(updates_for(marker), Vec::new(), "marker {marker:?}");
        }
    }

    #[test]
    fn the_rule_names_the_variables_gpui_pre_linux_uses() {
        assert_eq!(SELECTION_VARIABLE, "GPUI_VULKAN_DRIVER_SELECT");
        assert_eq!(LOADER_SELECTION_VARIABLE, "VK_LOADER_DRIVERS_SELECT");
        assert_eq!(FALLBACK_VALUE, "fallback");
        assert_eq!(OPT_OUT_VALUE, "0");
    }
}
