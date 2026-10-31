//! Direct client for the gpui-mcp bridge embedded in every ReqFlux window.
//!
//! Discovery reads the endpoint descriptors the bridge writes under the
//! runtime directory; [`BridgeClient`] speaks the framed wire protocol to one
//! target. The `ui_*` tools build on this surface.

mod client;
mod descriptor;
mod discover;
#[cfg(all(test, unix))]
pub(crate) mod fake;
mod session;
#[cfg(all(test, unix))]
mod tests;

pub use client::{BridgeClient, CallError, IO_TIMEOUT};
pub use descriptor::{
    DescriptorError, InvalidTargetId, MAX_DESCRIPTOR_BYTES, TargetId, TargetSummary,
    ValidatedTarget, read_and_validate,
};
pub(crate) use discover::pid_is_alive;
pub use discover::{DiscoveryError, MAX_DISCOVERY_CANDIDATES, default_endpoint_dir, discover};
pub use session::{BridgeSession, SessionError};
