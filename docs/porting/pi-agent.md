# Porting status: pi-agent → elph-agent

**Last audited:** 2026-07-29T20:00:00Z
**Upstream:** `@earendil-works/pi-agent-core` · `packages/agent` · **v0.82.1** + Unreleased
**Upstream commit:** `cee5ff75`
**Elph crate:** `crates/elph-agent`
**Depends on:** `elph-ai` — see [pi-ai.md](./pi-ai.md)

---

## At a glance (post Sprint 5)

- Core agent + agent loop — **[Parity]**
- `AgentThinkingLevel::Max` — **[Parity]**
- `added_tool_names` on tool results + loop — **[Parity]**
- Session entry transforms / projectors — **[Parity]**
- Compaction estimate timestamp gate (#6464) — **[Parity]**
- Usage metadata on tool results — **[Parity]** (Sprint 5: `AgentToolResult.usage` + `Message::ToolResult` propagation)
- Goals / MCP / subagent / plugins / tools — **[Elph delta]** (product modules; not pi-agent gaps)

---

## Timeline

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

---

## Remaining / watch

- **[P2]** Split-turn summary serialization regression (#5536) — confirm coverage; elph already runs history then turn-prefix summaries sequentially in `compaction/compact.rs`.
- **[P2 / N/A]** JSONL v3 header custom `metadata` — only if interop with pi coding-agent JSONL is required (elph uses session_dir layout).
- **[Gap P1]** `AgentHarnessTool` + `toolContext` — pi v0.82.0 replaced `ExecutionEnv` with application-defined tool contexts. Elph-agent still uses `LocalExecutionEnv`; product tools need convergence.
- **[Gap P1]** `SessionStorage` API v2 — pi v0.81.0 broke the interface with `getPathToRootOrCompaction()`, cursor-based reads, and checkpoint tails. Elph `session/` module API predates this.
- **[Gap P1]** `AgentHarnessTool` context-aware tools (`read`/`write`/`edit`/`bash`) — pi ships these as library; elph-agent has product-level equivalents in `src/tools/`.
- **[Gap P2]** Retry policy + lifecycle events for compaction/branch-summary (pi v0.81.1).
- **[Gap P2]** Fresh routing session IDs for compaction with cache disabled (pi v0.82.0).
- Product modules (goals, MCP, subagent, tools, …) — Elph-only; not pi-agent gaps.

---

## Elph-only (not port gaps)

Modules under `elph-agent` that pi-agent-core does not ship as library surface:

`goals/`, `agent/subagent/`, `plugins/`, `tools/` (incl. `tools/mcp/`), `collaboration/`, `datastore/`, session_dir + Turso backends, `prompt/encoding/` (TOON), richer harness wiring for product hosts.
