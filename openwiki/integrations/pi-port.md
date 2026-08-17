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
- **Package version:** `v0.82.1` (released 2026-07-25) + Unreleased (v0.84.1 partial)
- **Last audit:** 2026-08-06T18:14:06Z
- **Elph HEAD:** `53dcd0c` — update provider model catalog metadata

## Crate Mapping

| pi TypeScript Package                                       | Elph Rust Crate                  | Status                   |
| ----------------------------------------------------------- | -------------------------------- | ------------------------ |
| `@earendil-works/pi-ai` (`packages/ai`)                     | `crates/elph-ai`                 | [Parity] (post Sprint 5) |
| `@earendil-works/pi-agent-core` (`packages/agent`)          | `crates/elph-agent`              | [Parity] (core)          |
| `@earendil-works/pi-coding-agent` (`packages/coding-agent`) | `crates/coding-agent/` (product) | [Partial]                |

## Structural Changes Since Last Audit

| Change                                                                     | Commit               | Details                                                                                                                                                            |
| -------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `elph/` → `crates/coding-agent/`                                           | `dc726d9`            | Product binary moved into workspace crates/ directory                                                                                                              |
| `elph-exec` merged into `elph-agent`                                       | `c8f65ab`            | Now `crate::exec` with `PtySize`, `open_pty()`, `exec_shell_command()`                                                                                             |
| `elph-db` extracted                                                        | `431cee2`            | Shared Turso open/connect/retry helpers from `elph-agent`                                                                                                          |
| `elph-db` **removed**                                                      | `eba87a7`            | Absorbed into `elph-agent/src/datastore/conn.rs`; shared `database` handle passed via `SubagentBootstrap`                                                          |
| `rendown` crate added                                                      | —                    | Streaming markdown renderer, excluded from workspace                                                                                                               |
| `ext-hello` moved to `crates/ext-hello/`                                   | `f12fba4`            | WASM extension example                                                                                                                                             |
| `tools` CLI subcommand removed                                             | `ddf8919`            | Tools are now listed via `list_available_tools` agent tool                                                                                                         |
| Model catalogs as compressed JSON                                          | `85069b1`            | Replaced generated Rust code with embedded `models/*.zstd`                                                                                                         |
| `prompt-templates` no deps                                                 | `ebcd802`            | `minijinja` dependency removed from `elph-agent` (moved to product)                                                                                                |
| `rmcp` v3.0.0                                                              | `5b658eb`            | Upgraded MCP client library with OAuth/client lifecycle changes                                                                                                    |
| TursoSessionStorage                                                        | `e225323`            | Replaced `SessionDirStorage` with pi-aligned Turso-backed schema                                                                                                   |
| Compaction threshold_pct                                                   | `6484604`            | `CompactionSettings.threshold_pct` replaces `threshold` field                                                                                                      |
| `compact()` takes `CompactionPreparation`                                  | `4cedf40`            | Simplified compact signature, preparation done upstream                                                                                                            |
| Crawlberg replaces Obscura                                                 | `0b2b522`            | Browser backend for web tools                                                                                                                                      |
| `env:` credential prefix                                                   | `f85a127`            | Store plaintext env var references for provider credentials                                                                                                        |
| OAuth: Kimi, OpenRouter, Radius                                            | `ec33716`            | New OAuth providers added                                                                                                                                          |
| Providers: Infron, Baseten, etc.                                           | `a5befd8`            | New provider adapters (see [Providers](../domains/providers.md))                                                                                                   |
| `elph-db` removed, absorbed into `elph-agent`                              | `eba87a7`            | Open/connect/retry/lock-error/WAL recovery helpers moved to `crates/elph-agent/src/datastore/conn.rs`. Subagent shares database handle via `Arc<turso::Database>`. |
| `turso` 0.8.0-pre.3                                                        | `a25ed48`            | Upgraded from 0.7.2                                                                                                                                                |
| `metal` feature for macOS GPU                                              | `3f15161`            | `crates/coding-agent/` forwards to `floppy/metal`; auto-detected on Apple Silicon                                                                                  |
| `dist` build profile                                                       | `b315e28`            | `make install PROFILE=dist` → `elph` binary                                                                                                                        |
| Subagent output durability                                                 | `951fea9`            | `SubagentOutput`, `TurnGuard`, `wait_agent_for_output()`, `output.md`/`events.jsonl`/`meta.json` persist                                                           |
| OpenAI compat gaps (finish_reason, sampling_params, thinking_token_budget) | `f398e03`            | Ported to v0.84.1 parity                                                                                                                                           |
| `/compact` options                                                         | `e5144fa`            | `--threshold`, `--keep-recent`, `--model`, `--memory-flush`                                                                                                        |
| TUI env var model resolution                                               | `3c5aca0`            | `ELPH_PROVIDER`/`ELPH_MODEL` now properly override last-used model                                                                                                 |
| Steering prompt prefix exports                                             | `457464e`            | `CONTINUATION_PROMPT_PREFIX`, `BUDGET_LIMIT_PROMPT_PREFIX` exposed for TUI                                                                                         |
| Codegraph include/exclude patterns                                         | `d85a84a`            | File include/exclude patterns for codegraph indexing                                                                                                               |
| Turso v0.8.0-pre.3 upgrade                                                 | `a25ed48`            | Updated from v0.7.2                                                                                                                                                |
| MCP registry split into `mod.rs` + `discovery.rs` + `bridge.rs`            | `45c8e6e`            | Refactored monolithic `registry.rs`                                                                                                                                |
| `finish_reason` + `thinking_token_budget` for OpenAI compat                | `f398e03`            | `supports_finish_reason`, `supports_thinking_token_budget`, `sampling_params` on `OpenAICompletionsCompat`                                                         |
| `BeforeToolCallResult.terminate`                                           | `f398e03`            | Early termination hint on blocked tool calls                                                                                                                       |
| `/compact` slash command options                                           | `e5144fa`            | `--threshold`, `--keep-recent`, `--model`, `--memory-flush`                                                                                                        |
| Subagent output durability + `TurnGuard`                                   | `951fea9`, `4baf7aa` | Persistent artifacts, wait-for-output, RAII turn guard                                                                                                             |
| `coding-agent/metal` feature                                               | `3f15161`            | macOS GPU acceleration for floppy embeddings                                                                                                                       |
| `resolve_boot_model` env var fix                                           | `3c5aca0`            | `ELPH_PROVIDER`/`ELPH_MODEL` now properly resolved                                                                                                                 |
| Steering prompt prefixes public export                                     | `457464e`            | `BUDGET_LIMIT_PROMPT_PREFIX`, `CONTINUATION_PROMPT_PREFIX`                                                                                                         |
| `dist` build profile                                                       | `b315e28`            | New Makefile profile for distribution builds                                                                                                                       |
| Codegraph exclude patterns removed                                         | `4c16523`            | `exclude_patterns` removed from codegraph settings                                                                                                                 |
| Embed batch size 64→128                                                    | `4ce11f3`            | Default batch size increased                                                                                                                                       |
| Floppy connection pool deadlock fix                                        | `3e3f35f`            | Guard against zero connection pool permits                                                                                                                         |

## Crate Mapping

| pi TypeScript Package                                       | Elph Rust Crate                  | Status                   |
| ----------------------------------------------------------- | -------------------------------- | ------------------------ |
| `@earendil-works/pi-ai` (`packages/ai`)                     | `crates/elph-ai`                 | [Parity] (post Sprint 5) |
| `@earendil-works/pi-agent-core` (`packages/agent`)          | `crates/elph-agent`              | [Parity] (core)          |
| `@earendil-works/pi-coding-agent` (`packages/coding-agent`) | `crates/coding-agent/` (product) | [Partial]                |

## Parity Status Overview

### elph-ai ↔ pi-ai

**Status:** [Parity] through v0.84.1

| Feature                                         | Status       | Details                                                                    |
| ----------------------------------------------- | ------------ | -------------------------------------------------------------------------- |
| Architecture (Models, providers, auth, streams) | [Parity]     | Core abstractions ported                                                   |
| Model catalogs (GPT-5.6, tiers, max maps)       | [Parity]     | Hyper is Elph-only; compressed JSON                                        |
| Thinking levels + `max`                         | [Parity]     | `ThinkingLevel::Max` support                                               |
| Deferred / dynamic tools                        | [Parity]     | `Message::ToolResult.added_tool_names`                                     |
| Cost accounting tiers                           | [Parity]     | `ModelCostTier`, `calculate_cost()`                                        |
| Bedrock bearer from `api_key`                   | [Parity]     | Sprint 3                                                                   |
| Empty thinking + signature                      | [Parity]     | Sprint 3                                                                   |
| Context estimate + compaction boundary          | [Parity]     | Sprint 3                                                                   |
| Diagnostics + session resource cleanup          | [Parity]     | Sprint 4                                                                   |
| `contentText` utility                           | [Parity]     | Sprint 5                                                                   |
| `CredentialStore.list()`                        | [Parity]     | Sprint 5                                                                   |
| `ModelsStore` + `etag` support                  | [Parity]     | Sprint 5                                                                   |
| `ConstrainedSampling` + compat flags            | [Parity]     | Sprint 5                                                                   |
| `SessionAffinityFormat`                         | [Parity]     | Sprint 5                                                                   |
| `ANTHROPIC_AUTH_TOKEN` bearer                   | [Parity]     | Sprint 5                                                                   |
| Retry patterns (DNS, gRPC, socket-drop, HTTP/2) | [Parity]     | Sprint 5                                                                   |
| `sampling_params` passthrough                   | [Parity]     | Sprint 8 (commit `f398e03`) — model-level defaults + per-request overrides |
| `thinking_token_budget` compat flag             | [Parity]     | Sprint 8 (commit `f398e03`) — vLLM-style reasoning cap                     |
| `supports_finish_reason` inference              | [Parity]     | Sprint 8 (commit `f398e03`) — opt-out, infers ToolUse vs Stop              |
| Infron provider                                 | [Elph delta] | Not in pi                                                                  |
| Baseten, Ollama Cloud, TokenRouter, OpenGateway | [Elph delta] | New provider adapters                                                      |

### elph-agent ↔ pi-agent-core

**Status:** [Parity] on core through v0.84.1; [Elph delta] on product modules

| Feature                               | Status       | Details                                                    |
| ------------------------------------- | ------------ | ---------------------------------------------------------- |
| Core agent + agent loop               | [Parity]     |                                                            |
| `AgentThinkingLevel::Max`             | [Parity]     |                                                            |
| `added_tool_names` on tool results    | [Parity]     |                                                            |
| Session entry transforms / projectors | [Parity]     |                                                            |
| Compaction estimate timestamp gate    | [Parity]     | `threshold_pct` replaces `threshold`                       |
| Usage metadata on tool results        | [Parity]     | Sprint 5                                                   |
| Shell execution (was `elph-exec`)     | [Parity]     | Merged as `crate::exec`                                    |
| `BeforeToolCallResult.terminate`      | [Parity]     | Sprint 8 (commit `f398e03`) — batch early-termination hint |
| `Agent.reset()` idle-guard            | [Parity]     | Sprint 8 — returns `Result`; bails if run in flight        |
| pi-agent v4 lane-based session model  | [Gap]        | [Architectural] — not ported; see lane model gap below     |
| TOON prompt encoding                  | [Elph delta] | Not in pi-agent-core                                       |
| Goals, MCP, subagent, plugins, tools  | [Elph delta] | Product modules not in pi-agent-core                       |

### elph (product) ↔ pi-coding-agent

**Status:** [Partial]

| Feature                        | Status       | Details                                                                     |
| ------------------------------ | ------------ | --------------------------------------------------------------------------- |
| Module layout / product intent | [Partial]    | `crates/coding-agent/src/agent/` is the declared pi-coding-agent equivalent |
| Session orchestration          | [Partial]    | `CodingAgentSession`, wiring exist                                          |
| Interactive TUI                | [Partial]    | Shell/TUI wired; overlays stubbed                                           |
| Print / non-interactive mode   | [Partial]    | `elph run` exists                                                           |
| Built-in tools                 | [Parity]     | Via `elph-agent` tools + Elph extras                                        |
| Extensions                     | [Partial]    | WASM Component Model (pi: JS/TS host)                                       |
| Skills + prompt templates      | [Partial]    | Load paths in agent crate                                                   |
| RPC / JSON automation          | [Gap]        | Elph has ACP instead ([Elph delta])                                         |
| Public SDK                     | [Gap]        | Library = crates, not pi-compatible SDK                                     |
| Export HTML / share gist       | [Gap]        | CLI export stub                                                             |
| Memory / codegraph / server    | [Elph delta] | Elph-only features                                                          |

## Known Gaps

From `docs/porting/pi-agent.md`, `docs/porting/pi-ai.md`, and recent git history:

| Gap                                       | Priority | Status   | Description                                                                                                                                                 |
| ----------------------------------------- | -------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentHarnessTool` + `toolContext`        | P1       | [Open]   | pi v0.82.0 replaced `ExecutionEnv` with app-defined tool contexts                                                                                           |
| `SessionStorage` API v2                   | P1       | [Open]   | pi v0.81.0 broke interface with cursor-based reads                                                                                                          |
| `AgentHarnessTool` context-aware tools    | P1       | [Open]   | pi ships read/write/edit/bash as library                                                                                                                    |
| Retry policy for compaction               | P2       | [Closed] | `compaction_ops.rs` added retry (commit `4cedf40`)                                                                                                          |
| Fresh routing session IDs for compaction  | P2       | [Open]   | pi v0.82.0                                                                                                                                                  |
| Split-turn summary serialization          | P2       | [Closed] | `compact.rs` handles split-turn via `turn_prefix_messages`                                                                                                  |
| JSONL v3 header metadata                  | P2/N/A   | [Open]   | Only if cross-compat needed                                                                                                                                 |
| `finish_reason` + `thinking_token_budget` | P2       | [Closed] | `f398e03` — OpenAI compat gaps ported to v0.84.1                                                                                                            |
| `sampling_params` merge                   | P2       | [Closed] | `f398e03` — model-level defaults + per-request overrides                                                                                                    |
| `BeforeToolCallResult.terminate`          | P2       | [Closed] | `f398e03` — before-hook path feeds batch early-termination                                                                                                  |
| `Agent.reset()` idle-guard                | P2       | [Closed] | Sprint 8 — `Result` return; bails if run in flight                                                                                                          |
| pi-agent v4 lane-based session model      | P1       | [Open]   | [Architectural] — pi v0.84.0 rewrites session layer with lanes, durable operation records, shared sequence numbers. Deferred pending architecture decision. |
| Headless mode parity                      | P2       | [Closed] | `0c73d8d` — `elph run` with `--output=plain/pretty/json/stream-json/stream-message-json`                                                                    |
| Session persistence (turns, todos, GC)    | P2       | [Closed] | `2ce555b` — `TurnStore`, `TodoStore`, `RetentionPolicy`, `run_session_gc()`                                                                                 |
| Handover (Claude + Codex)                 | P2       | [Closed] | `92c17da`, `601eebf` — inert transcript, bounded reads, safety boundary                                                                                     |
| Subagent output durability                | P2       | [Closed] | `951fea9` — `SubagentOutput`, `TurnGuard`, `wait_agent_for_output()`                                                                                        |

## Elph-Only Extensions

These features exist in Elph but not in pi:

- **Infron, Baseten, Ollama Cloud, TokenRouter, OpenGateway providers** — new provider adapters
- **Memory system** (`crates/coding-agent/src/memory/`) — floppy/Turso-backed vector memory
- **Codegraph** (`crates/coding-agent/src/codegraph/`) — semantic code index and impact graph
- **ACP** — Agent Client Protocol (alternative to pi RPC)
- **WASM extensions** — `extensions/` + `crates/elph-agent/src/plugins/` (WASM Component Model)
- **Swarm** — `crates/elph-swarm/` (multi-agent orchestration, skeleton)
- **Cron** — `crates/elph-cron/` (scheduled tasks, skeleton)
- **Sandbox** — `crates/elph-sandbox/` (zerobox-powered, skeleton)
- **TOON prompt encoding** — `crates/elph-agent/src/prompt/` (string encoding for prompts)
- **Collaboration tools** — `crates/elph-agent/src/collaboration/`
- **Subagent persistent output** — `SubagentOutput`, `TurnGuard`, `wait_agent_for_output()`, `subagent_persist_event()`
- **`rendown`** — `crates/rendown/` (streaming markdown renderer)
- **`web_extract` tool** — structured DOM data extraction via `htmd` and `astral-tl`
- **`metal` feature** — macOS GPU acceleration for floppy embeddings (codegraph + memory)

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
6. After porting, regenerate model catalogs with `make generate-models` (reads from `../../earendil-works/pi/packages/ai`).

## Source References

- `docs/porting/README.md` — porting overview, timeline, sync workflow
- `docs/porting/pi-ai.md` — pi-ai ↔ elph-ai status (v0.84.1)
- `docs/porting/pi-agent.md` — pi-agent ↔ elph-agent status (v0.84.1, includes lane model gap analysis)
- `docs/porting/pi-coding-agent.md` — pi-coding-agent ↔ elph product status
- `crates/elph-ai/src/providers/builtin.rs` — Hyper provider (Elph delta)
- `crates/elph-agent/src/agent/subagent/` — subagent orchestration (Elph delta)
- `crates/elph-agent/src/tools/mcp/` — MCP integration (Elph delta)
