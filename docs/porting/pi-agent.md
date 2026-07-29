# Porting status: pi-agent → elph-agent

**Last audited:** 2026-07-29T20:00:00Z
**Upstream:** `@earendil-works/pi-agent-core` · `packages/agent` · **v0.82.1** + Unreleased
**Upstream commit:** `cee5ff75`
**Elph crate:** `crates/elph-agent`
**Depends on:** `elph-ai` — see [pi-ai.md](./pi-ai.md)

---

## At a glance (post Sprint 6)

- Core agent + agent loop — **[Parity]**
- `AgentThinkingLevel::Max` — **[Parity]**
- `added_tool_names` on tool results + loop — **[Parity]**
- Session entry transforms / projectors — **[Parity]**
- Compaction estimate timestamp gate (#6464) — **[Parity]**
- Usage metadata on tool results — **[Parity]**
- `AgentHarnessTool` + `toolContext` replacing `ExecutionEnv` — **[Parity]**
- `SessionStorage` API v2 with cursor/checkpoint/stats — **[Parity]**
- Compaction retry policy + lifecycle events — **[Parity]**
- Fresh routing session IDs for compaction — **[Parity]**
- Goals / MCP / subagent / plugins / tools — **[Elph delta]** (product modules; not pi-agent gaps)

---

## Timeline

### 2026-07-29 @ `cced6a21` (v0.82.1 + Unreleased)

**Sprint 6: P1/P2 gap port — 4 feature areas.**

- **`AgentHarnessTool` + `toolContext`** — pi v0.82.0 breaking change: `ToolContext` struct in `tools/types.rs` carries `Arc<LocalExecutionEnv>`, `cwd`, `is_plan_mode`. `ToolExecuteFn` signature extended. `context_aware_tool()` helper. Threaded through `AgentLoopConfig`, `execute_prepared_tool_call`, and all dispatch paths. `shell_exec` migrated to context-aware execution; other tools can migrate incrementally.
- **`SessionStorage` API v2** — pi v0.81.0 breaking change: `get_path_to_root_or_compaction()`, `get_entries_cursor()`, `get_statistics()`, `store_checkpoint_tail()`, `load_checkpoint_tail()`, `list_checkpoint_tails()`, `get_name()`. All three backends (InMemory, SessionDir, Turso) updated. `Session::branch_or_compaction()`, `entries_cursor()`, `statistics()`, `store_checkpoint()`, `load_checkpoint()`, `checkpoint_tails()` passthrough methods.
- **Compaction retry lifecycle** — pi v0.81.1: `compact_with_retry()` in `compaction_ops.rs` with exponential backoff (1s, 2s, 4s, max 3 retries). `CompactionRetryEvent` enum (Attempt/Retry/Recovered/Failed) on `AgentHarnessOwnEvent`.
- **Fresh routing session IDs for compaction** — pi v0.82.0: `CheckpointTail` mechanism stores compaction checkpoints with root IDs, enabling cursor-based reads from compaction boundaries.

### 2026-07-29 @ `cee5ff75` (v0.82.1 + Unreleased)

**Usage metadata plumbing.** Sprint 5 added `AgentToolResult.usage` (`tools/types.rs`) and wired propagation from agent tool results into `Message::ToolResult.usage` (`runtime/exec/messages.rs`). The `AgentToolResult::text()` / `::error()` constructors default to `usage: None`; callers can attach usage via `with_usage()`.

Also added constrained sampling config to all built-in `elph_ai::Tool` constructors across the codebase.

### 2026-07-29 @ `4c18610` (v0.80.6 + Unreleased)

**Test fix:** Two integration tests used `get_model("openai", "gpt-4o-mini")` which no longer resolves after `elph-ai` catalog restructure (see [pi-ai.md](./pi-ai.md#timeline)). Updated to `get_models(None).next()`.

### 2026-07-11T11:23:28Z @ `4c18610` (v0.80.6 + Unreleased)

**Sprints 1–4:** Max thinking, deferred tool names, session transforms, estimate gate.

### 2026-07-11T11:12:19Z @ `4c18610` (v0.80.6 + Unreleased)

Initial gap audit.

---

## What landed

- `AgentThinkingLevel::Max` — `src/types/enums.rs`, harness helpers
- `AgentToolResult.added_tool_names` — `src/tools/types.rs`
- `AgentToolResult.usage` — `src/tools/types.rs` (Sprint 5)
- Loop → `Message::ToolResult` propagation — `src/runtime/exec/messages.rs` (includes `usage` field)
- After-tool / harness patches — `src/runtime/loop_config.rs`, `src/runtime/exec/execute.rs`, `ToolResultPatch`
- `SessionContextBuildOptions` — `src/session/context.rs`
- `entry_transforms` / `entry_projectors` — `build_session_context_with_options`, `Session::build_context_with_options`
- Timestamp-aware last usage — `src/compaction/estimation.rs`
- `ToolContext` + `AgentHarnessTool` — `src/tools/types.rs` (Sprint 6)
- `SessionStorage` API v2 — `src/session/types.rs` + backends (Sprint 6)
- `compact_with_retry()` — `src/agent/harness/compaction_ops.rs` (Sprint 6)
- `CompactionRetryEvent` — `src/agent/harness/types/events.rs` (Sprint 6)
- `CheckpointTail` mechanism — `src/session/types.rs` + `storage_utils.rs` (Sprint 6)

---

## Remaining / watch

- **[P2]** Split-turn summary serialization regression (#5536) — confirm coverage; elph already runs history then turn-prefix summaries sequentially in `compaction/compact.rs`.
- **[P2 / N/A]** JSONL v3 header custom `metadata` — only if interop with pi coding-agent JSONL is required (elph uses session_dir layout).
- **[P2]** `AgentHarnessTool` migration for remaining tools — `shell_exec` migrated; `read_file`, `edit_file`, `write_file`, `grep`, `find_path`, `list_dir`, `copy_path`, `create_dir`, `delete_path`, `move_path`, `web_*` still use `simple_tool` wrapper (compatible via `_context` parameter).
- Product modules (goals, MCP, subagent, tools, …) — Elph-only; not pi-agent gaps.

---

## Elph-only (not port gaps)

Modules under `elph-agent` that pi-agent-core does not ship as library surface:

`goals/`, `agent/subagent/`, `plugins/`, `tools/` (incl. `tools/mcp/`), `collaboration/`, `datastore/`, session_dir + Turso backends, `prompt/encoding/` (TOON), richer harness wiring for product hosts.
