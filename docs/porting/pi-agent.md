# Porting status: pi-agent → elph-agent

**Last audited:** 2026-07-11T11:23:28Z
**Upstream:** `@earendil-works/pi-agent-core` · `packages/agent` · **v0.80.6** + Unreleased
**Upstream commit:** `4c18610`
**Elph crate:** `crates/elph-agent`
**Depends on:** `elph-ai` — see [pi-ai.md](./pi-ai.md)

---

## At a glance (post Sprints 1–4)

- Core agent + agent loop — **[Parity]**
- `AgentThinkingLevel::Max` — **[Parity]**
- `added_tool_names` on tool results + loop — **[Parity]**
- Session entry transforms / projectors — **[Parity]**
- Compaction estimate timestamp gate (#6464) — **[Parity]**
- Goals / MCP / subagent / plugins / tools — **[Elph delta]** (product modules; not pi-agent gaps)

---

## Timeline

### 2026-07-11T11:23:28Z @ `4c18610` (v0.80.6 + Unreleased)

**Sprints 1–4:** Max thinking, deferred tool names, session transforms, estimate gate.

### 2026-07-11T11:12:19Z @ `4c18610` (v0.80.6 + Unreleased)

Initial gap audit.

---

## What landed

- `AgentThinkingLevel::Max` — `src/types/enums.rs`, harness helpers
- `AgentToolResult.added_tool_names` — `src/types/tools.rs`
- Loop → `Message::ToolResult` propagation — `src/agent_loop/tools/messages.rs`
- After-tool / harness patches — `loop_config.rs`, `execute.rs`, `ToolResultPatch`
- `SessionContextBuildOptions` — `src/session/context.rs`
- `entry_transforms` / `entry_projectors` — `build_session_context_with_options`, `Session::build_context_with_options`
- Timestamp-aware last usage — `src/compaction/estimation.rs`

---

## Remaining / watch

- **[P2]** Split-turn summary serialization regression (#5536) — confirm coverage; elph already runs history then turn-prefix summaries sequentially in `compaction/compact.rs`.
- **[P2 / N/A]** JSONL v3 header custom `metadata` — only if interop with pi coding-agent JSONL is required (elph uses session_dir layout).
- Product modules (goals, MCP, subagent, tools, …) — Elph-only; not pi-agent gaps.

---

## Elph-only (not port gaps)

Modules under `elph-agent` that pi-agent-core does not ship as library surface:

`goals/`, `mcp/`, `subagent/`, `plugins/`, `tools/`, `mode/`, `sandbox/`, `datastore/`, session_dir + Turso backends, `runtime/prompt_encoding/` (TOON), richer harness wiring for product hosts.
