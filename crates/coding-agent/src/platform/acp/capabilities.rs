//! Advertised ACP v2 agent capabilities and implementation info.

use agent_client_protocol::schema::v2::{
    AgentCapabilities, Implementation, PromptCapabilities, PromptEmbeddedContextCapabilities, SessionCapabilities,
    SessionDeleteCapabilities,
};

pub fn implementation() -> Implementation {
    Implementation::new("elph", env!("CARGO_PKG_VERSION")).title("Elph")
}

pub fn agent_capabilities() -> AgentCapabilities {
    AgentCapabilities::new().session(
        SessionCapabilities::new()
            .prompt(PromptCapabilities::new().embedded_context(PromptEmbeddedContextCapabilities::new()))
            .delete(SessionDeleteCapabilities::new()),
    )
}
