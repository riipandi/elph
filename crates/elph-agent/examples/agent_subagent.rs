//! Subagent orchestration — registry, info, limits, status, and agent name generation.
//!
//! Demonstrates: `SubagentInfo`, `SubagentStatus`, `SubagentLimits`,
//! `SubagentBootstrap`, `generate_agent_name`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_subagent
//! ```

use elph_agent::agent::subagent::{SubagentInfo, SubagentLimits, SubagentStatus, generate_agent_name};

fn main() {
    // ── 1. generate_agent_name: readable names for subagents ──
    println!("=== Agent Name Generation ===");
    for _ in 0..5 {
        println!("  {}", generate_agent_name());
    }

    // ── 2. SubagentStatus variants ──
    println!("\n=== SubagentStatus ===");
    for status in &[
        SubagentStatus::Pending,
        SubagentStatus::Running,
        SubagentStatus::Idle,
        SubagentStatus::Done,
        SubagentStatus::Error,
    ] {
        println!("  {status:?}");
    }

    // ── 3. SubagentInfo: describe a subagent ──
    println!("\n=== SubagentInfo ===");
    let info = SubagentInfo {
        id: "agent-sd-7f3a".into(),
        session_id: "session-abc".into(),
        task_name: "explorer".into(),
        agent_path: "root/agent-sd-7f3a".into(),
        depth: 1,
        status: SubagentStatus::Running,
        parent_session_id: "session-root".into(),
    };
    println!("  id:                {}", info.id);
    println!("  session_id:        {}", info.session_id);
    println!("  task_name:         {}", info.task_name);
    println!("  agent_path:        {}", info.agent_path);
    println!("  depth:             {}", info.depth);
    println!("  status:            {:?}", info.status);
    println!("  parent_session_id: {}", info.parent_session_id);

    // ── 4. SubagentLimits ──
    println!("\n=== SubagentLimits ===");
    let default_limits = SubagentLimits::default();
    println!(
        "  default: max_depth={}, max_concurrent={}",
        default_limits.max_depth, default_limits.max_concurrent
    );
    let custom_limits = SubagentLimits {
        max_depth: 5,
        max_concurrent: 8,
    };
    println!(
        "  custom:  max_depth={}, max_concurrent={}",
        custom_limits.max_depth, custom_limits.max_concurrent
    );

    // ── 5. SubagentBootstrap ──
    println!("\n=== SubagentBootstrap ===");
    // Note: AgentHarnessResources construction is complex; showing the type info instead.
    println!("  (SubagentBootstrap requires AgentHarnessResources — not constructed here)");
    println!("  Fields: project_key, cwd, sessions_root, resources, stream_options, thinking_level, agent_graph");

    println!("\nDone.");
}
