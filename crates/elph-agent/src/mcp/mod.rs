//! MCP client integration via [rmcp](https://crates.io/crates/rmcp).

#[cfg(feature = "mcp")]
mod client;
#[cfg(feature = "mcp")]
mod config;
#[cfg(feature = "mcp")]
mod registry;

#[cfg(feature = "mcp")]
pub use client::{
    McpProbeResult, PROBE_TIMEOUT, call_stdio_tool, list_tools, parse_stdio_config, probe_server, probe_stdio_server,
};
#[cfg(feature = "mcp")]
pub use config::{McpConfig, McpServerConfig, McpStdioConfig};
#[cfg(feature = "mcp")]
pub use registry::{McpToolDescriptor, McpToolRegistry};
