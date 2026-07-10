//! Sub-agent orchestration (Codex-style multi-agent control plane).

mod control;
mod graph;
mod harness;
mod id;
mod registry;
mod types;

pub use control::{AgentControl, SubagentEventForwarder, SubagentSpawnConfig};
pub use graph::AgentGraphStore;
pub use harness::SubagentHarness;
pub use id::generate_agent_name;
pub use registry::{AgentRegistry, SubagentRecord};
pub use types::{SubagentBootstrap, SubagentInfo, SubagentLimits, SubagentStatus};
