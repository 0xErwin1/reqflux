use std::path::Path;

use super::command;

#[test]
fn the_command_passes_the_path_to_the_os_handler_without_running_it() {
    let path = Path::new("/tmp/a dir/config.toml");
    let command = command(path);
    let program = command.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if cfg!(target_os = "macos") {
        assert_eq!(program, "open");
    } else if cfg!(target_os = "windows") {
        assert_eq!(program, "cmd");
    } else {
        assert_eq!(program, "xdg-open");
    }
    assert_eq!(
        args.last().map(String::as_str),
        Some("/tmp/a dir/config.toml"),
        "the path is one argument, never split"
    );
}

/// The probe of `the_linux_command_drops_the_driver_selection` reads the marker from the process
/// environment, which a test cannot set, so the test runs it in a child with the marker.
#[cfg(target_os = "linux")]
mod driver_selection {
    use std::path::Path;
    use std::process::Command;

    use super::command;

    const PROBE: &str = "open_file::tests::driver_selection::probe_command_drops_the_selection";
    const GUARD: &str = "REQFLUX_OPEN_FILE_PROBE";
    const MARKER: &str = "GPUI_VULKAN_DRIVER_SELECT";

    /// Runs only inside the child of the test below; elsewhere it passes at once.
    #[test]
    fn probe_command_drops_the_selection() {
        if std::env::var_os(GUARD).is_none() {
            return;
        }
        let command = command(Path::new("/tmp/config.toml"));
        let mut removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        removed.sort();
        assert_eq!(
            removed,
            ["GPUI_VULKAN_DRIVER_SELECT", "VK_LOADER_DRIVERS_SELECT"]
        );
    }

    #[test]
    fn the_linux_command_drops_the_driver_selection() {
        let exe = std::env::current_exe().expect("the test binary has a path");
        let mut child = Command::new(exe);
        child.args(["--exact", PROBE, "--nocapture"]);
        child.env(GUARD, "1");
        child.env(MARKER, "*intel_icd*");
        let output = child.output().expect("the probe starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(
            stdout.contains("1 passed"),
            "the probe did not run\n{stdout}\n{stderr}"
        );
    }
}
