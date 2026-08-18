//! Sub-agent orchestration (Codex-style multi-agent control plane).

mod control;
#[cfg(feature = "backend-turso")]
mod graph;
#[cfg(feature = "backend-turso")]
mod harness;
mod id;
mod registry;
mod types;

pub use control::{AgentControl, SubagentEventForwarder, SubagentSpawnConfig};
#[cfg(feature = "backend-turso")]
pub use graph::AgentGraphStore;
#[cfg(feature = "backend-turso")]
pub use harness::SubagentHarness;
pub use id::generate_agent_name;
pub use registry::{AgentRegistry, SubagentRecord};
pub use types::{SubagentBootstrap, SubagentInfo, SubagentLimits, SubagentOutput, SubagentStatus};

/// Append a streamed output delta to a subagent's persistent `events.jsonl`.
///
/// Best-effort; failures are swallowed so event forwarding never blocks the
/// agent loop. `dir` is the subagent artifact directory
/// (`outputs_root/subagents/<agent_id>`).
pub fn subagent_persist_event(dir: &std::path::Path, event: &str, text: &str) {
    types::persist::append_event(dir, event, text);
}
