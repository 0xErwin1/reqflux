//! Discovery of live bridge targets in the endpoint directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_mcp_protocol::{AppId, ProcessId};
use thiserror::Error;
use tokio::time::timeout;

use super::client;
use super::descriptor::{ValidatedTarget, read_descriptor, validate};

/// At most this many descriptor files are considered, in name order.
pub const MAX_DISCOVERY_CANDIDATES: usize = 128;

/// How long a target has to accept a connection before it is skipped.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Why discovery could not list the endpoint directory.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// The operating system exposes no owner-scoped runtime directory.
    #[error("the operating system did not expose an owner-scoped runtime directory")]
    RuntimeRoot,
    /// The endpoint directory could not be read.
    #[error("could not read endpoint directory {1}: {0}")]
    ReadDir(#[source] std::io::Error, PathBuf),
}

/// The `run` endpoint directory below the runtime root.
///
/// # Errors
///
/// Returns [`DiscoveryError::RuntimeRoot`] when the operating system exposes
/// no owner-scoped runtime directory.
pub fn default_endpoint_dir() -> Result<PathBuf, DiscoveryError> {
    let root = gpui_mcp_protocol::runtime_root().map_err(|_| DiscoveryError::RuntimeRoot)?;
    Ok(root.join("run"))
}

/// List every usable bridge target in `endpoint_dir`, sorted by target id.
///
/// A descriptor is skipped, never an error, when it is unreadable, invalid,
/// describes a dead process, or its endpoint refuses a connection. A missing
/// directory yields no targets.
pub async fn discover(
    endpoint_dir: &Path,
    app_id: Option<&AppId>,
) -> Result<Vec<ValidatedTarget>, DiscoveryError> {
    let mut paths = match fs::read_dir(endpoint_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(DiscoveryError::ReadDir(error, endpoint_dir.to_path_buf())),
    };
    paths.sort_unstable();
    paths.truncate(MAX_DISCOVERY_CANDIDATES);

    let mut targets = Vec::new();
    for path in paths {
        let descriptor = match read_descriptor(&path) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                log::debug!("skipping endpoint descriptor {}: {error}", path.display());
                continue;
            }
        };
        if let Some(expected) = app_id
            && descriptor.app_id != *expected
        {
            continue;
        }
        if !pid_is_alive(descriptor.pid) {
            log::debug!(
                "skipping endpoint descriptor {}: pid {} is gone",
                path.display(),
                descriptor.pid
            );
            continue;
        }
        let target = match validate(&descriptor, &path) {
            Ok(()) => ValidatedTarget { descriptor },
            Err(error) => {
                log::debug!("skipping endpoint descriptor {}: {error}", path.display());
                continue;
            }
        };
        if probe(&target).await {
            targets.push(target);
        } else {
            log::debug!(
                "skipping endpoint descriptor {}: its endpoint refused a connection",
                path.display()
            );
        }
    }
    targets.sort_unstable_by_key(|target| target.target_id());
    Ok(targets)
}

/// Whether `pid` still names a process on this machine.
///
/// Linux answers from `/proc`; elsewhere every pid counts as alive and the
/// connection probe filters dead descriptors.
pub(crate) fn pid_is_alive(pid: ProcessId) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.get().to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

/// Whether the endpoint accepts a connection within the probe timeout.
async fn probe(target: &ValidatedTarget) -> bool {
    matches!(
        timeout(
            PROBE_TIMEOUT,
            client::connect(&target.descriptor().endpoint)
        )
        .await,
        Ok(Ok(_))
    )
}
