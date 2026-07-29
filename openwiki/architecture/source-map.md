---
type: Source Map
title: Elph Crate-by-Crate Source Map
description: Module map for every crate in the Elph workspace, noting which modules are pi ports vs elph-only
tags: [source-map, modules, pi-port, elph-only]
---

# Source Map

Crate-by-crate module map with file paths, noting pi port origins vs Elph-only extensions.

## `elph` (product binary + library)

**Path:** `elph/src/`
**Pi mapping:** `@earendil-works/pi-coding-agent` → `elph/`
**Key files:**

| Module                     | Path                                  | Status                                  |
| -------------------------- | ------------------------------------- | --------------------------------------- |
| `main.rs`                  | `elph/src/main.rs`                    | [Elph delta] — CLI entry via clap       |
| `lib.rs`                   | `elph/src/lib.rs`                     | [Elph delta] — re-exports all modules   |
| `cli/`                     | `elph/src/cli/` (19 subcommand files) | [Elph delta] — CLI subcommands          |
| `agent/`                   | `elph/src/agent/`                     | [Partial] — pi-coding-agent equivalent  |
| `agent/runtime.rs`         | `elph/src/agent/runtime.rs`           | [Partial] — session factory             |
| `agent/session/`           | `elph/src/agent/session/`             | [Partial] — CodingAgentSession          |
| `agent/session_manager.rs` | `elph/src/agent/session_manager.rs`   | [Partial]                               |
| `agent/slash_commands.rs`  | `elph/src/agent/slash_commands.rs`    | [Partial]                               |
| `agent/mode_change.rs`     | `elph/src/agent/mode_change.rs`       | [Elph delta]                            |
| `agent/run_mode.rs`        | `elph/src/agent/run_mode.rs`          | [Partial] — non-interactive mode        |
| `agent/tool_policy.rs`     | `elph/src/agent/tool_policy.rs`       | [Elph delta]                            |
| `agent/mcp_bootstrap.rs`   | `elph/src/agent/mcp_bootstrap.rs`     | [Elph delta]                            |
| `agent/prompt/`            | `elph/src/agent/prompt/`              | [Partial]                               |
| `tui/`                     | `elph/src/tui/` (35+ files)           | [Elph delta] — iocraft-based TUI        |
| `platform/`                | `elph/src/platform/`                  | [Elph delta]                            |
| `platform/paths.rs`        | `elph/src/platform/paths.rs`          | [Elph delta] — ELPH_HOME, paths         |
| `platform/settings.rs`     | `elph/src/platform/settings.rs`       | [Elph delta] — settings merge           |
| `memory/`                  | `elph/src/memory/`                    | [Elph delta] — floppy memory            |
| `extensions/`              | `elph/src/extensions/`                | [Elph delta] — WASM extension host      |
| `codegraph/`               | `elph/src/codegraph/`                 | [Elph delta] — code review graph        |
| `worktree/`                | `elph/src/worktree/`                  | [Elph delta]                            |
| `types.rs`                 | `elph/src/types.rs`                   | [Elph delta] — AgentMode, ThinkingLevel |

## `elph-agent` (agent runtime)

**Path:** `crates/elph-agent/src/`
**Pi mapping:** `@earendil-works/pi-agent-core` → `crates/elph-agent`
**Key files:**

| Module                        | Path                                                | Status                          |
| ----------------------------- | --------------------------------------------------- | ------------------------------- |
| `lib.rs`                      | `crates/elph-agent/src/lib.rs`                      | [Parity] — re-exports           |
| `agent/`                      | `crates/elph-agent/src/agent/`                      | [Parity] — Agent struct, events |
| `agent/harness/`              | `crates/elph-agent/src/agent/harness/`              | [Parity] — AgentHarness         |
| `agent/harness/mod.rs`        | `crates/elph-agent/src/agent/harness/mod.rs`        | [Parity]                        |
| `agent/harness/prompt_ops.rs` | `crates/elph-agent/src/agent/harness/prompt_ops.rs` | [Parity]                        |
| `agent/harness/run_loop/`     | `crates/elph-agent/src/agent/harness/run_loop/`     | [Parity]                        |
| `agent/subagent/`             | `crates/elph-agent/src/agent/subagent/`             | [Elph delta]                    |
| `runtime/`                    | `crates/elph-agent/src/runtime/`                    | [Parity]                        |
| `runtime/run_loop.rs`         | `crates/elph-agent/src/runtime/run_loop.rs`         | [Parity]                        |
| `runtime/exec/`               | `crates/elph-agent/src/runtime/exec/`               | [Parity]                        |
| `runtime/local_env/`          | `crates/elph-agent/src/runtime/local_env/`          | [Parity]                        |
| `compaction/`                 | `crates/elph-agent/src/compaction/`                 | [Parity]                        |
| `session/`                    | `crates/elph-agent/src/session/`                    | [Parity]                        |
| `tools/`                      | `crates/elph-agent/src/tools/`                      | [Elph delta] — product tools    |
| `tools/types.rs`              | `crates/elph-agent/src/tools/types.rs`              | [Elph delta]                    |
| `tools/mcp/`                  | `crates/elph-agent/src/tools/mcp/`                  | [Elph delta] — MCP client       |
| `tools/web/`                  | `crates/elph-agent/src/tools/web/`                  | [Elph delta]                    |
| `skills/`                     | `crates/elph-agent/src/skills/`                     | [Elph delta] — SKILL.md system  |
| `goals/`                      | `crates/elph-agent/src/goals/`                      | [Elph delta]                    |
| `plugins/`                    | `crates/elph-agent/src/plugins/`                    | [Elph delta] — WASM plugins     |
| `collaboration/`              | `crates/elph-agent/src/collaboration/`              | [Elph delta]                    |
| `datastore/`                  | `crates/elph-agent/src/datastore/`                  | [Elph delta]                    |
| `prompt/`                     | `crates/elph-agent/src/prompt/`                     | [Elph delta] — TOON encoding    |
| `builder.rs`                  | `crates/elph-agent/src/builder.rs`                  | [Elph delta]                    |

## `elph-ai` (LLM API layer)

**Path:** `crates/elph-ai/src/`
**Pi mapping:** `@earendil-works/pi-ai` → `crates/elph-ai`
**Key files:**

| Module                      | Path                                           | Status                           |
| --------------------------- | ---------------------------------------------- | -------------------------------- |
| `lib.rs`                    | `crates/elph-ai/src/lib.rs`                    | [Parity]                         |
| `api/`                      | `crates/elph-ai/src/api/`                      | [Parity] — provider API adapters |
| `api/anthropic_messages.rs` | `crates/elph-ai/src/api/anthropic_messages.rs` | [Parity]                         |
| `api/bedrock_converse.rs`   | `crates/elph-ai/src/api/bedrock_converse.rs`   | [Parity]                         |
| `api/transform_messages.rs` | `crates/elph-ai/src/api/transform_messages.rs` | [Parity]                         |
| `auth/`                     | `crates/elph-ai/src/auth/`                     | [Parity]                         |
| `auth/credential_store.rs`  | `crates/elph-ai/src/auth/credential_store.rs`  | [Parity]                         |
| `auth/models_store.rs`      | `crates/elph-ai/src/auth/models_store.rs`      | [Parity]                         |
| `auth/oauth/`               | `crates/elph-ai/src/auth/oauth/`               | [Parity]                         |
| `auth/resolve.rs`           | `crates/elph-ai/src/auth/resolve.rs`           | [Parity]                         |
| `providers/`                | `crates/elph-ai/src/providers/`                | [Parity]                         |
| `providers/builtin.rs`      | `crates/elph-ai/src/providers/builtin.rs`      | [Parity] + [Elph delta]          |
| `providers/adapter.rs`      | `crates/elph-ai/src/providers/adapter.rs`      | [Parity]                         |
| `providers/faux/`           | `crates/elph-ai/src/providers/faux/`           | [Parity] — test provider         |
| `models/`                   | `crates/elph-ai/src/models/`                   | [Parity]                         |
| `types/`                    | `crates/elph-ai/src/types/`                    | [Parity]                         |
| `utils/`                    | `crates/elph-ai/src/utils/`                    | [Parity]                         |
| `utils/retry.rs`            | `crates/elph-ai/src/utils/retry.rs`            | [Parity]                         |
| `utils/deferred_tools.rs`   | `crates/elph-ai/src/utils/deferred_tools.rs`   | [Parity]                         |
| `utils/text.rs`             | `crates/elph-ai/src/utils/text.rs`             | [Parity]                         |
| `images/`                   | `crates/elph-ai/src/images/`                   | [Parity]                         |
| `resilience/`               | `crates/elph-ai/src/resilience/`               | [Parity]                         |
| `session_resources.rs`      | `crates/elph-ai/src/session_resources.rs`      | [Parity]                         |

## `elph-tui` (TUI components)

**Path:** `crates/elph-tui/src/`
**Status:** [Elph delta] — no pi equivalent

| Module            | Path                                                                                      |
| ----------------- | ----------------------------------------------------------------------------------------- |
| `lib.rs`          | `crates/elph-tui/src/lib.rs`                                                              |
| `components/`     | `crates/elph-tui/src/components/` (select, card, code, diff, markdown, scroll, tab, etc.) |
| `text_editing/`   | `crates/elph-tui/src/text_editing/`                                                       |
| `slash_palette/`  | `crates/elph-tui/src/slash_palette/`                                                      |
| `theme_config.rs` | `crates/elph-tui/src/theme_config.rs`                                                     |
| `loader.rs`       | `crates/elph-tui/src/loader.rs`                                                           |
| `clipboard.rs`    | `crates/elph-tui/src/clipboard.rs`                                                        |
| `types.rs`        | `crates/elph-tui/src/types.rs`                                                            |

## `elph-exec` (shell execution)

**Path:** `crates/elph-exec/src/`
**Status:** [Elph delta]

| Module      | Path                             |
| ----------- | -------------------------------- |
| `lib.rs`    | `crates/elph-exec/src/lib.rs`    |
| `shell.rs`  | `crates/elph-exec/src/shell.rs`  |
| `types.rs`  | `crates/elph-exec/src/types.rs`  |
| `error.rs`  | `crates/elph-exec/src/error.rs`  |
| `output.rs` | `crates/elph-exec/src/output.rs` |
| `pty/`      | `crates/elph-exec/src/pty/`      |

## `floppy` (memory)

**Path:** `crates/floppy/src/`
**Status:** [Elph delta] — port of memelord SDK

| Module          | Path                              |
| --------------- | --------------------------------- |
| `lib.rs`        | `crates/floppy/src/lib.rs`        |
| `builder.rs`    | `crates/floppy/src/builder.rs`    |
| `embed.rs`      | `crates/floppy/src/embed.rs`      |
| `migrations.rs` | `crates/floppy/src/migrations.rs` |
| `paths.rs`      | `crates/floppy/src/paths.rs`      |
| `query/`        | `crates/floppy/src/query/`        |
| `store/`        | `crates/floppy/src/store/`        |
| `scoring.rs`    | `crates/floppy/src/scoring.rs`    |
| `types/`        | `crates/floppy/src/types/`        |

## Skeleton crates

| Crate          | Path                             | Status            |
| -------------- | -------------------------------- | ----------------- |
| `elph-cron`    | `crates/elph-cron/src/lib.rs`    | Empty placeholder |
| `elph-sandbox` | `crates/elph-sandbox/src/lib.rs` | Empty placeholder |
| `elph-swarm`   | `crates/elph-swarm/src/lib.rs`   | Empty placeholder |
