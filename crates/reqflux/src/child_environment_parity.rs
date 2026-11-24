//! The engine's `restore_child_environment` mirrors `gpui_linux::restore_child_environment`
//! because the engine does not link GPUI. Both read `GPUI_VULKAN_DRIVER_SELECT` from the process
//! environment, which a test cannot change (`set_var` is unsafe), so each case re-runs this test
//! binary on [`PROBE`] with the marker in its environment, and the probe compares the updates
//! both functions record.

use std::process::Command;

const PROBE: &str = "child_environment_parity::probe_both_rules_record_the_same_updates";
const GUARD: &str = "REQFLUX_CHILD_ENV_PROBE";
const MARKER: &str = "GPUI_VULKAN_DRIVER_SELECT";

type Updates = Vec<(String, Option<String>)>;

fn updates(restore: fn(&mut Command)) -> Updates {
    let mut command = Command::new("reqflux-child");
    restore(&mut command);
    let mut updates: Updates = command
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

/// Runs only inside the child of [`assert_parity_for`]; elsewhere it passes at once.
#[test]
fn probe_both_rules_record_the_same_updates() {
    if std::env::var_os(GUARD).is_none() {
        return;
    }
    let engine = updates(reqflux_engine::child_environment::restore_child_environment);
    let gpui = updates(gpui_linux::restore_child_environment);
    assert_eq!(engine, gpui, "marker {:?}", std::env::var_os(MARKER));
}

fn assert_parity_for(marker: Option<&str>) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe);
    child
        .args(["--exact", PROBE, "--nocapture"])
        .env(GUARD, "1");
    match marker {
        Some(marker) => child.env(MARKER, marker),
        None => child.env_remove(MARKER),
    };
    let output = child.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "marker {marker:?}\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed"),
        "the probe did not run for marker {marker:?}\n{stdout}\n{stderr}"
    );
    Ok(())
}

#[test]
fn the_engine_rule_matches_gpui_linux_for_every_marker() -> std::io::Result<()> {
    for marker in [
        None,
        Some(""),
        Some("0"),
        Some("fallback"),
        Some("*intel_icd*"),
        Some("*intel_icd*,*intel_hasvk_icd*"),
    ] {
        assert_parity_for(marker)?;
    }
    Ok(())
}
