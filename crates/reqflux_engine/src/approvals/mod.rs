//! In-app approvals of MCP calls (mcp-approvals S1, S2): the protocol between `reqflux mcp`
//! and a running window, the window's listener and the discovery of a live window.
//!
//! A window that wants to answer approvals calls [`serve`] with an [`ApprovalHandler`]. It
//! creates `$XDG_RUNTIME_DIR/reqflux/` (0700), listens on `approvals-<pid>.sock` (0600) there
//! and writes the descriptor `approvals-<pid>.json` (0600) beside it, the way the gpui-mcp
//! bridge advertises its endpoint. Dropping the returned [`ApprovalListener`] stops it and
//! removes both files. With no `XDG_RUNTIME_DIR` there are no approvals.
//!
//! The protocol is newline-delimited JSON, one exchange per connection: the client sends one
//! [`ApprovalRequest`] line and the window answers one [`ApprovalResponse`] line with the
//! same `id`. Both carry [`PROTOCOL_VERSION`]. A request names the tool, its class, the
//! workspace, the targets by name and, for `request_execute`, the method and the masked URL;
//! never an argument value. Optional fields added within version 1 name the MCP client, the
//! ids the call targets, the environment, the policy rule that asked and when; an end that
//! does not know them leaves them out. [`ask`] is the client's half of one exchange.
//!
//! [`preview`] is what the window shows of a call from its own copy of the workspace: the
//! request prepared without its scripts, its secrets masked, and the variables it uses.
//!
//! Platforms: the listener uses `interprocess` local sockets, a Unix-domain socket on Unix.
//! On Linux and the BSDs (macOS included) a connection from another user is refused by its
//! peer credentials. Elsewhere [`serve`] reports [`ApprovalError::Unsupported`].

mod client;
mod preview;
#[cfg(test)]
mod preview_tests;
mod serve;
#[cfg(all(test, unix))]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use client::{AskError, ask};
pub use preview::{
    CallPreview, UsedVariable, VariableSource, breadcrumb, find_environment, preview,
};
pub use serve::{ApprovalHandler, ApprovalListener, Reply, serve, serve_in};

/// The protocol version both ends speak.
pub const PROTOCOL_VERSION: u32 = 1;

/// The largest line either end reads, in bytes.
pub const MAX_LINE_BYTES: u64 = 64 * 1024;

/// The workspace a call works on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRef {
    /// The workspace id.
    pub id: String,
    /// The workspace name.
    pub name: String,
}

/// One approval request, from `reqflux mcp` to the window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// [`PROTOCOL_VERSION`].
    pub version: u32,
    /// The request id the response repeats.
    pub id: u64,
    /// The tool name.
    pub tool: String,
    /// The tool class: `read`, `write`, `execute` or `destructive`.
    pub class: String,
    /// The workspace the call works on; `None` for a call that opens or creates one.
    pub workspace: Option<WorkspaceRef>,
    /// The entities the call targets, by name.
    pub targets: Vec<String>,
    /// The method of a `request_execute`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The URL of a `request_execute`, its secrets masked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The name the MCP client gave in its `initialize` (`clientInfo`), such as
    /// `Claude Code`; absent when it gave none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// The ids of the entities the call targets, in the order of `targets`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_ids: Vec<String>,
    /// The environment the call uses, by name or slug as the agent passed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// The policy rule that asked, such as `execute → approval`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// When the client asked, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<i64>,
}

/// The window's answer to one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// Run this call.
    AllowOnce,
    /// Run this call and every later one of the same class and workspace until this
    /// `reqflux mcp` exits.
    AllowSession,
    /// Refuse the call.
    Deny,
}

/// One approval response, from the window to `reqflux mcp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalResponse {
    /// [`PROTOCOL_VERSION`].
    pub version: u32,
    /// The id of the request it answers.
    pub id: u64,
    /// The decision.
    pub decision: ApprovalDecision,
    /// Why the user denied the call, as they typed it; absent when they gave none. The
    /// client cleans it before it shows it to anyone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The descriptor a listening window writes beside its socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowDescriptor {
    /// [`PROTOCOL_VERSION`].
    pub version: u32,
    /// The window's process id.
    pub pid: u32,
    /// The socket, in the descriptor's directory.
    pub socket: PathBuf,
}

/// Why the listener could not start.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    /// `XDG_RUNTIME_DIR` is not set.
    #[error("XDG_RUNTIME_DIR is not set, so the window cannot listen for approvals")]
    NoRuntimeDir,
    /// The platform has no supported local socket.
    #[error("in-app approvals are not supported on this platform")]
    Unsupported,
    /// The directory, the socket or the descriptor could not be created.
    #[error("cannot listen for approvals in {path}: {source}")]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The IO failure.
        #[source]
        source: std::io::Error,
    },
}

/// `$XDG_RUNTIME_DIR/reqflux`, where windows advertise their approval sockets; `None`
/// without a runtime dir.
#[must_use]
pub fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(value).join("reqflux"))
}

/// The socket of the process `pid` in `dir`.
#[must_use]
pub fn socket_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("approvals-{pid}.sock"))
}

/// The descriptor of the process `pid` in `dir`.
#[must_use]
pub fn descriptor_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("approvals-{pid}.json"))
}

/// The descriptor of the first live window in `dir`, in name order: one of this protocol
/// version, owner-only, whose process is alive and whose socket is in `dir`. A missing or
/// unreadable directory has none; a descriptor that breaks a rule is skipped.
#[must_use]
pub fn find_window(dir: &Path) -> Option<WindowDescriptor> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("approvals-") && name.ends_with(".json"))
        })
        .collect();
    paths.sort_unstable();
    paths
        .iter()
        .find_map(|path| read_descriptor(path).filter(|descriptor| is_live(descriptor, dir)))
}

/// The descriptor at `path`, when it is a small owner-only regular file holding one.
fn read_descriptor(path: &Path) -> Option<WindowDescriptor> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_LINE_BYTES {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return None;
        }
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Whether `descriptor` speaks this version, names a live process and a socket in `dir`.
fn is_live(descriptor: &WindowDescriptor, dir: &Path) -> bool {
    descriptor.version == PROTOCOL_VERSION
        && descriptor.socket.parent() == Some(dir)
        && descriptor.socket == socket_path(dir, descriptor.pid)
        && pid_is_alive(descriptor.pid)
        && descriptor.socket.exists()
}

/// Whether `pid` still names a process. Linux answers from `/proc`; elsewhere every pid
/// counts as alive and a refused connection reports the window gone.
fn pid_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        pid != 0
    }
}
