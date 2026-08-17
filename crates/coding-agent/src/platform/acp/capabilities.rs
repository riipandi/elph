//! Advertised ACP v2 agent capabilities and implementation info.

use agent_client_protocol::schema::v2::{
    AgentCapabilities, Implementation, McpCapabilities, McpHttpCapabilities, McpStdioCapabilities, PromptCapabilities,
    PromptEmbeddedContextCapabilities, SessionCapabilities, SessionDeleteCapabilities,
};

pub fn implementation() -> Implementation {
    Implementation::new("elph", env!("CARGO_PKG_VERSION")).title("Elph")
}

pub fn agent_capabilities() -> AgentCapabilities {
    AgentCapabilities::new().session(
        SessionCapabilities::new()
            .prompt(PromptCapabilities::new().embedded_context(PromptEmbeddedContextCapabilities::new()))
            .mcp(
                McpCapabilities::new()
                    .stdio(McpStdioCapabilities::new())
                    .http(McpHttpCapabilities::new()),
            )
            .delete(SessionDeleteCapabilities::new()),
    )
}
