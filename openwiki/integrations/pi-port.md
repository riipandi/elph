---
type: Concept
title: Pi Port — Upstream Porting from earendil-works/pi
description: Tracking the port from upstream pi TypeScript packages to Rust — crate mapping, parity gaps, and Elph-only extensions
tags: [porting, pi, upstream, parity, gaps, elph-delta]
---

# Pi Port

Elph is a Rust port of the [earendil-works/pi](https://github.com/earendil-works/pi) TypeScript project. This page documents the crate mapping, parity status, and Elph-specific extensions. See [Architecture Overview](../architecture/overview.md) for how the ported crates fit together, and [Source Map](../architecture/source-map.md) for which modules are ports vs Elph-only.

## Reference

- **Upstream commit:** `cee5ff75` (_ref: remove openclaw reference from readme_)
- **Package version:** `v0.82.1` (released 2026-07-25) + Unreleased
- **Last audit:** 2026-07-29T20:00:00Z

## Crate Mapping

| pi TypeScript Package                                       | Elph Rust Crate     | Status                   |
| ----------------------------------------------------------- | ------------------- | ------------------------ |
| `@earendil-works/pi-ai` (`packages/ai`)                     | `crates/elph-ai`    | [Parity] (post Sprint 5) |
| `@earendil-works/pi-agent-core` (`packages/agent`)          | `crates/elph-agent` | [Parity] (core)          |
| `@earendil-works/pi-coding-agent` (`packages/coding-agent`) | `elph/` (product)   | [Partial]                |

## Parity Status Overview

### elph-ai ↔ pi-ai

**Status:** [Parity] through v0.82.1

| Feature                                         | Status       | Details                                |
| ----------------------------------------------- | ------------ | -------------------------------------- |
| Architecture (Models, providers, auth, streams) | [Parity]     | Core abstractions ported               |
| Model catalogs (GPT-5.6, tiers, max maps)       | [Parity]     | Hyper is Elph-only                     |
| Thinking levels + `max`                         | [Parity]     | `ThinkingLevel::Max` support           |
| Deferred / dynamic tools                        | [Parity]     | `Message::ToolResult.added_tool_names` |
| Cost accounting tiers                           | [Parity]     | `ModelCostTier`, `calculate_cost()`    |
| Bedrock bearer from `api_key`                   | [Parity]     | Sprint 3                               |
| Empty thinking + signature                      | [Parity]     | Sprint 3                               |
| Context estimate + compaction boundary          | [Parity]     | Sprint 3                               |
| Diagnostics + session resource cleanup          | [Parity]     | Sprint 4                               |
| `contentText` utility                           | [Parity]     | Sprint 5                               |
| `CredentialStore.list()`                        | [Parity]     | Sprint 5                               |
| `ModelsStore` + `etag` support                  | [Parity]     | Sprint 5                               |
| `ConstrainedSampling` + compat flags            | [Parity]     | Sprint 5                               |
| `SessionAffinityFormat`                         | [Parity]     | Sprint 5                               |
| `ANTHROPIC_AUTH_TOKEN` bearer                   | [Parity]     | Sprint 5                               |
| Retry patterns (DNS, gRPC, socket-drop, HTTP/2) | [Parity]     | Sprint 5                               |
| Hyper provider                                  | [Elph delta] | Not in pi                              |

### elph-agent ↔ pi-agent-core

**Status:** [Parity] on core; [Elph delta] on product modules

| Feature                               | Status       | Details                              |
| ------------------------------------- | ------------ | ------------------------------------ |
| Core agent + agent loop               | [Parity]     |                                      |
| `AgentThinkingLevel::Max`             | [Parity]     |                                      |
| `added_tool_names` on tool results    | [Parity]     |                                      |
| Session entry transforms / projectors | [Parity]     |                                      |
| Compaction estimate timestamp gate    | [Parity]     |                                      |
| Usage metadata on tool results        | [Parity]     | Sprint 5                             |
| Goals, MCP, subagent, plugins, tools  | [Elph delta] | Product modules not in pi-agent-core |

### elph (product) ↔ pi-coding-agent

**Status:** [Partial]

| Feature                        | Status       | Details                                                      |
| ------------------------------ | ------------ | ------------------------------------------------------------ |
| Module layout / product intent | [Partial]    | `elph/src/agent/` is the declared pi-coding-agent equivalent |
| Session orchestration          | [Partial]    | `CodingAgentSession`, wiring exist                           |
| Interactive TUI                | [Partial]    | Shell/TUI wired; overlays stubbed                            |
| Print / non-interactive mode   | [Partial]    | `elph run` exists                                            |
| Built-in tools                 | [Parity]     | Via `elph-agent` tools + Elph extras                         |
| Extensions                     | [Partial]    | WASM Component Model (pi: JS/TS host)                        |
| Skills + prompt templates      | [Partial]    | Load paths in agent crate                                    |
| RPC / JSON automation          | [Gap]        | Elph has ACP instead ([Elph delta])                          |
| Public SDK                     | [Gap]        | Library = crates, not pi-compatible SDK                      |
| Export HTML / share gist       | [Gap]        | CLI export stub                                              |
| Memory / codegraph / server    | [Elph delta] | Elph-only features                                           |

## Known Gaps

From `docs/porting/pi-agent.md`:

| Gap                                      | Priority | Description                                                       |
| ---------------------------------------- | -------- | ----------------------------------------------------------------- |
| `AgentHarnessTool` + `toolContext`       | P1       | pi v0.82.0 replaced `ExecutionEnv` with app-defined tool contexts |
| `SessionStorage` API v2                  | P1       | pi v0.81.0 broke interface with cursor-based reads                |
| `AgentHarnessTool` context-aware tools   | P1       | pi ships read/write/edit/bash as library                          |
| Retry policy for compaction              | P2       | pi v0.81.1                                                        |
| Fresh routing session IDs for compaction | P2       | pi v0.82.0                                                        |
| Split-turn summary serialization         | P2       | Confirm coverage                                                  |
| JSONL v3 header metadata                 | P2/N/A   | Only if cross-compat needed                                       |

## Elph-Only Extensions

These features exist in Elph but not in pi:

- **Hyper provider** (`crates/elph-ai/src/providers/builtin.rs`)
- **Memory system** (`elph/src/memory/`) — floppy/Turso-backed vector memory
- **Codegraph** (`elph/src/codegraph/`) — structural knowledge graph for code reviews
- **ACP** — Agent Client Protocol (alternative to pi RPC)
- **WASM extensions** — `extensions/` + `plugins/` (WASM Component Model)
- **Swarm** — `crates/elph-swarm/` (multi-agent orchestration, skeleton)
- **Cron** — `crates/elph-cron/` (scheduled tasks, skeleton)
- **Sandbox** — `crates/elph-sandbox/` (zerobox-powered, skeleton)
- **TOON prompt encoding** — `crates/elph-agent/src/prompt/encoding/`
- **Collaboration tools** — `crates/elph-agent/src/collaboration/`
- **Datastore** — `crates/elph-agent/src/datastore/` (session_dir + Turso)

## Sync Workflow

From `docs/porting/README.md`:

1. Update the local pi clone.
2. Read upstream changelogs (`packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`).
3. Diff against the timeline.
4. Port + regenerate catalogs:
    ```sh
    cargo run -p elph-ai --bin generate-models -- chat --skip-scripts
    ```
5. Append a timeline entry with ISO timestamp + pi commit/version.

## Source References

- `docs/porting/README.md` — porting overview, timeline, sync workflow
- `docs/porting/pi-ai.md` — pi-ai ↔ elph-ai status
- `docs/porting/pi-agent.md` — pi-agent ↔ elph-agent status
- `docs/porting/pi-coding-agent.md` — pi-coding-agent ↔ elph product status
- `crates/elph-ai/src/providers/builtin.rs` — Hyper provider (Elph delta)
- `crates/elph-agent/src/agent/subagent/` — subagent orchestration (Elph delta)
- `crates/elph-agent/src/tools/mcp/` — MCP integration (Elph delta)
