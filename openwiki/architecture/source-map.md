---
type: Source Map
title: Elph Crate-by-Crate Source Map
description: Module map for every crate in the Elph workspace, noting which modules are pi ports vs elph-only
tags: [source-map, modules, pi-port, elph-only]
---

# Source Map

Crate-by-crate module map with file paths, noting pi port origins vs Elph-only extensions.

## `elph` (product binary + library)

**Path:** `crates/coding-agent/src/`
**Pi mapping:** `@earendil-works/pi-coding-agent` → `crates/coding-agent/`
**Status:** Moved from `elph/` to `crates/coding-agent/` (commit `dc726d9`)
**Key files:**

| Module                     | Path                                                 | Status                                                                                      |
| -------------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `main.rs`                  | `crates/coding-agent/src/main.rs`                    | [Elph delta] — CLI entry via clap                                                           |
| `lib.rs`                   | `crates/coding-agent/src/lib.rs`                     | [Elph delta] — re-exports all modules                                                       |
| `cli/`                     | `crates/coding-agent/src/cli/` (18 subcommand files) | [Elph delta] — CLI subcommands                                                              |
| `agent/`                   | `crates/coding-agent/src/agent/`                     | [Partial] — pi-coding-agent equivalent                                                      |
| `agent/runtime.rs`         | `crates/coding-agent/src/agent/runtime.rs`           | [Partial] — session factory                                                                 |
| `agent/session/`           | `crates/coding-agent/src/agent/session/`             | [Partial] — CodingAgentSession                                                              |
| `agent/session_manager.rs` | `crates/coding-agent/src/agent/session_manager.rs`   | [Partial]                                                                                   |
| `agent/slash_commands.rs`  | `crates/coding-agent/src/agent/slash_commands.rs`    | [Partial]                                                                                   |
| `agent/slash_misc.rs`      | `crates/coding-agent/src/agent/slash_misc.rs`        | [Elph delta] — /resume, /tree, /fork, /clone, /export, /import, /workers, /settings, /trust |
| `agent/mode_change.rs`     | `crates/coding-agent/src/agent/mode_change.rs`       | [Elph delta]                                                                                |
| `agent/run_mode.rs`        | `crates/coding-agent/src/agent/run_mode.rs`          | [Partial] — non-interactive mode                                                            |
| `agent/headless_status.rs` | `crates/coding-agent/src/agent/headless_status.rs`   | [Elph delta] — braille spinner                                                              |
| `agent/pretty_markdown.rs` | `crates/coding-agent/src/agent/pretty_markdown.rs`   | [Elph delta] — streaming markdown→ANSI                                                      |
| `agent/tool_policy.rs`     | `crates/coding-agent/src/agent/tool_policy.rs`       | [Elph delta]                                                                                |
| `agent/mcp_bootstrap.rs`   | `crates/coding-agent/src/agent/mcp_bootstrap.rs`     | [Elph delta]                                                                                |
| `agent/prompt/`            | `crates/coding-agent/src/agent/prompt/`              | [Partial]                                                                                   |
| `agent/handover/`          | `crates/coding-agent/src/agent/handover/`            | [Elph delta] — Claude/Codex handover                                                        |
| `agent/aside.rs`           | `crates/coding-agent/src/agent/aside.rs`             | [Elph delta] — inline side questions                                                        |
| `agent/worker_runtime.rs`  | `crates/coding-agent/src/agent/worker_runtime.rs`    | [Elph delta] — multi-worker runtime                                                         |
| `tui/`                     | `crates/coding-agent/src/tui/` (40+ files)           | [Elph delta] — iocraft-based TUI                                                            |
| `platform/`                | `crates/coding-agent/src/platform/`                  | [Elph delta]                                                                                |
| `platform/paths.rs`        | `crates/coding-agent/src/platform/paths.rs`          | [Elph delta] — ELPH_HOME, paths                                                             |
| `platform/settings.rs`     | `crates/coding-agent/src/platform/settings.rs`       | [Elph delta] — settings merge                                                               |
| `memory/`                  | `crates/coding-agent/src/memory/`                    | [Elph delta] — floppy memory                                                                |
| `codegraph/`               | `crates/coding-agent/src/codegraph/`                 | [Elph delta] — code review graph                                                            |
| `extensions/`              | `crates/coding-agent/src/extensions/`                | [Elph delta] — WASM extension host                                                          |
| `command/`                 | `crates/coding-agent/src/command/`                   | [Elph delta] — shell command helpers                                                        |
| `types/`                   | `crates/coding-agent/src/types.rs`                   | [Elph delta] — AgentMode, ThinkingLevel                                                     |
| `utils/`                   | `crates/coding-agent/src/utils/`                     | [Elph delta] — shared utilities                                                             |
| `worktree/`                | `crates/coding-agent/src/worktree/`                  | [Elph delta] — git worktree management                                                      |

## `elph-agent` (agent runtime)

**Path:** `crates/elph-agent/src/`
**Pi mapping:** `@earendil-works/pi-agent-core` → `crates/elph-agent`
**Key files:**

| Module                            | Path                                                    | Status                                                                                                                                                                                                              |
| --------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib.rs`                          | `crates/elph-agent/src/lib.rs`                          | [Parity] — re-exports                                                                                                                                                                                               |
| `agent/`                          | `crates/elph-agent/src/agent/`                          | [Parity] — Agent struct, events                                                                                                                                                                                     |
| `agent/harness/`                  | `crates/elph-agent/src/agent/harness/`                  | [Parity] — AgentHarness                                                                                                                                                                                             |
| `agent/harness/mod.rs`            | `crates/elph-agent/src/agent/harness/mod.rs`            | [Parity]                                                                                                                                                                                                            |
| `agent/harness/prompt_ops.rs`     | `crates/elph-agent/src/agent/harness/prompt_ops.rs`     | [Parity]                                                                                                                                                                                                            |
| `agent/harness/run_loop/`         | `crates/elph-agent/src/agent/harness/run_loop/`         | [Parity]                                                                                                                                                                                                            |
| `agent/subagent/`                 | `crates/elph-agent/src/agent/subagent/`                 | [Elph delta]                                                                                                                                                                                                        |
| `agent/subagent/types.rs`         | `crates/elph-agent/src/agent/subagent/types.rs`         | [Elph delta] — `SubagentOutput`, `SubagentBootstrap.database`/`outputs_root`, `persist` module (`output.md`, `events.jsonl`, `meta.json`)                                                                           |
| `agent/subagent/control.rs`       | `crates/elph-agent/src/agent/subagent/control.rs`       | [Elph delta] — `wait_agent_for_output()`, `refresh_record_output()`, `TurnGuard`                                                                                                                                    |
| `agent/subagent/harness.rs`       | `crates/elph-agent/src/agent/subagent/harness.rs`       | [Elph delta] — output tracking, `TurnGuard`, `wait_for_turn()`                                                                                                                                                      |
| `agent/subagent/registry.rs`      | `crates/elph-agent/src/agent/subagent/registry.rs`      | [Elph delta] — `agents_mut()` for coherent updates                                                                                                                                                                  |
| `runtime/`                        | `crates/elph-agent/src/runtime/`                        | [Parity]                                                                                                                                                                                                            |
| `runtime/run_loop.rs`             | `crates/elph-agent/src/runtime/run_loop.rs`             | [Parity]                                                                                                                                                                                                            |
| `runtime/exec/`                   | `crates/elph-agent/src/runtime/exec/`                   | [Parity]                                                                                                                                                                                                            |
| `runtime/local_env/`              | `crates/elph-agent/src/runtime/local_env/`              | [Parity]                                                                                                                                                                                                            |
| `exec/`                           | `crates/elph-agent/src/exec/`                           | [Elph delta] — merged from elph-exec (commit `c8f65ab`)                                                                                                                                                             |
| `compaction/`                     | `crates/elph-agent/src/compaction/`                     | [Parity]                                                                                                                                                                                                            |
| `session/`                        | `crates/elph-agent/src/session/`                        | [Parity]                                                                                                                                                                                                            |
| `session/retention.rs`            | `crates/elph-agent/src/session/retention.rs`            | [Elph delta] — `RetentionPolicy`, `run_session_gc()`, `run_full_session_gc()`                                                                                                                                       |
| `turns/`                          | `crates/elph-agent/src/turns/`                          | [Elph delta] — TurnStore, TurnRecord, TurnUsage, turn accounting                                                                                                                                                    |
| `todos/`                          | `crates/elph-agent/src/todos/`                          | [Elph delta] — TodoStore, TodoTools, todo_write/todo_read agent tools                                                                                                                                               |
| `workers/`                        | `crates/elph-agent/src/workers/`                        | [Elph delta] — multi-process worker coordination (registry, leases, mailbox, path claims, tools)                                                                                                                    |
| `tools/`                          | `crates/elph-agent/src/tools/`                          | [Elph delta] — product tools                                                                                                                                                                                        |
| `tools/types.rs`                  | `crates/elph-agent/src/tools/types.rs`                  | [Elph delta]                                                                                                                                                                                                        |
| `tools/mcp/`                      | `crates/elph-agent/src/tools/mcp/`                      | [Elph delta] — MCP client                                                                                                                                                                                           |
| `tools/mcp/registry/mod.rs`       | `crates/elph-agent/src/tools/mcp/registry/mod.rs`       | [Elph delta] — `McpToolRegistry` (split from `registry.rs`, commit `45c8e6e`)                                                                                                                                       |
| `tools/mcp/registry/discovery.rs` | `crates/elph-agent/src/tools/mcp/registry/discovery.rs` | [Elph delta] — server discovery                                                                                                                                                                                     |
| `tools/mcp/registry/bridge.rs`    | `crates/elph-agent/src/tools/mcp/registry/bridge.rs`    | [Elph delta] — tool bridge to AgentTool                                                                                                                                                                             |
| `tools/web/`                      | `crates/elph-agent/src/tools/web/`                      | [Elph delta] — web_fetch, web_search, web_extract                                                                                                                                                                   |
| `skills/`                         | `crates/elph-agent/src/skills/`                         | [Elph delta] — SKILL.md system                                                                                                                                                                                      |
| `goals/`                          | `crates/elph-agent/src/goals/`                          | [Elph delta]                                                                                                                                                                                                        |
| `goals/steering.rs`               | `crates/elph-agent/src/goals/steering.rs`               | [Elph delta] — exports `BUDGET_LIMIT_PROMPT_PREFIX`, `CONTINUATION_PROMPT_PREFIX`                                                                                                                                   |
| `plugins/`                        | `crates/elph-agent/src/plugins/`                        | [Elph delta] — WASM plugins                                                                                                                                                                                         |
| `collaboration/`                  | `crates/elph-agent/src/collaboration/`                  | [Elph delta]                                                                                                                                                                                                        |
| `datastore/`                      | `crates/elph-agent/src/datastore/`                      | [Elph delta]                                                                                                                                                                                                        |
| `datastore/conn.rs`               | `crates/elph-agent/src/datastore/conn.rs`               | [Elph delta] — Turso open/connect/retry/lock-error/WAL recovery. Absorbed from removed `elph-db` crate (commit `eba87a7`). Adds `is_wal_io_err`, `database_in_use`, `clear_broken_wal_sidecars`, `open_local_with`. |
| `prompt/`                         | `crates/elph-agent/src/prompt/`                         | [Elph delta] — TOON encoding                                                                                                                                                                                        |
| `builder.rs`                      | `crates/elph-agent/src/builder.rs`                      | [Elph delta]                                                                                                                                                                                                        |
| `fs/`                             | `crates/elph-agent/src/fs/`                             | [Elph delta]                                                                                                                                                                                                        |
| `logger/`                         | `crates/elph-agent/src/logger/`                         | [Elph delta]                                                                                                                                                                                                        |
| `messages/`                       | `crates/elph-agent/src/messages/`                       | [Elph delta]                                                                                                                                                                                                        |
| `trace/`                          | `crates/elph-agent/src/trace/`                          | [Elph delta]                                                                                                                                                                                                        |
| `utils/`                          | `crates/elph-agent/src/utils/`                          | [Elph delta]                                                                                                                                                                                                        |

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

## `elph-exec` (shell execution) — merged into `elph-agent`

**Former path:** `crates/elph-exec/src/`
**Status:** [Elph delta] — merged into `crates/elph-agent/src/exec/` (commit `c8f65ab`)

The `elph-exec` crate was absorbed into `elph-agent` as `crate::exec`. Key modules:

| Module      | Path                              |
| ----------- | --------------------------------- |
| `exec/`     | `crates/elph-agent/src/exec/`     |
| `exec/pty/` | `crates/elph-agent/src/exec/pty/` |

Public API re-exported from `elph_agent`:

- `exec_shell_command()`, `resolve_shell()`, `ShellConfig`, `ExecError`, `ExecErrorCode`
- `PtySize`, `open_pty()` (unix only)

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

## `elph-db` (shared Turso helpers) — **REMOVED**

**Former path:** `crates/elph-db/src/`
**Status:** [Elph delta] — removed in commit `eba87a7`. Its open/connect/retry/lock-error/WAL recovery helpers were absorbed into `crates/elph-agent/src/datastore/conn.rs`.

The `elph-db` crate was a single-file crate providing shared Turso (local SQLite) open/connect/retry/lock-error helpers. It was eliminated to avoid a separate crate for a few hundred lines. The replacement lives in `crates/elph-agent/src/datastore/conn.rs` with the same functions (`is_lock_err`, `cleanup_stale_shared_memory`, `open_local`, `connect`, `with_conn`) plus new ones (`is_wal_io_err`, `database_in_use`, `clear_broken_wal_sidecars`, `open_local_with`). The `SubagentBootstrap.database` field (`Arc<turso::Database>`) now allows sharing an already-open database handle instead of opening independently.

## `rendown` (markdown renderer)

**Path:** `crates/rendown/src/`
**Status:** [Elph delta] — streaming markdown renderer, excluded from workspace

| Module   | Path                        |
| -------- | --------------------------- |
| `lib.rs` | `crates/rendown/src/lib.rs` |

## Skeleton crates

| Crate          | Path                             | Status            |
| -------------- | -------------------------------- | ----------------- |
| `elph-cron`    | `crates/elph-cron/src/lib.rs`    | Empty placeholder |
| `elph-sandbox` | `crates/elph-sandbox/src/lib.rs` | Empty placeholder |
| `elph-swarm`   | `crates/elph-swarm/src/lib.rs`   | Empty placeholder |
