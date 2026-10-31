//! The MCP server that exposes the engine to AI agents.

pub mod approvals;
pub mod bridge;
pub mod engine_tools;
pub mod server;
pub mod tools;

/// The application identifier a window reports and a client asks for.
pub use gpui_mcp_protocol::AppId;
