---
type: Concept
title: Subagents — Multi-Agent Collaboration
description: Subagent orchestration in Elph — output durability, TurnGuard, wait-for-output, persistent artifacts, and shared database handles
tags: [subagent, collaboration, multi-agent, output-persistence]
openwiki:
    roles: [domain, architecture]
    change_kinds: [lifecycle, public-api]
    source_paths: [crates/elph-agent/src/agent/subagent/]
    symbols:
        [
            SubagentOutput,
            SubagentBootstrap,
            SubagentHarness,
            AgentControl,
            TurnGuard,
            SubagentEventForwarder,
        ]
    test_paths: [crates/elph-agent/tests/subagent.rs]
    invariants:
        [
            TurnGuard releases in-flight slot on Drop even if the task panics; wait_for_turn blocks until in-flight count reaches 0; output.summary() never returns empty string,
        ]
    validation_commands: [cargo test -p elph-agent --test subagent]
---

# Subagents

Subagent orchestration lives in `crates/elph-agent/src/agent/subagent/`. It enables a parent agent to spawn child agents, send them tasks, and collect results. Subagents share the parent's database and session infrastructure.

## Module Structure

```
crates/elph-agent/src/agent/subagent/
├── mod.rs         — re-exports, pub fn subagent_persist_event()
├── control.rs     — AgentControl: spawn, followup, wait, abort
├── harness.rs     — SubagentHarness: wrap AgentHarness + output tracking
├── registry.rs    — AgentRegistry: in-memory agent record store
├── types.rs       — SubagentOutput, SubagentBootstrap, SubagentInfo, SubagentLimits, persist module
├── id.rs          — generate_agent_name()
└── graph.rs       — AgentGraphStore: persistent spawn edge tracking
```

New public exports from `crates/elph-agent/src/lib.rs` (commit `951fea9`):

- `SubagentOutput` — struct with `text`, `output_path`, `finished_at_ms`, `turns`
- `subagent_persist_event()` — append streamed delta to `events.jsonl`
- `BUDGET_LIMIT_PROMPT_PREFIX`, `CONTINUATION_PROMPT_PREFIX` — from `goals/steering.rs`

## SubagentOutput (commit `951fea9`)

```rust
pub struct SubagentOutput {
    pub text: String,                     // Final assistant text, trimmed
    pub output_path: Option<String>,       // Path to output.md (when artifacts dir configured)
    pub finished_at_ms: Option<i64>,       // Epoch ms of last completed turn
    pub turns: u32,                       // Assistant turns completed
}
```

`SubagentOutput::summary()` returns a non-empty human-readable string: the text when present, or a fallback with the log path, or `"(no output captured)"`.

## SubagentBootstrap (commit `eba87a7`, `951fea9`)

Two new fields:

| Field          | Type                    | Description                                                                                                         |
| -------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `database`     | `Option<Arc<Database>>` | Shared, already-open database handle. Child repos connect from this handle instead of opening independently.        |
| `outputs_root` | `Option<PathBuf>`       | Session artifacts root: `APP_DATA/sessions/<SESSION_ID>`. When set, each spawned agent writes persistent artifacts. |

### Persistent Artifact Layout

`outputs_root/subagents/<agent_id>/`:

| File           | Format | Description                                           |
| -------------- | ------ | ----------------------------------------------------- |
| `output.md`    | Text   | Final assistant text (re-written per completed turn)  |
| `events.jsonl` | JSONL  | Streamed output deltas (append-only, replayable)      |
| `meta.json`    | JSON   | Spawn metadata (agent id, task, path, depth, session) |

The `persist` module in `types.rs` provides helper functions: `write_output()`, `append_event()`, `write_meta()`.

## TurnGuard (commit `4baf7aa`)

```rust
pub struct TurnGuard {
    harness: Arc<SubagentHarness>,
}
```

Prevents subagent turn wait races. Key invariants:

- `TurnGuard` releases the in-flight slot on `Drop` even if the background task panics or is cancelled.
- `SubagentHarness::turn_started()` increments the in-flight counter synchronously before spawning the task.
- `SubagentHarness::wait_for_turn()` blocks until the in-flight count reaches 0.
- `SubagentHarness::turn_guard()` (on `Arc<Self>`) creates a guard for `RAII` lifetime management.

## wait_agent_for_output (commit `951fea9`, `4baf7aa`)

`AgentControl` added two new methods:

```rust
impl AgentControl {
    /// Block until the subagent's current turn completes and return its final
    /// assistant text. Falls back to a readable placeholder.
    pub async fn wait_agent_for_output(&self, agent_id: &str) -> Result<String, String>;

    pub async fn wait_agent_cancellable_for_output(&self, agent_id: &str, signal: Option<&CancellationToken>) -> Result<String, String>;
}
```

The `wait_agent` tool in `crates/elph-agent/src/tools/collaboration.rs` now returns the subagent's output text instead of a static `"{agent_id} is idle"` message (commit `951fea9`). `list_agents` also includes `SubagentOutput` in the response.

## AgentGraphStore (commit `eba87a7`)

`AgentGraphStore` now supports a shared database handle:

```rust
pub struct AgentGraphStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,  // injected by host
}
```

When `database` is set, the store connects from this handle. The host owns the open/apply-migrations lifetime.

## Event Forwarding (commit `4baf7aa`)

`SubagentEventForwarder` is subscribed exactly once at spawn time (not per-followup) to avoid accumulating duplicate subscribers. `SubagentHarness::forward_events()` subscribes a forwarder to the harness events.

## Key Features Added in This Cycle

| Feature                                               | Commit    | Details                                               |
| ----------------------------------------------------- | --------- | ----------------------------------------------------- |
| Persistent subagent output artifacts                  | `951fea9` | `output.md`, `events.jsonl`, `meta.json`              |
| `SubagentOutput` + `summary()`                        | `951fea9` | Never-empty result for tools/UI                       |
| `wait_agent_for_output()`                             | `951fea9` | Returns final assistant text                          |
| Collaboration tool returns text                       | `951fea9` | `wait_agent` returns output instead of static message |
| `TurnGuard` + `wait_for_turn()`                       | `4baf7aa` | RAII guard, prevents race between spawn and wait      |
| Event forwarding at spawn (not per-turn)              | `4baf7aa` | Avoids duplicate subscribers                          |
| Shared database handle (`SubagentBootstrap.database`) | `eba87a7` | Removes `elph-db` crate, shares handle from parent    |
| `subagent_persist_event()` public export              | `951fea9` | Best-effort event log append                          |

## Source References

- `crates/elph-agent/src/agent/subagent/types.rs` — `SubagentOutput`, `SubagentBootstrap`, `persist` module
- `crates/elph-agent/src/agent/subagent/harness.rs` — `SubagentHarness`, `TurnGuard`, `forward_events()`
- `crates/elph-agent/src/agent/subagent/control.rs` — `AgentControl`, `wait_agent_for_output()`, `followup_task()`
- `crates/elph-agent/src/agent/subagent/registry.rs` — `AgentRegistry`, `agents_mut()`
- `crates/elph-agent/src/agent/subagent/graph.rs` — `AgentGraphStore`, `with_database()`
- `crates/elph-agent/src/agent/subagent/mod.rs` — re-exports, `subagent_persist_event()`
- `crates/elph-agent/src/tools/collaboration.rs` — `wait_agent` tool updated to return output
- `crates/elph-agent/tests/subagent.rs` — subagent tests
- `docs/design/subagent-output-durability.md` — design notes
