---
type: Guide
title: Agent Runtime
description: The elph-agent turn loop, harness, session management, tool execution, MCP integration, subagents, goals, and compaction.
tags: [agent, runtime, elph-agent, sessions, tools]
resource: /crates/elph-agent/
---

# Agent Runtime

The agent runtime lives in `/crates/elph-agent/` and provides the core turn cycle, tool execution, session management, and agent orchestration. The `elph` binary wires it into a coding-agent product via `/elph/src/agent/runtime.rs`.

## Turn cycle

The core loop (`runtime/run_loop.rs`) follows Codex-style agent flow:

```
User message
  → assemble system prompt + conversation history + resources
  → stream completion from LLM provider (with tool schemas)
  → if tool calls:
      → check approval policy (risky tools need user OK)
      → execute tools
      → append results to history
      → repeat (model calls tools again or responds)
  → when model stops calling tools:
      → persist conversation
      → emit turn_done event
```

**Source:** `crates/elph-agent/src/runtime/run_loop.rs`, `crates/elph-agent/src/runtime/loop_config.rs`

## AgentHarness

The harness (`agent/harness/`) is the top-level orchestrator that wraps the runtime loop with:

- **Session persistence** — save/load conversation history
- **Hook system** — lifecycle hooks (`hooks.rs`)
- **Compaction** — automatic history compaction when limits are reached
- **System prompt management** — prompt assembly with resources and skills
- **Tree navigation** — branch/fork support for session trees
- **Plan mode** — read-only collaboration mode

**Source:** `crates/elph-agent/src/agent/harness/mod.rs`

## Sessions

Three storage backends (`session/`):

| Backend                  | When used          | Key features                          |
| ------------------------ | ------------------ | ------------------------------------- |
| `InMemorySessionStorage` | Tests, ephemeral   | No persistence                        |
| `SessionDirStorage`      | Production default | Per-session directory, JSON metadata  |
| `TursoSessionStorage`    | Optional           | SQLite/Turso for multi-process access |

Sessions support:

- **Fork/branch** — create child sessions from any point in history
- **Resume** — reload a session by ID
- **Metadata** — session ID (kalid), created/updated timestamps, model info

**Source:** `crates/elph-agent/src/session/`

## Built-in tools

Tools are feature-gated (`crates/elph-agent/Cargo.toml`). The `BuiltinToolsBuilder` in `builder.rs` creates the tool catalog.

| Tool group    | Features              | Key tools                                                                          |
| ------------- | --------------------- | ---------------------------------------------------------------------------------- |
| Core          | `tools-core`          | `read`, `write`, `edit`, `grep`, `glob`, `shell_exec`, `list_files`, `search_code` |
| Edit          | `tools-edit`          | `file_edit` (sed-like), `file_insert`, `file_write`                                |
| Search        | `tools-search`        | `search_code`, `grep`, `fff_search`                                                |
| Web           | `tools-web`           | `web_fetch`, `web_search`                                                          |
| Collaboration | `tools-collaboration` | `ask_user`, `create_subagent`, `delegate_task`                                     |
| MCP           | `mcp`                 | `mcp_{server}__{tool}` auto-registered tools                                       |

**Source:** `crates/elph-agent/src/tools/mod.rs`, `crates/elph-agent/src/builder.rs`

### Tool approval policy

Tools are classified into permission levels. The `ToolPolicy` (`/elph/src/agent/tool_policy.rs`) maps the four [agent modes](../quickstart.md#agent-modes) to approval rules.

| Mode  | Risky tools                |
| ----- | -------------------------- |
| Build | Require approval           |
| Plan  | Read-only (no mutations)   |
| Ask   | No tools except `ask_user` |
| Brave | All auto-approved          |

**Source:** `/elph/src/agent/tool_policy.rs`

## MCP integration

See [mcp-integration.md](mcp-integration.md) for full details.

The MCP tool registry (`tools/mcp/`) supports:

- **Transport**: stdio child process, HTTP (Streamable HTTP), SSE
- **Auth**: Bearer token (env or inline), OAuth with PKCE, encrypted `auth.json`
- **Policy engine**: per-server tool allow/deny/approval rules
- **Hot reload**: `elph mcp reload` without restart
- **Notifications**: server-initiated notifications via the registry

**Source:** `crates/elph-agent/src/tools/mcp/`

## Subagents

Subagent orchestration (`agent/subagent/`) supports:

- **Depth-3 nesting** — subagents can spawn their own subagents (max 3 levels)
- **Shared registry** — access to parent's tool registry
- **Graph edges** — tracking parent-child relationships
- **Session isolation** — each subagent gets its own history slice
- **Exit summaries** — Codex-style completion summary

**Source:** `crates/elph-agent/src/agent/subagent/`

## Goals

Codex-style goal/todo system (`goals/`):

- **Lifecycle**: active → completed / failed / cancelled
- **Budgets**: token limit, turn limit, time limit
- **Store**: persist goals alongside session data
- **Slash commands**: `/goal` in the TUI

**Source:** `crates/elph-agent/src/goals/`

## Compaction

History compaction (`compaction/`) manages context window limits:

- **Triggers**: ~32 messages or ~512KB
- **Strategies**: branch summarization, oldest message pruning
- **Token estimation**: estimates token counts for messages
- **Branch management**: merges branched conversations when appropriate

**Source:** `crates/elph-agent/src/compaction/`

## Collaboration modes

The collaboration module (`collaboration/`) implements plan/implement workflows:

- **Plan mode** — model creates a plan without executing tools
- **Implement mode** — model executes tools according to the plan
- **Plan confirmation** — verification before implementation
- **Tool exposure policies** — what tools are visible in each mode

**Source:** `crates/elph-agent/src/collaboration/`

## Agent builder

`AgentBuilder` (`builder.rs`) is the main entry point for constructing agents:

```rust
let agent = AgentBuilder::default()
    .with_logging_logforth(...)
    .with_builtin_tools(BuiltinToolsBuilder::all(env))
    .build()?;
```

`BuiltinToolsBuilder` provides `.all(env)`, `.core(env)`, or granular per-tool-group constructors.

**Source:** `crates/elph-agent/src/builder.rs`

## TOON prompt encoding

An optional compressed JSON format for tool results to reduce token consumption:

- Controlled by `ELPH_PROMPT_ENCODING` env var (`off`, `toon`, `auto`)
- Delimiter configurable via `ELPH_PROMPT_ENCODING_DELIMITER` and `ELPH_PROMPT_ENCODING_TABULAR_DELIMITER`
- Minimum encoding threshold: `ELPH_PROMPT_ENCODING_MIN_BYTES` (default 2048)

**Source:** `crates/elph-agent/docs/prompt-encoding.md`

## Changing the agent runtime

When modifying the agent runtime, relevant test locations:

- Unit tests — colocated with source
- Integration tests — `crates/elph-agent/tests/` (see [testing.md](testing.md))
- Key areas: session persistence, tool execution, MCP connectivity, compaction, subagent lifecycle

Run: `make test` or `cargo nextest run -p elph-agent`
