# Porting status: pi-agent → elph-agent

**Last audited:** 2026-08-07T14:00:00Z
**Upstream:** `@earendil-works/pi-agent-core` · `packages/agent` · **v0.84.1** + Unreleased
**Upstream commit:** `7aca0d7b3`
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
- `BeforeToolCallResult.terminate` batch early-termination hint (before-hook path) — **[Parity]**
- `Agent.reset()` idle-guard — **[Parity]** (Result return; bails if a run is in flight)
- Goals / MCP / subagent / tools — **[Elph delta]** (product modules; not pi-agent gaps)

---

## Timeline

### 2026-08-07 @ `7aca0d7b3` (v0.84.1 + Unreleased)

**Sprint 8: P2 gap port — 2 feature areas (additive, no architecture change).**

Doctrine applied: port only safe, additive gaps that complement the existing Elph tree-entry `SessionStorage` design; do not restructure it.

- **`BeforeToolCallResult.terminate` (P2)** — pi v0.84.1 #7715. `crates/elph-agent/src/runtime/loop_config.rs`: new `terminate: Option<bool>` field on `BeforeToolCallResult`. `crates/elph-agent/src/runtime/exec/prepare.rs`: a blocked `beforeToolCall` result now propagates `terminate` into the `AgentToolResult.terminate`, so it participates in the existing `should_terminate_tool_batch` batch-early-termination rule (the after-hook path was already ported in Sprint 6).
- **`Agent.reset()` idle-guard (P2)** — pi v0.84.1 #7717. `crates/elph-agent/src/agent/mod.rs`: `reset()` now returns `Result<(), anyhow::Error>` and bails with `"Agent is already processing. Wait for completion before resetting."` when an `activeRun` is in flight, instead of clearing transcript and runtime state mid-run.

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
- **[P2] (done in Sprint 8)** `BeforeToolCallResult.terminate` before-hook path — ported; feeds `should_terminate_tool_batch`.
- **[P2] (done in Sprint 8)** `Agent.reset()` idle-guard — ported as a `Result` return.
- **[P2]** `shouldStopAfterTurn` callback (pi v0.84.0 #7367) — graceful stop after a completed turn before queued messages / another model call. Not ported; small and self-contained if product needs it.
- **[P2]** `Events` harness subscription interface (pi `harness/events.ts`, `on(type, listener)`) — Elph already has `subscribe_agent_events` + `HarnessHooks::on(event_type, listener)`, so intent is **[Parity]** despite the different shape.
- **[P2]** `AgentHarness` v2 scaffold + `HarnessNotImplemented` (pi v0.84.0) — compile-complete scaffold where unfinished ops reject. Elph's harness is fully implemented; not needed.
- **[P2]** Harness telemetry schemas via `@earendil-works/pi-telemetry` (pi v0.84.0) — vendor-specific external crate; Elph has no telemetry stack. **[N/A]** unless telemetry is adopted.
- **[P2]** `prepareNextTurn` with `AgentLoopTurnUpdate` — pi v0.84.0 extends this to return replacement context/model/thinking state. Elph already has `prepare_next_turn` returning `AgentLoopTurnUpdate` **[Parity]**; watch for signature drift.
- Product modules (goals, MCP, subagent, tools, …) — Elph-only; not pi-agent gaps.

### pi-agent v4 session/repo model — **[Gap P1 architectural, not ported]**

pi v0.84.0 rewrites the session layer around a **lane-based** model (`packages/agent/src/harness/session/`). This is a ground-up architectural change, not a safe additive port, and it would reshape Elph's existing tree-entry `SessionStorage` design (`crates/elph-agent/src/session/`). Deferred pending an explicit architecture decision.

**What pi ships** (`types.ts`, `session.ts`, `state.ts`, `memory.ts`, `jsonl/{repo,storage,codec,errors}.ts`, `jsonl/search.ts`, `reducer.ts`, `telemetry.ts`, `testing/conformance.ts`):

- Lane-based `Session` with shared sequence numbers, tree-scoped `SessionTree` views, and durable `LaneRecord` operation records (operation_started/finished, step_attempt, tool_started, queue_enqueued/cancelled, write_deferred, usage).
- `SessionStorage` v4 contract: `getLanes`/`createLane`/`moveLane`, `appendEntry`/`appendRecord`, `findEntriesOnBranch` (mandatory `start`), `findRecords` with `RecordQuery.operationKind`, `findOpenOperations(lane, {limit})` recovery, `getLog({afterSeq, limit})`, plus global facts (`getName`/`setName`/`getLabel`/`setLabel`).
- `SessionRepo` contract (`create`/`open`/`list`/`delete`/`fork`) with `JsonlSessionRepo` (append-only, metadata validation, shared storage, atomic publication) and `InMemorySessionRepo`.
- `FileSystem.renameFile()` now **required** for atomic JSONL publication.
- `SessionSearch` interface + scanning implementation; SQLite FTS5 search index factory (separate `feat/search` commits).
- 993-line conformance test suite validating the `SessionRepo`/`SessionStorage` contract.

**What Elph has today:** a simpler tree-entry model (`SessionTreeEntry` enum) + `SessionStorage` trait v2 (ported Sprint 6) with `get_path_to_root_or_compaction`, cursor-based reads, checkpoint tails, labels, and stats — but **no lanes, no durable operation records, no shared sequence, no `appendRecord`/`log`, no branch-scoped queries, no open-operation recovery**. Elph's `SessionRepo` (`InMemory` + `Turso`) is already product-shaped and not a literal port of pi's contract.

**Implication:** future pi features (telemetry, search, conformance) build on the v4 model. Porting them onto Elph's tree-entry design requires either a selective port of behaviors or a deliberate architecture decision to adopt lanes.

---

## Elph-only (not port gaps)

Modules under `elph-agent` that pi-agent-core does not ship as library surface:

`goals/`, `agent/subagent/`, `tools/` (incl. `tools/mcp/`), `collaboration/`, `datastore/`, session_dir + Turso backends, `prompt/encoding/` (TOON), richer harness wiring for product hosts. Native command hooks are owned by `crates/coding-agent`, not this crate.
