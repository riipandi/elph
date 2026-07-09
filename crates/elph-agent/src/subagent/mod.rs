//! Sub-agent orchestration (Codex-style multi-agent control plane).

mod control;
mod registry;
mod types;

pub use control::{AgentControl, SubagentSpawnConfig};
pub use registry::{AgentRegistry, SubagentRecord};
pub use types::{SubagentInfo, SubagentLimits, SubagentStatus};
