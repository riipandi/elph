# Porting status: pi-agent → elph-agent

| Field               | Value                                                                         |
| ------------------- | ----------------------------------------------------------------------------- |
| **Last audited**    | 2026-07-11T11:23:28Z                                                          |
| **Upstream**        | `@earendil-works/pi-agent-core` · `packages/agent` · **v0.80.6** + Unreleased |
| **Upstream commit** | `4c18610`                                                                     |
| **Elph crate**      | `crates/elph-agent`                                                           |
| **Depends on**      | `elph-ai` — see [pi-ai.md](./pi-ai.md)                                        |

---

## At a glance (post Sprint 1–4)

| Area                                       | Assessment                       |
| ------------------------------------------ | -------------------------------- |
| Core agent + agent loop                    | **Parity**                       |
| `AgentThinkingLevel::Max`                  | **Parity**                       |
| `added_tool_names` on tool results + loop  | **Parity**                       |
| Session entry transforms / projectors      | **Parity**                       |
| Compaction estimate timestamp gate (#6464) | **Parity**                       |
| Goals / MCP / subagent / plugins / tools   | **Missing in pi** (Elph product) |

---

## Audit log

| Timestamp (UTC)      | Pi version / commit               | Notes                                                                                  |
| -------------------- | --------------------------------- | -------------------------------------------------------------------------------------- |
| 2026-07-11T11:12:19Z | `0.80.6` + Unreleased @ `4c18610` | Initial gap audit.                                                                     |
| 2026-07-11T11:23:28Z | `0.80.6` + Unreleased @ `4c18610` | **Sprints 1–4:** Max thinking, deferred tool names, session transforms, estimate gate. |

---

## What landed

| Sprint item                              | Location                                                                    |
| ---------------------------------------- | --------------------------------------------------------------------------- |
| `AgentThinkingLevel::Max`                | `src/types/enums.rs`, harness helpers                                       |
| `AgentToolResult.added_tool_names`       | `src/types/tools.rs`                                                        |
| Loop → `Message::ToolResult` propagation | `src/agent_loop/tools/messages.rs`                                          |
| After-tool / harness patches             | `loop_config.rs`, `execute.rs`, `ToolResultPatch`                           |
| `SessionContextBuildOptions`             | `src/session/context.rs`                                                    |
| `entry_transforms` / `entry_projectors`  | `build_session_context_with_options`, `Session::build_context_with_options` |
| Timestamp-aware last usage               | `src/compaction/estimation.rs`                                              |

---

## Remaining / watch

| Item                                                     | Priority | Notes                                                  |
| -------------------------------------------------------- | -------- | ------------------------------------------------------ |
| Split-turn summary serialization regression test (#5536) | P2       | Confirm coverage if not already present                |
| JSONL v3 header custom `metadata`                        | P2 / N/A | Only if interop with pi coding-agent JSONL is required |
| Product modules (goals, MCP, subagent, tools, …)         | —        | Elph-only; not pi-agent gaps                           |

---

## Elph-only (not port gaps)

`goals/`, `mcp/`, `subagent/`, `plugins/`, `tools/`, `mode/`, `sandbox/`, `datastore/`, session_dir + Turso backends.
