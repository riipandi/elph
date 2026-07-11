# Porting status: pi-agent → elph-agent

| Field               | Value                                                                                  |
| ------------------- | -------------------------------------------------------------------------------------- |
| **Last audited**    | 2026-07-11T11:12:19Z                                                                   |
| **Upstream**        | `@earendil-works/pi-agent-core` · `packages/agent` · **v0.80.6** + Unreleased          |
| **Upstream commit** | `4c18610` (2026-07-11)                                                                 |
| **Elph crate**      | `crates/elph-agent`                                                                    |
| **Depends on**      | `elph-ai` (see [pi-ai.md](./pi-ai.md) for provider/catalog gaps that affect the agent) |
| **Primary sources** | pi `CHANGELOG.md` / `src/**`; elph `src/**` + `docs/**`                                |

---

## At a glance

| Area                                            | Assessment                                          |
| ----------------------------------------------- | --------------------------------------------------- |
| Core agent + agent loop                         | **Mostly parity** with 0.80.x                       |
| Harness (session, compaction, skills, prompts)  | **Mostly parity**; pluggable context transforms lag |
| Thinking level `max`                            | **Missing in elph** (needs `elph-ai` too)           |
| `addedToolNames` propagation (Unreleased)       | **Missing in elph**                                 |
| `prepareNextTurnWithContext` (0.80.3)           | **Parity**                                          |
| Length-truncated tool-call fail (#6285)         | **Parity**                                          |
| Goals / MCP / subagent / plugins / coding tools | **Missing in pi** (Elph product surface)            |

---

## Audit log

| Timestamp (UTC)      | Pi version / commit               | Notes                                                                                                                                            |
| -------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-07-11T11:12:19Z | `0.80.6` + Unreleased @ `4c18610` | Initial gap audit. Elph is a **superset** of pi-agent for product features; core loop/harness lag is smaller than catalog/type lag in `elph-ai`. |

Add a new row on every re-audit; do not delete old rows.

---

## Module map

### Upstream pi-agent (small core)

```
packages/agent/src/
├── agent.ts              # Agent state machine
├── agent-loop.ts         # Turn / tool loop
├── types.ts
├── proxy.ts
├── node.ts
└── harness/
    ├── agent-harness.ts
    ├── compaction/
    ├── session/          # jsonl + memory storage
    ├── env/nodejs.ts
    ├── messages.ts
    ├── prompt-templates.ts
    ├── skills.ts
    ├── system-prompt.ts
    ├── types.ts
    └── utils/
```

### Elph agent (core + product extensions)

| elph module                                     | Corresponds to pi                     | Status                                                                  |
| ----------------------------------------------- | ------------------------------------- | ----------------------------------------------------------------------- |
| `agent/`, `agent_loop/`                         | `agent.ts`, `agent-loop.ts`           | **Partial** (see loop gaps)                                             |
| `types/`                                        | `types.ts`                            | **Partial**                                                             |
| `harness/`                                      | `harness/*`                           | **Partial**                                                             |
| `session/`                                      | `harness/session/*`                   | **Partial** + **Elph backends**                                         |
| `compaction/`                                   | `harness/compaction/*`                | **Partial**                                                             |
| `messages/`, `prompt_templates/`, `skills/`     | harness messages / templates / skills | **Parity** (core)                                                       |
| `proxy/`                                        | `proxy.ts`                            | **Parity**                                                              |
| `env/`                                          | `harness/env/nodejs.ts`               | **Parity** (local Rust env)                                             |
| `builder.rs`, `runtime/`, `event_stream/`       | wiring helpers                        | **Missing in pi** (Rust ergonomics)                                     |
| `goals/`                                        | —                                     | **Missing in pi**                                                       |
| `mcp/`                                          | —                                     | **Missing in pi**                                                       |
| `subagent/`                                     | —                                     | **Missing in pi**                                                       |
| `plugins/` (extensions)                         | —                                     | **Missing in pi** (coding-agent package has related concepts elsewhere) |
| `tools/` (bash, edit, web, …)                   | —                                     | **Missing in pi** (lives in coding-agent upstream)                      |
| `mode/` (plan / collaboration)                  | —                                     | **Missing in pi** (or only higher up)                                   |
| `sandbox/`, `datastore/`, `migration/`, `init/` | —                                     | **Missing in pi**                                                       |

> When comparing to mainstream, treat **core** (agent loop + harness session/compaction)
> separately from **product** modules. Product modules are not “behind pi-agent”;
> they are Elph scope beyond `pi-agent-core`.

---

## Agent types and loop

| Capability                                          | pi  | elph                                   | Status                                     |
| --------------------------------------------------- | --- | -------------------------------------- | ------------------------------------------ |
| Agent state, queue, steer                           | yes | yes                                    | **Parity**                                 |
| Tool execution sequential / parallel                | yes | yes                                    | **Parity**                                 |
| `AgentToolResult.terminate` early stop              | yes | yes                                    | **Parity**                                 |
| `AgentToolResult.addedToolNames` (Unreleased #6474) | yes | no                                     | **Missing in elph**                        |
| Propagate `addedToolNames` → `ToolResult` message   | yes | no                                     | **Missing in elph**                        |
| `ThinkingLevel` incl. `xhigh`                       | yes | yes (`AgentThinkingLevel::Xhigh`)      | **Parity**                                 |
| Thinking level `max` (0.80.6)                       | yes | no                                     | **Missing in elph**                        |
| `prepareNextTurn` (abort signal)                    | yes | yes (legacy + modern)                  | **Parity**                                 |
| `prepareNextTurnWithContext` (0.80.3)               | yes | yes (`prepare_next_turn` with context) | **Parity**                                 |
| `shouldStopAfterTurn`                               | yes | yes                                    | **Parity**                                 |
| Length stop → fail tool calls (#6285, 0.80.4)       | yes | yes                                    | **Parity**                                 |
| Ignore late tool progress after settle (#5573)      | yes | verify on re-audit                     | **Partial** (assume ported; confirm tests) |
| `StreamFn` via `Models.streamSimple` (0.80.0)       | yes | yes                                    | **Parity**                                 |
| `maxRetryDelayMs` / retry options                   | yes | yes                                    | **Parity**                                 |

---

## Harness

| Capability                                             | pi  | elph                      | Status                                       |
| ------------------------------------------------------ | --- | ------------------------- | -------------------------------------------- |
| `AgentHarness` + events (model/tools/thinking updates) | yes | yes                       | **Parity**                                   |
| Required `Models` auth path (0.80.0 break)             | yes | yes                       | **Parity**                                   |
| Compaction + branch summarization                      | yes | yes                       | **Parity** (core flow)                       |
| Split-turn compaction                                  | yes | yes                       | **Parity**                                   |
| Serialize split-turn summary requests (#5536)          | yes | verify on re-audit        | **Partial**                                  |
| `getLastAssistantUsage` / estimates                    | yes | yes                       | **Partial** vs pi timestamp-boundary (#6464) |
| Skills + system prompt formatting                      | yes | yes                       | **Parity**                                   |
| Prompt templates load/substitute                       | yes | yes                       | **Parity**                                   |
| Shell exec options rename (`ShellExecOptions`)         | yes | yes                       | **Parity**                                   |
| Shell timeout validation (#6181)                       | yes | verify                    | **Partial**                                  |
| Session name newline normalize (#5999)                 | yes | verify                    | **Partial**                                  |
| Zero-usage ignore after truncated responses (#5526)    | yes | partial via usage filters | **Partial**                                  |

### Session context building (0.80.4)

| Capability                                       | pi  | elph                                                           | Status                                      |
| ------------------------------------------------ | --- | -------------------------------------------------------------- | ------------------------------------------- |
| Default compaction entry transform               | yes | yes (hard-coded in `session/context.rs`)                       | **Parity** (behavior)                       |
| Configurable `entryTransforms`                   | yes | no                                                             | **Missing in elph**                         |
| `entryProjectors` for custom entries             | yes | no (custom _messages_ yes; generic custom entry projectors no) | **Missing in elph**                         |
| Custom metadata on JSONL session header (#6417)  | yes | different storage model                                        | **Partial** / **N/A** for session_dir+Turso |
| Export in-memory + JSONL storage types (#6435)   | yes | memory + session_dir + Turso                                   | **Parity+** (more backends)                 |
| Short entry ids from uuid random tail (#6242)    | yes | TSID / backend-specific ids                                    | **N/A**                                     |
| Normalize null message content on ingest (#6343) | yes | typed ingest                                                   | **Partial**                                 |

---

## Compaction estimation detail

| Behavior                                                                                             | pi                                     | elph                                | Status              |
| ---------------------------------------------------------------------------------------------------- | -------------------------------------- | ----------------------------------- | ------------------- |
| Prefer last non-error assistant usage                                                                | yes                                    | yes                                 | **Parity**          |
| Add trailing message estimates after that usage                                                      | yes                                    | yes                                 | **Parity**          |
| Invalidate usage when a newer prefix message (e.g. compaction summary) is newer by timestamp (#6464) | yes (`estimate.ts` / agent compaction) | reverse walk without timestamp gate | **Missing in elph** |
| Count tools introduced via `addedToolNames` after usage anchor                                       | yes                                    | no field / no logic                 | **Missing in elph** |

Agent-side estimation is **ahead** of `elph-ai`’s naive `utils/estimate.rs`, but still behind full pi semantics.

---

## What exists only in elph (not a port gap)

These are product layers. Track them as features of Elph, not as “pi-agent missing work.”

| Module / capability                          | Purpose (short)                         |
| -------------------------------------------- | --------------------------------------- |
| `goals/`                                     | Multi-step goal runtime + tools         |
| `mcp/`                                       | MCP server probe/registry → agent tools |
| `subagent/`                                  | Nested agent graph / harness            |
| `plugins/` / extensions                      | WASM / manifest extensions              |
| `tools/`                                     | Built-in coding & web tools             |
| `mode/`                                      | Plan / collaboration mode policy        |
| `sandbox/`                                   | Execution sandbox hooks                 |
| `datastore/`, `migration/`, `init/`          | App bootstrap & DB lifecycle            |
| Session backends: **session_dir**, **Turso** | Beyond pi JSONL/memory                  |
| Hyper-related flows (via `elph-ai`)          | Extra provider                          |

Upstream coding-agent / other packages may have _related_ features; they are **not** part of `pi-agent-core`.

---

## What exists only in pi-agent (port gaps or N/A)

| Item                                                | Priority         | Notes                                                            |
| --------------------------------------------------- | ---------------- | ---------------------------------------------------------------- |
| `addedToolNames` on tool results + loop propagation | **P0**           | Blocked on / co-ship with `elph-ai` deferred tools               |
| `ThinkingLevel` / harness `max`                     | **P0**           | Co-ship with `elph-ai`                                           |
| Session `entryTransforms` / `entryProjectors`       | **P2**           | Needed for apps that inject context without forking session code |
| JSONL v3 header `metadata`                          | **P2** / **N/A** | Only if interop with pi coding-agent JSONL is required           |
| Node WSL `bash.exe` stdin script fix (#5893)        | **N/A** / low    | Platform-specific; elph local env differs                        |
| `pi-agent-core/base` entry (removed 0.80.0)         | **Skip**         | Do not port                                                      |

---

## Prioritized gaps (port worklist)

### P0 — do first (usually with `elph-ai`)

1. **`AgentThinkingLevel::Max`** + harness set/get + session thinking_level_change values.
2. **`AgentToolResult.added_tool_names`** + agent_loop → `Message::ToolResult` propagation.
3. Ensure convert-to-LLM / context building preserves `added_tool_names` once `elph-ai` supports it.

### P1 — correctness

4. Compaction / context estimate: **timestamp-aware** last-usage selection (#6464).
5. Confirm split-turn summary **serialization** for single-concurrency providers (#5536) with a test.
6. Re-verify shell timeout validation and session label newline normalize if those code paths matter to your apps.

### P2 — harness extensibility

7. Port `SessionContextBuildOptions`-style **entry transforms** and **custom entry projectors**.
8. Optional JSONL metadata parity if session file interop is a goal.

### Product (not pi-agent sync)

9. Continue Elph-only modules (goals, MCP, subagent, tools) under their own docs — they are not measured against `packages/agent`.

---

## Dependency note

Several agent gaps are **blocked by `elph-ai`**:

| Agent need                   | Requires in `elph-ai`                                                |
| ---------------------------- | -------------------------------------------------------------------- |
| Thinking `max`               | `ThinkingLevel::Max`, provider maps, catalog                         |
| Deferred tools               | `added_tool_names` on messages, deferred split, provider native load |
| Accurate cost in harness UIs | `ModelCost.tiers` + `calculate_cost`                                 |
| Fresh models in harness      | Regenerated `models/*.json`                                          |

Always re-audit [pi-ai.md](./pi-ai.md) in the same pass as this file when syncing mainstream.

---

## How to re-audit this package

```bash
cd /path/to/pi && git pull && git rev-parse --short HEAD
head -80 packages/agent/CHANGELOG.md

# Focus files
# - packages/agent/src/types.ts          (AgentToolResult, ThinkingLevel)
# - packages/agent/src/agent-loop.ts     (length stop, addedToolNames, terminate)
# - packages/agent/src/agent.ts          (prepareNextTurn*)
# - packages/agent/src/harness/session/session.ts  (transforms, projectors)
# - packages/agent/src/harness/compaction/*
```

Compare against:

- `crates/elph-agent/src/types/`
- `crates/elph-agent/src/agent_loop/`
- `crates/elph-agent/src/session/context.rs`
- `crates/elph-agent/src/compaction/`

Then update: **Last audited**, **Audit log**, and status cells.
