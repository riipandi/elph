# Agent Runtime

The agent runtime module (`crates/elph-agent`) is the core of Elph's AI interaction engine. It provides an app-agnostic runtime with session persistence, tool execution, context compaction, goals, and subagent orchestration.

## Architecture

```
User Input → CLI/TUI
                ↓
         AgentHarness ─── Session (persistence)
                │
          Agent Loop ─── Stream completion → Tool call → Result → repeat
                │
        ┌───────┴────────┬───────────┐
        ↓                ↓           ↓
     Tools           Skills      MCP Servers
  (read_file, bash, (SKILL.md   (external tools
   edit_file, grep,  files)      via rmcp)
   web_search, ...)
```

## AgentHarness

**File**: `/crates/elph-agent/src/agent/harness/mod.rs`

`AgentHarness<S>` is the central stateful runner. It wraps the low-level agent loop with:

- **Session persistence** via pluggable `SessionStorage` (filesystem `SessionDirStorage`, `TursoSessionStorage`, or `InMemorySessionStorage`)
- **Hook system** — `HookRegistry` with typed events: `BeforeAgentStart`, `BeforeProviderRequest`, `AfterProviderResponse`, `ToolCall`, `ToolResult`, `SessionCompact`, `SessionTree`, etc.
- **Compaction** — Automatic context window management
- **Plan mode** — Model must propose a plan before using mutating tools
- **Branch summarization** — Summarize branches during tree operations
- **Subagent support** — Spawn and control subagents
- **Goals** — Persisted session objectives

### Turn lifecycle

```
User message
  → assemble system prompt + resources + history
  → stream completion (with tool schemas)
  → [tool call?]
      → approve / ask user
      → execute tool
      → optionally TOON-encode result
      → append result
      → repeat until model stops calling tools
  → persist history + emit turn_done
```

Source: `/crates/elph-agent/src/agent/harness/run_loop/`

### Key harness events

Hooks are registered through `Harness::on_<event>()` methods. Event types defined in `/crates/elph-agent/src/agent/harness/hooks.rs`:

| Event                   | Trigger                   | Use case                        |
| ----------------------- | ------------------------- | ------------------------------- |
| `BeforeAgentStart`      | Before first completion   | Inject context, modify state    |
| `BeforeProviderRequest` | Before each provider call | Modify payload, add tools       |
| `AfterProviderResponse` | After each response       | Transform response, logging     |
| `ToolCall`              | Before tool execution     | Approval gate, logging          |
| `ToolResult`            | After tool result         | Transform result, TOON encoding |
| `SessionCompact`        | After compaction          | UI update, logging              |
| `SessionTree`           | After tree operation      | UI update                       |
| `SavePoint`             | Checkpoint reached        | Custom persistence              |

## Sessions

**File**: `/crates/elph-agent/src/session/`

Sessions are tree-structured: you can fork, branch, and resume. Each session has an ID (TSID), metadata, and conversation history.

### Backends

| Backend                  | Storage                          | When to use               |
| ------------------------ | -------------------------------- | ------------------------- |
| `SessionDirStorage`      | Filesystem (`~/.elph/sessions/`) | Default local development |
| `TursoSessionStorage`    | Turso/libSQL database            | Production / shared state |
| `InMemorySessionStorage` | In-memory HashMap                | Testing                   |

### Session tree operations

- **Fork** — Create a new session from a point in history
- **Branch** — Summerize a branch for context
- **Resume** — Load a session by ID and continue
- **Export/Import** — Archive and restore sessions

Key files:

- `/crates/elph-agent/src/session/backends/session_dir/` — Filesystem backend
- `/crates/elph-agent/src/session/backends/turso/` — Turso backend
- `/crates/elph-agent/src/session/tree.rs` — `Session` struct, branch summarization
- `/crates/elph-agent/src/session/repo.rs` — Session repository (CRUD)

## Agent Loop

**File**: `/crates/elph-agent/src/runtime/`

The low-level turn runner that handles the stream → tool call → result → repeat cycle. It operates on `AgentContext` (messages + state) and `AgentLoopConfig` (tools, mode, limits).

- Default **25 tool iterations** per turn (`maxToolIterations`)
- Returns an `AgentEventStream` for real-time event consumption
- Supports `continue` mode for multi-turn interactions

Key files:

- `/crates/elph-agent/src/runtime/run_loop/` — The core loop implementation
- `/crates/elph-agent/src/runtime/stream.rs` — Event stream types
- `/crates/elph-agent/src/runtime/exec/` — Tool call dispatch, execution, and failure handling

## Compaction

**File**: `/crates/elph-agent/src/compaction/`

Automatic context window management to stay within model context limits.

### Sub-modules

| Module                    | Purpose                                                     |
| ------------------------- | ----------------------------------------------------------- |
| `compact.rs`              | Main compaction orchestrator                                |
| `summarization.rs`        | Generate summaries of conversation segments                 |
| `estimation.rs`           | Token counting, context usage estimation, cut-point finding |
| `preparation.rs`          | Prepare entries for compaction                              |
| `branch_summarization.rs` | Summarize branch context for tree operations                |
| `types.rs`                | `CompactionDetails`, `CompactionResult`                     |
| `utils.rs`                | File operation extraction, conversation serialization       |

### Auto-compact

When enabled in settings (`auto_compact_context: true`), the harness automatically compacts when context usage exceeds a threshold.

## Goals

**File**: `/crates/elph-agent/src/goals/`

Persisted session objectives with auto-steering. The agent maintains a goal stack and can be steered toward completion.

| Component             | Purpose                                                  |
| --------------------- | -------------------------------------------------------- |
| `GoalStore`           | Persisted goal storage                                   |
| `GoalRuntime`         | Goal lifecycle management (start, pause, resume, cancel) |
| `create_goal_tools`   | Agent tools for goal management                          |
| `GoalAccountingState` | Token accounting per goal                                |

Goals are managed via the `/goal` slash command with subcommands: `status`, `pause`, `resume`, `cancel`, `replace`, `next`, and direct creation.

## Subagents

**File**: `/crates/elph-agent/src/agent/subagent/`

Codex-style multi-agent orchestration. The main agent can spawn subagents for parallel tasks, then merge results.

| Component             | Purpose                                   |
| --------------------- | ----------------------------------------- |
| `AgentControl`        | Control mechanism: spawn, signal, collect |
| `SubagentHarness`     | Harness for running subagents             |
| `AgentRegistry`       | Registry of active subagents              |
| `SubagentSpawnConfig` | Configuration for spawning subagents      |
| `AgentGraphStore`     | Graph of subagent relationships           |

## Skills

**File**: `/crates/elph-agent/src/skills/`

Skills provide reusable instructions for specific tasks. They follow the [agentskills.io](https://agentskills.io) specification.

### SKILL.md format

```markdown
---
name: my-skill
description: What this skill does
license: MIT
compatibility: Requires git
allowed-tools: read grep bash
---

# Instructions

Your skill content here...
```

Skills are discovered from `.agents/skills/` directories in the project or home directory. They are loaded into the system prompt as metadata; the agent reads the full body when relevant.

Key files:

- `/crates/elph-agent/src/skills/load/mod.rs` — Skill discovery and parsing
- `/crates/elph-agent/src/skills/load/parse.rs` — Frontmatter parsing and validation
- `/crates/elph-agent/src/skills/format.rs` — Skill formatting for system prompt

## Built-in Tools

**File**: `/crates/elph-agent/src/tools/`

| Helper                        | Tools                                                                                    |
| ----------------------------- | ---------------------------------------------------------------------------------------- |
| `create_edit_tools`           | `edit_file`, `write_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `create_search_tools`         | `read_file`, `grep`, `find_path`, `list_dir`                                             |
| `create_all_tools`            | All enabled filesystem tools (11 tools)                                                  |
| `create_web_tools`            | `web_search`, `web_fetch`                                                                |
| `create_all_tools_with_web`   | Filesystem + web tools                                                                   |
| `create_collaboration_tools`  | Collaboration tools (spawn, send_message, followup_task, wait_agent, list_agents)        |
| `create_list_available_tools` | Meta-tool listing all available tools with descriptions and parameters                   |

All filesystem tools resolve paths through `ExecutionEnv` and run on blocking thread pools. Web tools do not use `ExecutionEnv`.

Key source files (each tool in its own module):

- `/crates/elph-agent/src/tools/read_file.rs`
- `/crates/elph-agent/src/tools/bash.rs`
- `/crates/elph-agent/src/tools/edit_file.rs`
- `/crates/elph-agent/src/tools/write_file.rs`
- `/crates/elph-agent/src/tools/create_dir.rs`
- `/crates/elph-agent/src/tools/copy_path.rs`
- `/crates/elph-agent/src/tools/delete_path.rs`
- `/crates/elph-agent/src/tools/move_path.rs`
- `/crates/elph-agent/src/tools/grep.rs`
- `/crates/elph-agent/src/tools/find_path.rs`
- `/crates/elph-agent/src/tools/list_dir.rs`
- `/crates/elph-agent/src/tools/web/` — `web_search.rs` and `web_fetch.rs`
- `/crates/elph-agent/src/tools/collaboration.rs` — Collaboration tools (replaces `multi_agent.rs`)
- `/crates/elph-agent/src/tools/list_available_tools.rs` — Meta-tool for tool discovery
- `/crates/elph-agent/src/tools/fff_picker.rs` — File picker integration

## Modes

**File**: `/crates/elph-agent/src/collaboration/`

| Mode        | Description                                           |
| ----------- | ----------------------------------------------------- |
| **Default** | Normal agent interaction with full tool access        |
| **Plan**    | Agent must propose a plan before using mutating tools |

`CollaborationMode` enum drives tool filtering and system prompt modifications.

## Key source files

| Concern                   | Path                                             |
| ------------------------- | ------------------------------------------------ |
| Agent harness             | `/crates/elph-agent/src/agent/harness/mod.rs`    |
| Agent harness run loop    | `/crates/elph-agent/src/agent/harness/run_loop/` |
| Agent harness hook system | `/crates/elph-agent/src/agent/harness/hooks.rs`  |
| Agent harness types       | `/crates/elph-agent/src/agent/harness/types/`    |
| Session tree              | `/crates/elph-agent/src/session/tree.rs`         |
| Session backends          | `/crates/elph-agent/src/session/backends/`       |
| Compaction                | `/crates/elph-agent/src/compaction/`             |
| Goals                     | `/crates/elph-agent/src/goals/`                  |
| Subagents                 | `/crates/elph-agent/src/agent/subagent/`         |
| Skills                    | `/crates/elph-agent/src/skills/`                 |
| Built-in tools            | `/crates/elph-agent/src/tools/`                  |
| Collaboration modes       | `/crates/elph-agent/src/collaboration/`          |
| Agent loop / runtime      | `/crates/elph-agent/src/runtime/`                |
| Types                     | `/crates/elph-agent/src/types/`                  |
| Execution env             | `/crates/elph-agent/src/runtime/local_env/`      |
| Messages                  | `/crates/elph-agent/src/messages/`               |
| Event stream              | `/crates/elph-agent/src/runtime/event_stream.rs` |
| Plugin/WASM host          | `/crates/elph-agent/src/plugins/`                |
| MCP client                | `/crates/elph-agent/src/tools/mcp/`              |

## Change guidance

- **Agent loop changes**: Test in `crates/elph-agent/tests/agent_loop.rs` and `tests/harness.rs`
- **Session changes**: Test in `crates/elph-agent/tests/session.rs` and `tests/repo.rs`
- **Compaction changes**: Test in `crates/elph-agent/tests/compaction.rs`
- **Goals changes**: Test in `crates/elph-agent/tests/goals.rs`
- **Subagent changes**: Test in `crates/elph-agent/tests/subagent.rs`
- **Tool changes**: Test in `crates/elph-agent/tests/tools_fff.rs` and `tests/web_tools.rs`
- **Skills changes**: Test in `crates/elph-agent/tests/skills.rs`
- **Configuration**: Check `HarnessOptions`, `CompactionSettings`, `SessionStorage` generics
  nStorage` generics
